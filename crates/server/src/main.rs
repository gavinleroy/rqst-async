use std::sync::Arc;

use miniserve::{http::StatusCode, Content, Request, Response};
use serde::{Deserialize, Serialize};
use tokio::{join, sync::Mutex};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Data {
    messages: Vec<String>,
}

async fn chat(req: Request) -> Response {
    let str = if let Request::Post(str) = req {
        str
    } else {
        "".to_string()
    };

    let data: Data = serde_json::from_str(&str).map_err(|_| StatusCode::BAD_REQUEST)?;
    let data = Arc::new(Mutex::new(data));

    let data2 = Arc::clone(&data);
    let messages = tokio::spawn(async move {
        let data = data2.lock().await;
        chatbot::query_chat(&data.messages).await
    });
    let idx = tokio::spawn(chatbot::gen_random_number());

    let (responses, idx) = join!(messages, idx);
    let (responses, idx) = (responses.unwrap(), idx.unwrap());

    let new_message = responses[idx % responses.len()].clone();

    let mut data = data.lock().await;
    data.messages.push(new_message.clone());

    let resp = serde_json::to_string(&*data).unwrap();
    Response::Ok(Content::Json(resp))
}

async fn index(_req: Request) -> Response {
    let content = include_str!("../index.html").to_string();
    Ok(Content::Html(content))
}

#[tokio::main]
async fn main() {
    miniserve::Server::new()
        .route("/", index)
        .route("/chat", chat)
        .run()
        .await
}
