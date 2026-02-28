use axum::{Json, Router, routing};
use axum::extract::{State, Path};
use axum::response::Redirect;
use axum::http::StatusCode;
use rand;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::{SystemTime, UNIX_EPOCH};

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

const FIND_REDIRECT: &str = "
select link from links where token=$1 and $2 between \"start\" and \"end\" limit 1
";

#[tokio::main]
async fn main() {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://postgres:postgres@localhost/postgres").await.unwrap();
    sqlx::query(INIT_DB).execute(&pool).await.unwrap();

    let state = AppState{pool: pool};
    let app = Router::new()
    .route("/", routing::post(new_shortcut_handler))
    .route("/{token}", routing::get(follow_shortcut_handler))
    .with_state(state);
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

async fn new_shortcut_handler(State(state): State<AppState>, Json(payload): Json<CreateShortcut>) -> Json<NewShortcut> {
    let token = new_token();
    sqlx::query(INSERT_LINK).persistent(true).bind(&token).bind(&payload.url).bind(payload.start).bind(payload.end).execute(&state.pool).await.unwrap();
    let resp = NewShortcut {
        create: payload,
        url: format!("http://{}/{}", HOST, &token),
    };
    Json(resp)
}

async fn follow_shortcut_handler(State(state): State<AppState>, Path(token): Path<String>) -> Result<Redirect, StatusCode> {
    let now = SystemTime::now();
    let duration_since_epoch = now
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let timestamp_secs = duration_since_epoch.as_secs() as i32;
    let row: (String,) = sqlx::query_as(FIND_REDIRECT).persistent(true).bind(&token).bind(timestamp_secs).fetch_one(&state.pool).await.unwrap_or_default();
    if row.0 == "" {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(Redirect::temporary(&row.0))
    }
}
