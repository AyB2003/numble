pub mod user;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{
        HeaderValue, Method,
        header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, HeaderName},
    },
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let jwt_secret = load_jwt_secret();
    let database_url = std::env::var("DATABASE_URL").ok();
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "./users_db".to_string());
    let cors_origin = std::env::var("CORS_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let bind_address = std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:3001".to_string());

    let app_state = user::AppState::new(jwt_secret, database_url, db_path)
        .await
        .expect("failed to initialize users database");

    let cors = CorsLayer::new()
        .allow_origin(
            HeaderValue::from_str(&cors_origin)
                .expect("CORS_ORIGIN must be a valid header value, e.g. http://localhost:3000"),
        )
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    let app = Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(user::register))
        .route("/auth/login", post(user::login))
        .route("/auth/me", get(user::me))
        .route("/scores/record", post(user::record_score))
        .route("/scores/leaderboard", get(user::leaderboard))
        .layer(DefaultBodyLimit::max(8 * 1024))
        .layer(middleware::from_fn(add_security_headers))
        .layer(cors)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .expect("failed to bind TCP listener");

    println!("Backend listening on http://{bind_address}");

    axum::serve(listener, app)
        .await
        .expect("server stopped unexpectedly");
}

async fn health() -> &'static str {
    "ok"
}

fn load_jwt_secret() -> String {
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string());

    if env != "development" {
        if jwt_secret == "dev-secret-change-me" || jwt_secret.len() < 32 {
            panic!("JWT_SECRET must be set and at least 32 chars outside development");
        }
    }

    jwt_secret
}

async fn add_security_headers(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'; base-uri 'none'"),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));

    response
}
