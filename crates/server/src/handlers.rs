use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use trophyvault_core::generator::{
    generate_overall_passport_svg, generate_single_game_passport_svg, PassportOptions,
    PassportTheme,
};
use trophyvault_core::storage::{database_to_achievements_data, LibraryDatabase};

use crate::badge::generate_badge_svg;
use crate::state::{current_timestamp, SharedState};

/// Payload for /api/sync endpoint.
#[derive(Debug, Deserialize)]
pub struct SyncQuery {}

/// Query parameters for /api/passport.svg.
#[derive(Debug, Default, Deserialize)]
pub struct PassportQuery {
    pub theme: Option<String>,
    pub mode: Option<String>,
    pub appid: Option<String>,
    pub title: Option<String>,
    pub username: Option<String>,
}

/// Query parameters for /api/badge.svg.
#[derive(Debug, Default, Deserialize)]
pub struct BadgeQuery {
    pub theme: Option<String>,
    pub username: Option<String>,
}

/// JSON library summary returned by /api/stats.
#[derive(Debug, Serialize)]
pub struct LibraryStatsSummary {
    pub username: String,
    pub title: String,
    pub theme: String,
    pub total_games: usize,
    pub perfect_games: u32,
    pub in_progress_games: u32,
    pub not_started_games: u32,
    pub total_achievements: u32,
    pub unlocked_achievements: u32,
    pub locked_achievements: u32,
    pub completion_rate: f64,
    pub ovr_score: u32,
    pub total_playtime_minutes: u64,
    pub total_playtime_formatted: String,
    pub last_updated: u64,
}

/// JSON game item returned by /api/games.
#[derive(Debug, Serialize)]
pub struct GameSummaryItem {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub appid: Option<String>,
    pub unlocked: u32,
    pub total: u32,
    pub pct: u32,
    pub platinum: bool,
    pub playtime_minutes: u32,
    pub playtime_formatted: String,
    pub last_played: Option<u64>,
    pub source: String,
    pub last_scanned: u64,
}

/// Format playtime in minutes to readable hours and minutes string.
fn format_playtime(minutes: u64) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    format!("{}h {}m", hours, mins)
}

/// Helper to create Camo-bypassing SVG HTTP response.
fn make_svg_response(svg_content: String) -> Response {
    let mut hasher = DefaultHasher::new();
    svg_content.hash(&mut hasher);
    let etag = format!("W/\"{:016x}\"", hasher.finish());

    let mut res = svg_content.into_response();
    let headers = res.headers_mut();

    headers.insert(
        header::CONTENT_TYPE,
        "image/svg+xml; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CACHE_CONTROL,
        "no-cache, no-store, must-revalidate, max-age=0, s-maxage=0"
            .parse()
            .unwrap(),
    );
    headers.insert(header::PRAGMA, "no-cache".parse().unwrap());
    headers.insert(header::EXPIRES, "0".parse().unwrap());
    headers.insert(header::ETAG, etag.parse().unwrap());

    res
}

/// GET /api/health
///
/// Health check endpoint for web UI and load balancers.
pub async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// POST /api/sync
///
/// Synchronizes library database from desktop app to web server.
/// Authenticated via Bearer token.
pub async fn sync_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<LibraryDatabase>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // 1. Validate Authorization header
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|val| val.to_str().ok());

    let is_authorized = match auth_header {
        Some(val) if val.starts_with("Bearer ") => {
            let token = val.trim_start_matches("Bearer ").trim();
            let current_state = state.read().await;
            token == current_state.api_key
        }
        _ => false,
    };

    if !is_authorized {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "status": "error",
                "message": "Unauthorized: Invalid or missing Bearer token"
            })),
        ));
    }

    // 2. Persist database atomically to disk
    {
        let current_state = state.read().await;
        if let Err(e) = current_state.save_db_atomic(&payload) {
            tracing::error!("Failed to atomically save database: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": format!("Disk write failure: {}", e)
                })),
            ));
        }
    }

    // 3. Update in-memory cached state
    let games_count = payload.games.len();
    let synced_at = current_timestamp();

    {
        let mut write_guard = state.write().await;
        write_guard.database = payload;
        write_guard.last_modified = std::time::SystemTime::now();
    }

    tracing::info!(
        "Successfully synced library database ({} games) at {}",
        games_count,
        synced_at
    );

    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "games_count": games_count,
            "synced_at": synced_at
        })),
    )
        .into_response())
}

