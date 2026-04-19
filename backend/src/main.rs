pub mod user;

use axum::{
    Router,
    routing::{get, post},
};

#[tokio::main]
async fn main() {
    let jwt_secret =
        std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string());

    let app = Router::new()
        .route("/auth/register", post(user::register))
        .route("/auth/login", post(user::login))
        .route("/auth/me", get(user::me))
        .with_state(user::AppState::new(jwt_secret));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001")
        .await
        .expect("failed to bind TCP listener");

    println!("Backend listening on http://0.0.0.0:3001");

    axum::serve(listener, app)
        .await
        .expect("server stopped unexpectedly");
}
