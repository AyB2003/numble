use std::{collections::HashMap, sync::Arc, time::UNIX_EPOCH};

use axum::{
    Json,
    extract::{FromRef, FromRequestParts, State},
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
    response::{IntoResponse, Response},
};
use bcrypt::{DEFAULT_COST, hash, verify};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    users: Arc<RwLock<HashMap<String, String>>>,
    jwt_secret: Arc<String>,
}

impl AppState {
    pub fn new(jwt_secret: String) -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            jwt_secret: Arc::new(jwt_secret),
        }
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

    {
        let users = state.users.read().await;
        if users.contains_key(&payload.username) {
            return Err(AuthError::Conflict("username already exists"));
        }
    }

    let password_hash = tokio::task::spawn_blocking(move || hash(payload.password, DEFAULT_COST))
        .await
        .map_err(|_| AuthError::Internal("unable to hash password"))
        .and_then(|result| result.map_err(|_| AuthError::Internal("unable to hash password")))?;

    {
        let mut users = state.users.write().await;
        users.insert(payload.username.clone(), password_hash);
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

    let password_hash = {
        let users = state.users.read().await;
        users
            .get(&payload.username)
            .cloned()
            .ok_or(AuthError::Unauthorized("invalid credentials"))?
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