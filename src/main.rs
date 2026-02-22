use axum::{Json, Router, routing};
use rand;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::fmt;

const HOST: &str = "localhost:3000";

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", routing::post(handler));
    println!("router created");
    let listener = tokio::net::TcpListener::bind(HOST).await.unwrap();
    println!("serving http://{}/", HOST);
    axum::serve(listener, app).await.unwrap();
}

fn new_token() -> String {
    let rng = rand::rng();
    let chars: String = rng
        .sample_iter(&rand::distr::Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    chars
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateShortcut {
    url: String,
    start: u32,
    end: u32,
    token: Option<String>,
}

impl fmt::Display for CreateShortcut {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "CreateShortcut {{ url: {}, start: {}, end: {}, token: {:?} }}",
            self.url, self.start, self.end, self.token
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewShortcut {
    create: CreateShortcut,
    url: String,
}

impl fmt::Display for NewShortcut {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "CreateShortcut {{ create: {}, url: {} }}",
            self.create, self.url
        )
    }
}

async fn handler(Json(payload): Json<CreateShortcut>) -> Json<NewShortcut> {
    println!("request payload: {}", payload);
    let l = new_token();
    let resp = NewShortcut {
        create: payload,
        url: l,
    };
    Json(resp)
}
