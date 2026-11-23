pub mod badge;
pub mod db;
pub mod handlers;
pub mod state;

use axum::{
    handler::HandlerWithoutStateExt,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use std::path::PathBuf;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

/// Fallback HTML response when static assets are not yet populated.
async fn default_fallback() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>TrophyVault Web Server</title>
  <style>
    body { font-family: system-ui, sans-serif; background: #040c16; color: #f0f6fc; margin: 0; padding: 3rem; }
    h1 { color: #00f0ff; letter-spacing: 2px; }
    p { color: #8da4be; font-size: 1.1rem; }
    a { color: #ffd700; text-decoration: none; }
    a:hover { text-decoration: underline; }
    ul { list-style: none; padding: 0; }
    li { margin: 0.5rem 0; font-family: monospace; font-size: 1.05rem; }
    .badge { display: inline-block; padding: 2px 8px; border-radius: 4px; background: #0c3b5e; color: #00f0ff; font-weight: bold; }
  </style>
</head>
<body>
  <h1>TROPHYVAULT // SERVER <span class="badge">ONLINE</span></h1>
  <p>Self-hosted TrophyVault Sync Backend &amp; GitHub Camo Vector Showcase</p>
  <hr style="border: none; border-top: 1px solid #134267; margin: 2rem 0;">
  <h3>Active Endpoints:</h3>
  <ul>
    <li>&bull; <a href="/api/stats">/api/stats</a> &mdash; Library Summary JSON</li>
    <li>&bull; <a href="/api/games">/api/games</a> &mdash; Games &amp; Trophies JSON</li>
    <li>&bull; <a href="/api/passport.svg">/api/passport.svg</a> &mdash; Vector HUD Achievements Passport</li>
    <li>&bull; <a href="/api/badge.svg">/api/badge.svg</a> &mdash; Compact 400x100 GitHub Profile HUD Badge</li>
    <li>&bull; <span style="color:#ff3366;">POST</span> /api/sync &mdash; Desktop Remote Sync (Bearer Auth)</li>
    <li>&bull; <span style="color:#ff3366;">POST</span> /api/purge &mdash; Purge Database (Bearer / Admin Auth)</li>
    <li>&bull; <a href="/api/events">/api/events</a> &mdash; Server-Sent Events (SSE) Live Feed</li>
  </ul>
</body>
</html>"#,
    )
}

/// Creates the complete Axum router for the TrophyVault web server.
pub fn create_app(state: SharedState, static_dir: PathBuf) -> Router {
    let api_router = Router::new()
        .route("/api/health", get(handlers::health_handler))
        .route("/api/sync", post(handlers::sync_handler))
        .route("/api/purge", post(handlers::purge_handler))
        .route("/api/events", get(handlers::events_handler))
        .route("/api/passport.svg", get(handlers::passport_handler))
        .route("/api/badge.svg", get(handlers::badge_handler))
        .route("/api/stats", get(handlers::stats_handler))
        .route("/api/games", get(handlers::games_handler))
        .route("/api/games/:id", get(handlers::single_game_handler))
        .route("/api/games/:id/trophies", get(handlers::game_trophies_handler))
        .route(
            "/api/avatar",
            get(handlers::get_avatar_handler)
                .post(handlers::upload_avatar_handler)
                .delete(handlers::delete_avatar_handler),
        );

    let serve_service = if static_dir.exists() {
        ServeDir::new(&static_dir).fallback(default_fallback.into_service())
    } else {
        ServeDir::new("static").fallback(default_fallback.into_service())
    };

    api_router
        .fallback_service(serve_service)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
