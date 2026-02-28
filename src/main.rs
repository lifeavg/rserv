use axum::{Json, Router, routing};
use axum::extract::State;
use rand;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

const HOST: &str = "localhost:3000";

const INIT_DB: &str = "
create table if not exists links (
    token char(8) primary key,
    link varchar(2048) not null,
    \"start\" integer not null,
    \"end\" integer not null
)
";

const INSERT_LINK: &str = "
insert into links (token, link, \"start\", \"end\") values ($1, $2, $3, $4)
";

#[tokio::main]
async fn main() {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://postgres:postgres@localhost/postgres").await.unwrap();
    sqlx::query(INIT_DB).execute(&pool).await.unwrap();

    let state = AppState{pool: pool};
    let app = Router::new().route("/", routing::post(handler)).with_state(state);
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

#[derive(Clone)]
struct AppState {
    pool: PgPool
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateShortcut {
    url: String,
    start: i32,
    end: i32,
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

async fn handler(State(state): State<AppState>, Json(payload): Json<CreateShortcut>) -> Json<NewShortcut> {
    println!("request payload: {}", payload);
    let token = new_token();
    sqlx::query(INSERT_LINK).persistent(true).bind(&token).bind(&payload.url).bind(payload.start).bind(payload.end).execute(&state.pool).await.unwrap();
    let resp = NewShortcut {
        create: payload,
        url: format!("http://{}/{}", HOST, &token),
    };
    Json(resp)
}
