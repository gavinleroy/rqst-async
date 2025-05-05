use miniserve::{http::StatusCode, Content, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

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

    let mut data: Data = serde_json::from_str(&str).map_err(|_| StatusCode::BAD_REQUEST)?;
    data.messages.push("Howdy!".to_string());
    let resp = json!(data).to_string();
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
