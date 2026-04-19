use std::{sync::Arc, time::UNIX_EPOCH};

use axum::{
    Json,
    extract::{FromRef, FromRequestParts, State},
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
    response::{IntoResponse, Response},
};
use bcrypt::{DEFAULT_COST, hash, verify};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

#[derive(Clone)]
pub struct AppState {
    storage: StorageBackend,
    jwt_secret: Arc<String>,
}

#[derive(Clone)]
enum StorageBackend {
    Postgres(PgPool),
    Sled(sled::Db),
}

impl AppState {
    pub async fn new(
        jwt_secret: String,
        database_url: Option<String>,
        db_path: String,
    ) -> Result<Self, String> {
        let storage = if let Some(url) = database_url.filter(|value| !value.trim().is_empty()) {
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .connect(&url)
                .await
                .map_err(|error| format!("unable to connect to postgres: {error}"))?;

            initialize_postgres_schema(&pool)
                .await
                .map_err(|error| format!("unable to initialize postgres schema: {error}"))?;

            StorageBackend::Postgres(pool)
        } else {
            let users_db = sled::open(&db_path)
                .map_err(|error| format!("unable to open database at {db_path}: {error}"))?;
            StorageBackend::Sled(users_db)
        };

        Ok(Self {
            storage,
            jwt_secret: Arc::new(jwt_secret),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub token_type: String,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub username: String,
    pub score: u32,
    pub wins: u32,
    pub losses: u32,
    pub games_played: u32,
}

#[derive(Debug, Serialize)]
pub struct LeaderboardEntry {
    pub username: String,
    pub score: u32,
    pub wins: u32,
    pub losses: u32,
    pub games_played: u32,
}

#[derive(Debug, Serialize)]
pub struct LeaderboardResponse {
    pub players: Vec<LeaderboardEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ScoreUpdateRequest {
    pub won: bool,
    pub guesses_used: u32,
}

#[derive(Debug, Serialize)]
pub struct ScoreUpdateResponse {
    pub username: String,
    pub score: u32,
    pub wins: u32,
    pub losses: u32,
    pub games_played: u32,
}

#[derive(Debug, Serialize)]
pub struct ErrorMessage {
    pub error: String,
}

pub struct AuthUser {
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserRecord {
    password_hash: String,
    score: u32,
    wins: u32,
    losses: u32,
    games_played: u32,
}

pub enum AuthError {
    BadRequest(&'static str),
    Unauthorized(&'static str),
    Conflict(&'static str),
    Internal(&'static str),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            AuthError::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            AuthError::Conflict(message) => (StatusCode::CONFLICT, message),
            AuthError::Internal(message) => (StatusCode::INTERNAL_SERVER_ERROR, message),
        };

        (
            status,
            Json(ErrorMessage {
                error: message.to_string(),
            }),
        )
            .into_response()
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let State(app_state) = State::<AppState>::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthError::Internal("unable to read app state"))?;

        let header_value = parts
            .headers
            .get(AUTHORIZATION)
            .ok_or(AuthError::Unauthorized("missing Authorization header"))?
            .to_str()
            .map_err(|_| AuthError::Unauthorized("invalid Authorization header"))?;

        let token = header_value
            .strip_prefix("Bearer ")
            .ok_or(AuthError::Unauthorized("Authorization must be Bearer token"))?;

        let claims = decode_token(token, app_state.jwt_secret.as_str())?;

        Ok(Self {
            username: claims.sub,
        })
    }
}

pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AuthError> {
    validate_credentials(&payload.username, &payload.password)?;

    let password_hash = tokio::task::spawn_blocking(move || hash(payload.password, DEFAULT_COST))
        .await
        .map_err(|_| AuthError::Internal("unable to hash password"))
        .and_then(|result| result.map_err(|_| AuthError::Internal("unable to hash password")))?;

    match &state.storage {
        StorageBackend::Postgres(pool) => {
            let result = sqlx::query("INSERT INTO users (username, password_hash) VALUES ($1, $2)")
                .bind(payload.username.as_str())
                .bind(password_hash)
                .execute(pool)
                .await;

            match result {
                Ok(_) => {}
                Err(error) if is_unique_violation(&error) => {
                    return Err(AuthError::Conflict("username already exists"));
                }
                Err(_) => return Err(AuthError::Internal("unable to write user to database")),
            }
        }
        StorageBackend::Sled(db) => {
            let username_for_check = payload.username.clone();
            let db_for_check = db.clone();
            let user_exists = tokio::task::spawn_blocking(move || {
                db_for_check
                    .contains_key(username_for_check.as_bytes())
                    .map_err(|_| AuthError::Internal("unable to query users database"))
            })
            .await
            .map_err(|_| AuthError::Internal("unable to query users database"))??;

            if user_exists {
                return Err(AuthError::Conflict("username already exists"));
            }

            let user_record = UserRecord {
                password_hash,
                score: 0,
                wins: 0,
                losses: 0,
                games_played: 0,
            };
            let encoded_user = serde_json::to_vec(&user_record)
                .map_err(|_| AuthError::Internal("unable to encode user record"))?;

            let db_for_insert = db.clone();
            let username_for_insert = payload.username.clone();
            let write_result = tokio::task::spawn_blocking(move || {
                db_for_insert
                    .compare_and_swap(
                        username_for_insert.as_bytes(),
                        None as Option<&[u8]>,
                        Some(encoded_user.as_slice()),
                    )
                    .map_err(|_| AuthError::Internal("unable to write user to database"))
            })
            .await
            .map_err(|_| AuthError::Internal("unable to write user to database"))??;

            if write_result.is_err() {
                return Err(AuthError::Conflict("username already exists"));
            }

            let db_for_flush = db.clone();
            tokio::task::spawn_blocking(move || {
                db_for_flush
                    .flush()
                    .map_err(|_| AuthError::Internal("unable to persist user data"))
            })
            .await
            .map_err(|_| AuthError::Internal("unable to persist user data"))??;
        }
    }

    let access_token = create_token(&payload.username, state.jwt_secret.as_str())?;

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            access_token,
            token_type: "Bearer".to_string(),
        }),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, AuthError> {
    if payload.username.trim().is_empty() || payload.password.trim().is_empty() {
        return Err(AuthError::BadRequest("username and password are required"));
    }

    let password_hash = match &state.storage {
        StorageBackend::Postgres(pool) => {
            let maybe_row = sqlx::query("SELECT password_hash FROM users WHERE username = $1")
                .bind(payload.username.as_str())
                .fetch_optional(pool)
                .await
                .map_err(|_| AuthError::Internal("unable to read users database"))?;

            let row = maybe_row.ok_or(AuthError::Unauthorized("invalid credentials"))?;
            row.try_get::<String, _>("password_hash")
                .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
        }
        StorageBackend::Sled(db) => {
            let username_for_read = payload.username.clone();
            let db_for_read = db.clone();
            let user_record = tokio::task::spawn_blocking(move || {
                read_user_record_sled(&db_for_read, &username_for_read)
            })
            .await
            .map_err(|_| AuthError::Internal("unable to read users database"))??;

            user_record.password_hash
        }
    };

    let password = payload.password;
    let is_valid = tokio::task::spawn_blocking(move || verify(password, &password_hash))
        .await
        .map_err(|_| AuthError::Internal("unable to verify password"))
        .and_then(|result| result.map_err(|_| AuthError::Internal("unable to verify password")))?;

    if !is_valid {
        return Err(AuthError::Unauthorized("invalid credentials"));
    }

    let access_token = create_token(&payload.username, state.jwt_secret.as_str())?;

    Ok(Json(AuthResponse {
        access_token,
        token_type: "Bearer".to_string(),
    }))
}

pub async fn me(
    State(state): State<AppState>,
    auth_user: AuthUser,
) -> Result<impl IntoResponse, AuthError> {
    let username = auth_user.username;

    let record = match &state.storage {
        StorageBackend::Postgres(pool) => {
            let maybe_row = sqlx::query(
                "SELECT score, wins, losses, games_played FROM users WHERE username = $1",
            )
            .bind(username.as_str())
            .fetch_optional(pool)
            .await
            .map_err(|_| AuthError::Internal("unable to read users database"))?;

            let row = maybe_row.ok_or(AuthError::Unauthorized("invalid credentials"))?;
            UserRecord {
                password_hash: String::new(),
                score: row
                    .try_get::<i32, _>("score")
                    .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                    as u32,
                wins: row
                    .try_get::<i32, _>("wins")
                    .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                    as u32,
                losses: row
                    .try_get::<i32, _>("losses")
                    .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                    as u32,
                games_played: row
                    .try_get::<i32, _>("games_played")
                    .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                    as u32,
            }
        }
        StorageBackend::Sled(db) => {
            let username_for_lookup = username.clone();
            let db_for_read = db.clone();
            tokio::task::spawn_blocking(move || {
                read_user_record_sled(&db_for_read, &username_for_lookup)
            })
            .await
            .map_err(|_| AuthError::Internal("unable to read users database"))??
        }
    };

    Ok(Json(MeResponse {
        username,
        score: record.score,
        wins: record.wins,
        losses: record.losses,
        games_played: record.games_played,
    }))
}

pub async fn record_score(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<ScoreUpdateRequest>,
) -> Result<impl IntoResponse, AuthError> {
    let username = auth_user.username;
    let score_delta = calculate_score_delta(payload.won, payload.guesses_used)?;

    let updated_record = match &state.storage {
        StorageBackend::Postgres(pool) => {
            let maybe_row = sqlx::query(
                "UPDATE users
                 SET games_played = games_played + 1,
                     wins = wins + CASE WHEN $2 THEN 1 ELSE 0 END,
                     losses = losses + CASE WHEN $2 THEN 0 ELSE 1 END,
                     score = score + $3
                 WHERE username = $1
                 RETURNING score, wins, losses, games_played",
            )
            .bind(username.as_str())
            .bind(payload.won)
            .bind(score_delta as i32)
            .fetch_optional(pool)
            .await
            .map_err(|_| AuthError::Internal("unable to update score"))?;

            let row = maybe_row.ok_or(AuthError::Unauthorized("invalid credentials"))?;
            UserRecord {
                password_hash: String::new(),
                score: row
                    .try_get::<i32, _>("score")
                    .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                    as u32,
                wins: row
                    .try_get::<i32, _>("wins")
                    .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                    as u32,
                losses: row
                    .try_get::<i32, _>("losses")
                    .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                    as u32,
                games_played: row
                    .try_get::<i32, _>("games_played")
                    .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                    as u32,
            }
        }
        StorageBackend::Sled(db) => {
            let username_for_write = username.clone();
            let db_for_write = db.clone();
            tokio::task::spawn_blocking(move || {
                update_user_score_sled(&db_for_write, &username_for_write, payload.won, score_delta)
            })
            .await
            .map_err(|_| AuthError::Internal("unable to update score"))??
        }
    };

    Ok(Json(ScoreUpdateResponse {
        username,
        score: updated_record.score,
        wins: updated_record.wins,
        losses: updated_record.losses,
        games_played: updated_record.games_played,
    }))
}

pub async fn leaderboard(State(state): State<AppState>) -> Result<impl IntoResponse, AuthError> {
    let players = match &state.storage {
        StorageBackend::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT username, score, wins, losses, games_played
                 FROM users
                 ORDER BY score DESC, wins DESC, losses ASC, username ASC
                 LIMIT 10",
            )
            .fetch_all(pool)
            .await
            .map_err(|_| AuthError::Internal("unable to read leaderboard"))?;

            rows.into_iter()
                .map(|row| {
                    Ok(LeaderboardEntry {
                        username: row
                            .try_get::<String, _>("username")
                            .map_err(|_| AuthError::Internal("stored username data is corrupted"))?,
                        score: row
                            .try_get::<i32, _>("score")
                            .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                            as u32,
                        wins: row
                            .try_get::<i32, _>("wins")
                            .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                            as u32,
                        losses: row
                            .try_get::<i32, _>("losses")
                            .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                            as u32,
                        games_played: row
                            .try_get::<i32, _>("games_played")
                            .map_err(|_| AuthError::Internal("stored user data is corrupted"))?
                            as u32,
                    })
                })
                .collect::<Result<Vec<_>, AuthError>>()?
        }
        StorageBackend::Sled(db) => {
            let db_for_read = db.clone();
            tokio::task::spawn_blocking(move || read_leaderboard_sled(&db_for_read))
                .await
                .map_err(|_| AuthError::Internal("unable to read leaderboard"))??
        }
    };

    Ok(Json(LeaderboardResponse { players }))
}

fn validate_credentials(username: &str, password: &str) -> Result<(), AuthError> {
    if username.trim().len() < 3 {
        return Err(AuthError::BadRequest(
            "username must contain at least 3 characters",
        ));
    }

    if password.len() < 8 {
        return Err(AuthError::BadRequest(
            "password must contain at least 8 characters",
        ));
    }

    Ok(())
}

fn create_token(username: &str, secret: &str) -> Result<String, AuthError> {
    let now = now_in_seconds();
    let expires_in = 24 * 60 * 60;

    let claims = Claims {
        sub: username.to_string(),
        exp: (now + expires_in) as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AuthError::Internal("unable to create access token"))
}

fn decode_token(token: &str, secret: &str) -> Result<Claims, AuthError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| AuthError::Unauthorized("invalid or expired token"))
}

fn now_in_seconds() -> u64 {
    UNIX_EPOCH.elapsed().unwrap_or_default().as_secs()
}

async fn initialize_postgres_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL,
            score INTEGER NOT NULL DEFAULT 0,
            wins INTEGER NOT NULL DEFAULT 0,
            losses INTEGER NOT NULL DEFAULT 0,
            games_played INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|db_error| db_error.code().map(|code| code == "23505"))
        .unwrap_or(false)
}