/// GET /api/passport.svg
///
/// Generates achievements passport SVG with GitHub Camo cache-bypass headers.
pub async fn passport_handler(
    State(state): State<SharedState>,
    Query(query): Query<PassportQuery>,
) -> Response {
    let current_state = state.read().await;
    let db = &current_state.database;
    let achievements_data = database_to_achievements_data(db);

    let theme_str = query.theme.as_deref().unwrap_or("cyberpunk");
    let theme = PassportTheme::parse_theme(theme_str);

    let mode = query.mode.as_deref().unwrap_or("overall");
    let requested_appid = query.appid.as_deref();

    let username_override = query
        .username
        .as_deref()
        .unwrap_or(&db.preferences.username);
    let title_override = query
        .title
        .as_deref()
        .unwrap_or(&db.preferences.title_line);

    // Determine if single game passport is requested
    let mut selected_game = None;
    if mode.eq_ignore_ascii_case("single")
        || mode.eq_ignore_ascii_case("game")
        || requested_appid.is_some()
    {
        if let Some(target_id) = requested_appid {
            selected_game = achievements_data.games.iter().find(|g| {
                g.appid.as_deref() == Some(target_id)
                    || g.name.eq_ignore_ascii_case(target_id)
            });
        }

        // If single mode was specified without valid appid, default to the highest completed or first game
        if selected_game.is_none() && (mode.eq_ignore_ascii_case("single") || mode.eq_ignore_ascii_case("game")) {
            selected_game = achievements_data.games.first();
        }
    }

    let svg = if let Some(game) = selected_game {
        let options = PassportOptions {
            username: username_override,
            title: title_override,
            theme,
            selected_game: Some(game),
        };
        generate_single_game_passport_svg(&achievements_data, &options)
    } else {
        let options = PassportOptions {
            username: username_override,
            title: title_override,
            theme,
            selected_game: None,
        };
        generate_overall_passport_svg(&achievements_data, &options)
    };

    make_svg_response(svg)
}

/// GET /api/badge.svg
///
/// Generates sleek, compact 400x100 HUD badge for GitHub profile headers.
pub async fn badge_handler(
    State(state): State<SharedState>,
    Query(query): Query<BadgeQuery>,
) -> Response {
    let current_state = state.read().await;
    let mut achievements_data = database_to_achievements_data(&current_state.database);

    if let Some(uname) = query.username {
        achievements_data.username = uname;
    }

    let theme_str = query.theme.as_deref().unwrap_or("cyberpunk");
    let theme = PassportTheme::parse_theme(theme_str);

    let svg = generate_badge_svg(&achievements_data, theme);
    make_svg_response(svg)
}

/// GET /api/stats
///
/// Returns JSON library summary.
pub async fn stats_handler(State(state): State<SharedState>) -> Json<LibraryStatsSummary> {
    let current_state = state.read().await;
    let db = &current_state.database;
    let data = database_to_achievements_data(db);
    let s = &data.stats;

    let summary = LibraryStatsSummary {
        username: db.preferences.username.clone(),
        title: db.preferences.title_line.clone(),
        theme: db.preferences.theme.clone(),
        total_games: db.games.len(),
        perfect_games: s.perfect_games,
        in_progress_games: s.in_progress_games,
        not_started_games: s.not_started_games,
        total_achievements: s.total_achievements,
        unlocked_achievements: s.unlocked_achievements,
        locked_achievements: s.locked_achievements,
        completion_rate: s.completion_rate,
        ovr_score: s.ovr_score,
        total_playtime_minutes: s.total_playtime_minutes,
        total_playtime_formatted: format_playtime(s.total_playtime_minutes),
        last_updated: db.last_updated,
    };

    Json(summary)
}

/// GET /api/games
///
/// Returns JSON array of games with playtime, completion %, unlocked trophies.
pub async fn games_handler(State(state): State<SharedState>) -> Json<Vec<GameSummaryItem>> {
    let current_state = state.read().await;
    let db = &current_state.database;

    let mut games: Vec<GameSummaryItem> = db
        .games
        .values()
        .map(|entry| GameSummaryItem {
            id: entry.id.clone(),
            name: entry.name.clone(),
            platform: entry.platform.clone(),
            appid: entry.appid.clone(),
            unlocked: entry.unlocked,
            total: entry.total,
            pct: entry.pct,
            platinum: entry.platinum,
            playtime_minutes: entry.playtime_minutes,
            playtime_formatted: format_playtime(entry.playtime_minutes as u64),
            last_played: entry.last_played,
            source: entry.source.clone(),
            last_scanned: entry.last_scanned,
        })
        .collect();

    // Sort by playtime descending, then pct descending, then name
    games.sort_by(|a, b| {
        b.playtime_minutes
            .cmp(&a.playtime_minutes)
            .then_with(|| b.pct.cmp(&a.pct))
            .then_with(|| a.name.cmp(&b.name))
    });

    Json(games)
}
