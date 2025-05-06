use std::sync::{Arc, LazyLock};

use tokio::{fs, sync::mpsc, task::JoinSet};

use chatbot::Chatbot;
use miniserve::{Content, Request, Response};
use serde::{Deserialize, Serialize};
use tokio::{join, sync::oneshot};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Data {
    messages: Vec<String>,
}

async fn chat(req: Request) -> Response {
    let Request::Post(str) = req else { todo!() };

    let data: Data = serde_json::from_str(&str).unwrap();
    let mut data = Arc::new(data);

    let data2 = Arc::clone(&data);
    let messages = tokio::spawn(async move { get_responses(data2).await });
    let idx = tokio::spawn(chatbot::gen_random_number());

    let (responses, idx) = join!(messages, idx);
    let (responses, idx) = (responses.unwrap(), idx.unwrap());

    let new_message = responses[idx % responses.len()].clone();

    Arc::get_mut(&mut data)
        .unwrap()
        .messages
        .push(new_message.clone());

    let resp = serde_json::to_string(&*data).unwrap();
    Response::Ok(Content::Json(resp))
}

async fn index(_req: Request) -> Response {
    let content = include_str!("../index.html").to_string();
    Ok(Content::Html(content))
}

async fn get_responses(messages: Arc<Data>) -> Vec<String> {
    type T = (Arc<Data>, oneshot::Sender<Vec<String>>);
    static SEND: LazyLock<mpsc::Sender<T>> = LazyLock::new(|| {
        let mut chatbot = Chatbot::new(vec![
            "🫵".to_string(),
            "🫠".to_string(),
            "🤗".to_string(),
            "🫡".to_string(),
            "🤪".to_string(),
        ]);

        let (tx, mut rx): (mpsc::Sender<T>, mpsc::Receiver<T>) = mpsc::channel(100);
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

                let responses = chatbot.query_chat(messages, &docs).await;
                ret.send(responses).unwrap();
            }
        });

        tx
    });

    let (tx, rx) = oneshot::channel();
    SEND.send((messages, tx)).await.unwrap();
    rx.await.unwrap()
}

#[tokio::main]
async fn main() {
    miniserve::Server::new()
        .route("/", index)
        .route("/chat", chat)
        .run()
        .await
}