fn parse_user_record_sled(bytes: &[u8]) -> Result<UserRecord, AuthError> {
    if let Ok(record) = serde_json::from_slice::<UserRecord>(bytes) {
        return Ok(record);
    }

    let legacy_hash = String::from_utf8(bytes.to_vec())
        .map_err(|_| AuthError::Internal("stored user data is corrupted"))?;

    Ok(UserRecord {
        password_hash: legacy_hash,
        score: 0,
        wins: 0,
        losses: 0,
        games_played: 0,
    })
}

fn read_user_record_sled(db: &sled::Db, username: &str) -> Result<UserRecord, AuthError> {
    let maybe_record = db
        .get(username.as_bytes())
        .map_err(|_| AuthError::Internal("unable to read users database"))?;

    let raw_record = maybe_record.ok_or(AuthError::Unauthorized("invalid credentials"))?;
    parse_user_record_sled(&raw_record)
}

fn update_user_score_sled(
    db: &sled::Db,
    username: &str,
    won: bool,
    score_delta: u32,
) -> Result<UserRecord, AuthError> {
    loop {
        let current_value = db
            .get(username.as_bytes())
            .map_err(|_| AuthError::Internal("unable to read users database"))?
            .ok_or(AuthError::Unauthorized("invalid credentials"))?;

        let mut record = parse_user_record_sled(&current_value)?;
        record.games_played = record.games_played.saturating_add(1);

        if won {
            record.wins = record.wins.saturating_add(1);
            record.score = record.score.saturating_add(score_delta);
        } else {
            record.losses = record.losses.saturating_add(1);
            record.score = record.score.saturating_add(score_delta);
        }

        let encoded_record = serde_json::to_vec(&record)
            .map_err(|_| AuthError::Internal("unable to encode user record"))?;

        let cas_result = db
            .compare_and_swap(
                username.as_bytes(),
                Some(current_value.as_ref()),
                Some(encoded_record.as_slice()),
            )
            .map_err(|_| AuthError::Internal("unable to update users database"))?;

        if cas_result.is_ok() {
            db.flush()
                .map_err(|_| AuthError::Internal("unable to persist user data"))?;
            return Ok(record);
        }
    }
}

