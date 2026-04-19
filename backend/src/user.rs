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
use serde_json;

#[derive(Clone)]
pub struct AppState {
    users_db: sled::Db,
    jwt_secret: Arc<String>,
}

impl AppState {
    pub fn new(jwt_secret: String, db_path: String) -> Result<Self, String> {
        let users_db = sled::open(&db_path)
            .map_err(|error| format!("unable to open database at {db_path}: {error}"))?;

        Ok(Self {
            users_db,
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

        (status, Json(ErrorMessage { error: message.to_string() })).into_response()
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

    let db_for_check = state.users_db.clone();
    let username_for_check = payload.username.clone();
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

    let password_hash = tokio::task::spawn_blocking(move || hash(payload.password, DEFAULT_COST))
        .await
        .map_err(|_| AuthError::Internal("unable to hash password"))
        .and_then(|result| result.map_err(|_| AuthError::Internal("unable to hash password")))?;

    let db_for_insert = state.users_db.clone();
    let username_for_insert = payload.username.clone();
    let user_record = UserRecord {
        password_hash,
        score: 0,
        wins: 0,
        losses: 0,
        games_played: 0,
    };
    let encoded_user = serde_json::to_vec(&user_record)
        .map_err(|_| AuthError::Internal("unable to encode user record"))?;

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

    let db_for_flush = state.users_db.clone();
    tokio::task::spawn_blocking(move || {
        db_for_flush
            .flush()
            .map_err(|_| AuthError::Internal("unable to persist user data"))
    })
    .await
    .map_err(|_| AuthError::Internal("unable to persist user data"))??;

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

    let db_for_read = state.users_db.clone();
    let username_for_read = payload.username.clone();
    let user_record = tokio::task::spawn_blocking(move || {
        read_user_record(&db_for_read, &username_for_read)
    })
    .await
    .map_err(|_| AuthError::Internal("unable to read users database"))??;

    let password_hash = user_record.password_hash;

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
    let db_for_read = state.users_db.clone();
    let username = auth_user.username;
    let username_for_lookup = username.clone();

    let user_record = tokio::task::spawn_blocking(move || {
        read_user_record(&db_for_read, &username_for_lookup)
    })
    .await
    .map_err(|_| AuthError::Internal("unable to read users database"))??;

    Ok(Json(MeResponse {
        username,
        score: user_record.score,
        wins: user_record.wins,
        losses: user_record.losses,
        games_played: user_record.games_played,
    }))
}

pub async fn record_score(
    State(state): State<AppState>,
    auth_user: AuthUser,
    Json(payload): Json<ScoreUpdateRequest>,
) -> Result<impl IntoResponse, AuthError> {
    let db_for_write = state.users_db.clone();
    let username = auth_user.username;
    let username_for_write = username.clone();

    let updated_record = tokio::task::spawn_blocking(move || {
        update_user_score(&db_for_write, &username_for_write, payload.won)
    })
    .await
    .map_err(|_| AuthError::Internal("unable to update score"))??;

    Ok(Json(ScoreUpdateResponse {
        username,
        score: updated_record.score,
        wins: updated_record.wins,
        losses: updated_record.losses,
        games_played: updated_record.games_played,
    }))
}

pub async fn leaderboard(State(state): State<AppState>) -> Result<impl IntoResponse, AuthError> {
    let db_for_read = state.users_db.clone();

    let players = tokio::task::spawn_blocking(move || read_leaderboard(&db_for_read))
        .await
        .map_err(|_| AuthError::Internal("unable to read leaderboard"))??;

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

fn parse_user_record(bytes: &[u8]) -> Result<UserRecord, AuthError> {
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

fn read_user_record(db: &sled::Db, username: &str) -> Result<UserRecord, AuthError> {
    let maybe_record = db
        .get(username.as_bytes())
        .map_err(|_| AuthError::Internal("unable to read users database"))?;

    let raw_record = maybe_record.ok_or(AuthError::Unauthorized("invalid credentials"))?;
    parse_user_record(&raw_record)
}

fn update_user_score(db: &sled::Db, username: &str, won: bool) -> Result<UserRecord, AuthError> {
    loop {
        let current_value = db
            .get(username.as_bytes())
            .map_err(|_| AuthError::Internal("unable to read users database"))?
            .ok_or(AuthError::Unauthorized("invalid credentials"))?;

        let mut record = parse_user_record(&current_value)?;
        record.games_played = record.games_played.saturating_add(1);

        if won {
            record.wins = record.wins.saturating_add(1);
            record.score = record.score.saturating_add(10);
        } else {
            record.losses = record.losses.saturating_add(1);
            record.score = record.score.saturating_add(2);
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

fn read_leaderboard(db: &sled::Db) -> Result<Vec<LeaderboardEntry>, AuthError> {
    let mut entries = Vec::new();

    for item in db.iter() {
        let (key, value) = item.map_err(|_| AuthError::Internal("unable to read leaderboard"))?;
        let username = String::from_utf8(key.to_vec())
            .map_err(|_| AuthError::Internal("stored username data is corrupted"))?;
        let record = parse_user_record(&value)?;

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