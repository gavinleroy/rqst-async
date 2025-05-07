use std::{
    future::Future,
    pin::pin,
    sync::{Arc, LazyLock},
    time::Duration,
};

use tokio::{fs, select, sync::mpsc, task::JoinSet, time::sleep};

use chatbot::Chatbot;
use miniserve::{Content, Request, Response};
use serde::{Deserialize, Serialize};
use tokio::{join, sync::oneshot};

#[derive(Debug, Clone, Deserialize)]
struct ChatData {
    messages: Vec<String>,
}

impl ChatData {
    fn success(mut self, msg: String) -> ChatResponse {
        self.messages.push(msg);
        ChatResponse::Success {
            messages: self.messages,
        }
    }

    fn cancelled(self) -> ChatResponse {
        ChatResponse::Cancelled
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum ChatResponse {
    Cancelled,
    Success { messages: Vec<String> },
}

async fn execute_timed<O>(f: impl Future<Output = O>) -> O {
    let mut fut = pin!(f);
    let mut counter = 0;
    let one_sec = Duration::from_secs(1);
    loop {
        select! {
            o = &mut fut => {
                return o;
            }
            _ = sleep(one_sec) => {
                counter += 1;
                println!("Waiting {counter} secs");
            }
        }
    }
}

async fn chat(req: Request) -> Response {
    let Request::Post(str) = req else { todo!() };

    let data = Arc::new(serde_json::from_str(&str).unwrap());
    let data2 = Arc::clone(&data);

    let messages = tokio::spawn(get_responses(data2));
    let idx = tokio::spawn(execute_timed(chatbot::gen_random_number()));

    let (responses, idx) = join!(messages, idx);

    let unarc = |arc| Arc::into_inner(arc).unwrap();
    let response = match responses.unwrap() {
        ChatResponses::Cancelled => unarc(data).cancelled(),
        ChatResponses::Messages(mut responses) => {
            let l = responses.len();
            let new_message = std::mem::take(&mut responses[idx.unwrap() % l]);
            unarc(data).success(new_message)
        }
    };

    Ok(Content::Json(serde_json::to_string(&response).unwrap()))
}

async fn cancel(_req: Request) -> Response {
    CHATBOT.cancel.send(()).await.unwrap();
    Ok(Content::Json("".to_string()))
}

async fn index(_req: Request) -> Response {
    let content = include_str!("../index.html").to_string();
    Ok(Content::Html(content))
}

#[derive(Debug, Clone)]
enum ChatResponses {
    Cancelled,
    Messages(Vec<String>),
}

type QueryData = (Arc<ChatData>, oneshot::Sender<ChatResponses>);

struct SendChannels {
    responses: mpsc::Sender<QueryData>,
    cancel: mpsc::Sender<()>,
}

static CHATBOT: LazyLock<SendChannels> = LazyLock::new(|| {
    let mut chatbot = Chatbot::new(vec![
        "🫵".to_string(),
        "🫠".to_string(),
        "🤗".to_string(),
        "🫡".to_string(),
        "🤪".to_string(),
    ]);

    let (tx, mut rx) = mpsc::channel::<QueryData>(100);
    let (ctx, mut crx) = mpsc::channel(1);

    tokio::spawn(async move {
        while let Some((data, ret)) = rx.recv().await {
            let messages: &[String] = &data.messages;
            let fns = chatbot.retrieval_documents(messages);

            let mut docs_maybe = fns
                .into_iter()
                .map(fs::read_to_string)
                .collect::<JoinSet<_>>();
            let mut docs = vec![];
            while let Some(doc) = docs_maybe.join_next().await {
                docs.push(doc.unwrap().unwrap());
            }

            let mut query_fut = pin!(execute_timed(chatbot.query_chat(messages, &docs)));
            let mut cancel_fut = pin!(crx.recv());

            tokio::select! {
                responses = &mut query_fut  => {
                    ret.send(ChatResponses::Messages(responses)).unwrap();
                }
                _ = &mut cancel_fut  => {
                    ret.send(ChatResponses::Cancelled).unwrap();
                }
            }
        }
    });

    SendChannels {
        responses: tx,
        cancel: ctx,
    }
});

async fn get_responses(messages: Arc<ChatData>) -> ChatResponses {
    let (tx, rx) = oneshot::channel();
    CHATBOT.responses.send((messages, tx)).await.unwrap();
    rx.await.unwrap()
}

#[tokio::main]
async fn main() {
    miniserve::Server::new()
        .route("/", index)
        .route("/chat", chat)
        .route("/cancel", cancel)
        .run()
        .await
}
