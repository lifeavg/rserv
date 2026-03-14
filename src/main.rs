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
use redis::{AsyncCommands, Client};
use log;
use env_logger::Env;

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
select link, \"end\" from links where token=$1 and $2 between \"start\" and \"end\" limit 1
";

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().filter_or("SHORTCUT_LOG", "info")).init();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect("postgres://postgres:postgres@localhost/postgres").await.unwrap_or_else(|e| {
            log::error!("Failed to connect to database: {}", e);
            panic!("Exit");
        });
    sqlx::query(INIT_DB).execute(&pool).await.unwrap_or_else(|e| {
        log::error!("Failed to initialize database: {}", e);
        panic!("Exit");
    });

    let redis_client = redis::Client::open("redis://localhost/").unwrap_or_else(|e| {
        log::error!("Failed to connect to Redis: {}", e);
        panic!("Exit");
    });

    let state = AppState{pool: pool, redis: redis_client};
    let app = Router::new()
    .route("/", routing::post(new_shortcut_handler))
    .route("/{token}", routing::get(follow_shortcut_handler))
    .with_state(state);
    log::info!("router created");
    let listener = tokio::net::TcpListener::bind(HOST).await.unwrap_or_else(|e| {
        log::error!("Failed to bind host: {}", e);
        panic!("Exit");
    });
    log::info!("serving http://{}/", HOST);
    axum::serve(listener, app).await.unwrap_or_else(|e| {
        log::error!("Failed to start server: {}", e);
        panic!("Exit");
    });
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
    pool: PgPool,
    redis: Client
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

async fn new_shortcut_handler(State(state): State<AppState>, Json(payload): Json<CreateShortcut>) ->  Result<Json<NewShortcut>, StatusCode> {
    let token = new_token();
    sqlx::query(INSERT_LINK).persistent(true).bind(&token).bind(&payload.url).bind(payload.start).bind(payload.end).execute(&state.pool).await.map_err(|e| {
        log::error!("Failed to save shortcut to database token={}, link={} : {}", &token, &payload.url, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let resp = NewShortcut {
        create: payload,
        url: format!("http://{}/{}", HOST, &token),
    };
    log::info!("Created new shortcut token={}, link={}", &token, &resp.create.url);
    Ok(Json(resp))
}

async fn follow_shortcut_handler(State(state): State<AppState>, Path(token): Path<String>) -> Result<Redirect, StatusCode> {
    match state.redis.get_multiplexed_async_connection().await {
        Ok(mut conn) => {
            match conn.get(&token).await {
                Result::<String, redis::RedisError>::Ok(r) => {
                    log::info!("Responded from cache token={} to link={}", &token, &r);
                    return Ok(Redirect::temporary(&r.as_str()));
                },
                Err(e) => {
                    log::info!("No cached link for token={}", &token);
                    drop(e);
                    let now = SystemTime::now();
                    let duration_since_epoch = now
                        .duration_since(UNIX_EPOCH)
                        .expect("Time went backwards");
                    let timestamp_secs = duration_since_epoch.as_secs() as i32;
                    match sqlx::query_as(FIND_REDIRECT).persistent(true).bind(&token).bind(timestamp_secs).fetch_one(&state.pool).await {
                        Result::<(String , i32), _>::Ok(row) => {
                            let ttl = row.1 - timestamp_secs;
                            match  conn.set_ex(&token, &row.0,ttl as u64).await {
                                Result::<String, redis::RedisError>::Ok(_) => {
                                    log::info!("Cached link={} for token={}", &row.0, &token);
                                },
                                Err(e) => {
                                    log::warn!("Failed to cache token={} for link={}: {}", &token,&row.0, e);
                                    drop(e);
                                }
                            }
                            log::info!("Responded token={} to link={}", &token, &row.0);
                            return Ok(Redirect::temporary(&row.0))
                        },
                        Err(e) => {
                            log::info!("Not found link for token={}: {}", &token, e);
                            return Err(StatusCode::NOT_FOUND)
                        }
                    };
                }
            };
        },
        Err(e) => {
            log::error!("Failed to get Redis connection for token={}: {}", &token, e);
            drop(e);
            let now = SystemTime::now();
            let duration_since_epoch = now
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards");
            let timestamp_secs = duration_since_epoch.as_secs() as i32;
            match sqlx::query_as(FIND_REDIRECT).persistent(true).bind(&token).bind(timestamp_secs).fetch_one(&state.pool).await {
                Result::<(String , i32), _>::Ok(row) => {
                    log::info!("Responded token={} to link={}", &token, &row.0);
                    return Ok(Redirect::temporary(&row.0))
                },
                Err(e) => {
                    log::info!("Not found link for token={}: {}", &token, e);
                    return Err(StatusCode::NOT_FOUND)
                }
            };
        }
    };
}
