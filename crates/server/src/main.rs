use std::{
    future::Future,
    pin::pin,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use tokio::{fs, select, task::JoinSet, time::sleep};

use chatbot::{Chatbot, Logger};
use miniserve::{Content, Request, Response};
use serde::{Deserialize, Serialize};
use tokio::join;

mod task;

use task::{Runner, Task};

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

struct Log {
    value: Logger,
}

impl Task for Log {
    type I = Arc<ChatData>;
    type O = ();

    async fn run(&mut self, data: Self::I) -> Self::O {
        self.value.append(data.messages.last().unwrap());
        let _ = self.value.save().await;
    }
}

struct Responses {
    value: Chatbot,
}

impl Task for Responses {
    type I = Arc<ChatData>;
    type O = Vec<String>;

    async fn run(&mut self, data: Self::I) -> Self::O {
        let fns = self.value.retrieval_documents(&data.messages);
        let mut docs_maybe = fns
            .into_iter()
            .map(fs::read_to_string)
            .collect::<JoinSet<_>>();
        let mut docs = vec![];
        while let Some(doc) = docs_maybe.join_next().await {
            docs.push(doc.unwrap().unwrap());
        }

        execute_timed(self.value.query_chat(&data.messages, &docs)).await
    }
}

// ==================
// Endpoints

async fn chat(req: Request) -> Response {
    let Request::Post(str) = req else { todo!() };

    let data = Arc::new(serde_json::from_str(&str).unwrap());

    let (responses, idx, ()) = join!(
        get_responses(Arc::clone(&data)),
        chatbot::gen_random_number(),
        log(Arc::clone(&data))
    );

    let unarc = |arc| Arc::into_inner(arc).unwrap();
    let response = match responses {
        None => unarc(data).cancelled(),
        Some(mut responses) => {
            let l = responses.len();
            let new_message = std::mem::take(&mut responses[idx % l]);
            unarc(data).success(new_message)
        }
    };

    Ok(Content::Json(serde_json::to_string(&response).unwrap()))
}

async fn cancel(_req: Request) -> Response {
    CHATBOT.cancel().await;
    Ok(Content::Json("".to_string()))
}

async fn index(_req: Request) -> Response {
    let content = include_str!("../index.html").to_string();
    Ok(Content::Html(content))
}

// ==============
// Helpers

async fn execute_timed<O>(f: impl Future<Output = O>) -> O {
    let mut fut = pin!(f);
    let now = Instant::now();
    let one_sec = Duration::from_secs(1);
    loop {
        select! {
            o = &mut fut => {
                return o;
            }
            _ = sleep(one_sec) => {
                println!("Waiting {} secs", now.elapsed().as_secs());
            }
        }
    }
}

static CHATBOT: LazyLock<Runner<Responses>> = LazyLock::new(|| {
    Runner::new(Responses {
        value: Chatbot::new(vec![
            "🫵".to_string(),
            "🫠".to_string(),
            "🤗".to_string(),
            "🫡".to_string(),
            "🤪".to_string(),
        ]),
    })
});

async fn get_responses(data: Arc<ChatData>) -> Option<Vec<String>> {
    CHATBOT.run(data).await
}

async fn log(data: Arc<ChatData>) {
    static LOGGER: LazyLock<Runner<Log>> = LazyLock::new(|| {
        Runner::new(Log {
            value: Logger::default(),
        })
    });
    LOGGER.run(data).await;
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