fn calculate_score_delta(won: bool, guesses_used: u32) -> Result<u32, AuthError> {
    const MAX_GUESSES: u32 = 6;

    if !(1..=MAX_GUESSES).contains(&guesses_used) {
        return Err(AuthError::BadRequest(
            "guesses_used must be between 1 and 6",
        ));
    }

    if !won {
        return Ok(0);
    }

    // Rewards fewer guesses with larger points: 120, 60, 40, 30, 24, 20.
    Ok(120 / guesses_used)
}

fn read_leaderboard_sled(db: &sled::Db) -> Result<Vec<LeaderboardEntry>, AuthError> {
    let mut entries = Vec::new();

    for item in db.iter() {
        let (key, value) = item.map_err(|_| AuthError::Internal("unable to read leaderboard"))?;
        let username = String::from_utf8(key.to_vec())
            .map_err(|_| AuthError::Internal("stored username data is corrupted"))?;
        let record = parse_user_record_sled(&value)?;

        entries.push(LeaderboardEntry {
            username,
            score: record.score,
            wins: record.wins,
            losses: record.losses,
            games_played: record.games_played,
        });
    }

    entries.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.wins.cmp(&a.wins))
            .then_with(|| a.losses.cmp(&b.losses))
            .then_with(|| a.username.cmp(&b.username))
    });

    entries.truncate(10);
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode},
        routing::{get, post},
    };
    use serde_json::{Value, json};
    use tempfile::TempDir;
    use tower::ServiceExt;

    async fn test_app() -> (Router, TempDir) {
        let tmp_dir = TempDir::new().expect("failed to create temp dir");
        let db_path = tmp_dir
            .path()
            .join("users_db")
            .to_string_lossy()
            .to_string();

        let state = AppState::new("test-secret".to_string(), None, db_path)
            .await
            .expect("failed to create app state");

        let app = Router::new()
            .route("/auth/register", post(register))
            .route("/auth/login", post(login))
            .route("/auth/me", get(me))
            .route("/scores/record", post(record_score))
            .route("/scores/leaderboard", get(leaderboard))
            .with_state(state);

        (app, tmp_dir)
    }

    async fn request_json(
        app: &Router,
        method: Method,
        uri: &str,
        body: Value,
        auth: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut request_builder = Request::builder().method(method).uri(uri);

        if let Some(token) = auth {
            request_builder = request_builder.header(AUTHORIZATION, format!("Bearer {token}"));
        }

        let request = request_builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("failed to build request");

        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("request failed");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed to read body");
        let payload = serde_json::from_slice::<Value>(&bytes).expect("invalid json response");

        (status, payload)
    }

    async fn request_no_body(app: &Router, method: Method, uri: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request");

        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("request failed");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed to read body");
        let payload = serde_json::from_slice::<Value>(&bytes).expect("invalid json response");

        (status, payload)
    }

    #[tokio::test]
    async fn auth_and_score_flow_works() {
        let (app, _tmp_dir) = test_app().await;

        let (register_status, register_payload) = request_json(
            &app,
            Method::POST,
            "/auth/register",
            json!({"username":"alice","password":"password123"}),
            None,
        )
        .await;
        assert_eq!(register_status, StatusCode::CREATED);

        let token = register_payload
            .get("access_token")
            .and_then(Value::as_str)
            .expect("missing access token");

        let (me_status, me_payload) =
            request_json(&app, Method::GET, "/auth/me", json!({}), Some(token)).await;
        assert_eq!(me_status, StatusCode::OK);
        assert_eq!(me_payload["username"], "alice");
        assert_eq!(me_payload["score"], 0);

        let (win_status, win_payload) = request_json(
            &app,
            Method::POST,
            "/scores/record",
            json!({"won": true, "guesses_used": 1}),
            Some(token),
        )
        .await;
        assert_eq!(win_status, StatusCode::OK);
        assert_eq!(win_payload["score"], 120);
        assert_eq!(win_payload["wins"], 1);

        let (loss_status, loss_payload) = request_json(
            &app,
            Method::POST,
            "/scores/record",
            json!({"won": false, "guesses_used": 2}),
            Some(token),
        )
        .await;
        assert_eq!(loss_status, StatusCode::OK);
        assert_eq!(loss_payload["score"], 120);
        assert_eq!(loss_payload["wins"], 1);
        assert_eq!(loss_payload["losses"], 1);
        assert_eq!(loss_payload["games_played"], 2);
    }

    #[tokio::test]
    async fn leaderboard_is_sorted_by_score() {
        let (app, _tmp_dir) = test_app().await;

        let (_, alice_register) = request_json(
            &app,
            Method::POST,
            "/auth/register",
            json!({"username":"alice","password":"password123"}),
            None,
        )
        .await;
        let alice_token = alice_register["access_token"]
            .as_str()
            .expect("missing alice token");

        let (_, bob_register) = request_json(
            &app,
            Method::POST,
            "/auth/register",
            json!({"username":"bob","password":"password123"}),
            None,
        )
        .await;
        let bob_token = bob_register["access_token"].as_str().expect("missing bob token");

        let _ = request_json(
            &app,
            Method::POST,
            "/scores/record",
            json!({"won": true, "guesses_used": 1}),
            Some(alice_token),
        )
        .await;
        let _ = request_json(
            &app,
            Method::POST,
            "/scores/record",
            json!({"won": true, "guesses_used": 2}),
            Some(alice_token),
        )
        .await;
        let _ = request_json(
            &app,
            Method::POST,
            "/scores/record",
            json!({"won": false, "guesses_used": 6}),
            Some(bob_token),
        )
        .await;

        let (status, leaderboard_payload) = request_no_body(&app, Method::GET, "/scores/leaderboard").await;
        assert_eq!(status, StatusCode::OK);

        let players = leaderboard_payload["players"]
            .as_array()
            .expect("players should be an array");
        assert_eq!(players[0]["username"], "alice");
        assert_eq!(players[0]["score"], 180);
        assert_eq!(players[1]["username"], "bob");
        assert_eq!(players[1]["score"], 0);
    }
}
