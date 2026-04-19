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
}

#[derive(Debug, Serialize)]
pub struct ErrorMessage {
    pub error: String,
}

pub struct AuthUser {
    pub username: String,
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
    let write_result = tokio::task::spawn_blocking(move || {
        db_for_insert
            .compare_and_swap(
                username_for_insert.as_bytes(),
                None as Option<&[u8]>,
                Some(password_hash.as_bytes()),
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
    let password_hash = tokio::task::spawn_blocking(move || {
        db_for_read
            .get(username_for_read.as_bytes())
            .map_err(|_| AuthError::Internal("unable to read users database"))
            .and_then(|maybe_hash| {
                maybe_hash
                    .ok_or(AuthError::Unauthorized("invalid credentials"))
                    .and_then(|hash_bytes| {
                        String::from_utf8(hash_bytes.to_vec()).map_err(|_| {
                            AuthError::Internal("stored user data is corrupted")
                        })
                    })
            })
    })
    .await
    .map_err(|_| AuthError::Internal("unable to read users database"))??;

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

pub async fn me(auth_user: AuthUser) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(MeResponse {
        username: auth_user.username,
    }))
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