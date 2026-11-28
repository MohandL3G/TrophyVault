use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use trophyvault_core::generator::{
    generate_overall_passport_svg, generate_single_game_passport_svg, PassportOptions,
    PassportTheme,
};
use trophyvault_core::storage::{database_to_achievements_data, LibraryDatabase};

use std::sync::atomic::Ordering;
use crate::badge::generate_badge_svg;
use crate::state::{current_timestamp, SharedState};
use crate::steam::{run_steam_sync, SteamSyncOutcome};

/// Query parameters for /api/sync endpoint.
#[derive(Debug, Default, Deserialize)]
pub struct SyncQuery {
    pub mode: Option<String>,
}

/// Query parameters for /api/sync/steam endpoint.
#[derive(Debug, Default, Deserialize)]
pub struct SteamSyncQuery {
    pub force: Option<bool>,
    pub wait: Option<bool>,
}

/// Publicly safe Steam settings & sync status payload.
#[derive(Debug, Serialize)]
pub struct SteamSettingsStatus {
    pub api_key_configured: bool,
    pub steam_id: String,
    pub cooldown_secs: u64,
    pub persona_name: Option<String>,
    pub avatar_url: Option<String>,
    pub last_sync_at: Option<u64>,
    pub last_sync_status: Option<String>,
    pub last_sync_message: Option<String>,
    pub sync_in_progress: bool,
    pub cooldown_remaining_secs: u64,
}

/// Publicly safe CloudRedirect placeholder settings payload.
#[derive(Debug, Serialize)]
pub struct CloudRedirectSettingsStatus {
    pub configured: bool,
    pub api_url: String,
    pub api_key_configured: bool,
    pub status: &'static str,
}

/// Combined Two Settings response for GET/PUT /api/settings.
#[derive(Debug, Serialize)]
pub struct SettingsStatusResponse {
    pub steam: SteamSettingsStatus,
    pub cloudredirect: CloudRedirectSettingsStatus,
}

/// Admin update payload for PUT/POST /api/settings.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateSettingsRequest {
    pub steam_api_key: Option<String>,
    pub clear_steam_api_key: Option<bool>,
    pub steam_id: Option<String>,
    pub steam_sync_cooldown_secs: Option<u64>,
    pub cloudredirect_api_url: Option<String>,
    pub cloudredirect_api_key: Option<String>,
    pub clear_cloudredirect_api_key: Option<bool>,
}

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
    pub slogan: String,
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
    pub has_avatar: bool,
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
/// Supports mode=update (default, incremental upsert) and mode=full (snapshot overwrite).
pub async fn sync_handler(
    State(state): State<SharedState>,
    Query(query): Query<SyncQuery>,
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

    let is_full_sync = query
        .mode
        .as_deref()
        .map(|m| m.eq_ignore_ascii_case("full"))
        .unwrap_or(false);

    let final_db = {
        let current_state = state.read().await;
        if is_full_sync {
            let mut db = payload;
            // Playtime Protection in full sync: ensure playtime_minutes never decreases
            for (game_id, entry) in &mut db.games {
                if let Some(stored) = current_state.database.games.get(game_id) {
                    entry.playtime_minutes = entry.playtime_minutes.max(stored.playtime_minutes);
                    entry.base_playtime_minutes = entry.base_playtime_minutes.max(stored.base_playtime_minutes);
                    if entry.trophies.is_empty() && !stored.trophies.is_empty() {
                        entry.trophies = stored.trophies.clone();
                    }
                }
            }
            db.last_updated = current_timestamp();
            db
        } else {
            // Incremental upsert: merge payload.games into existing database.games
            let mut db = current_state.database.clone();
            for (game_id, mut entry) in payload.games {
                // Playtime Protection in update sync: ensure playtime_minutes never decreases
                if let Some(stored) = db.games.get(&game_id) {
                    entry.playtime_minutes = entry.playtime_minutes.max(stored.playtime_minutes);
                    entry.base_playtime_minutes = entry.base_playtime_minutes.max(stored.base_playtime_minutes);
                    if entry.trophies.is_empty() && !stored.trophies.is_empty() {
                        entry.trophies = stored.trophies.clone();
                    }
                }
                db.games.insert(game_id, entry);
            }
            db.preferences = payload.preferences;
            if !payload.manual_games.is_empty() {
                db.manual_games = payload.manual_games;
            }
            db.version = payload.version;
            db.last_updated = current_timestamp();
            db
        }
    };

    // 2. Persist database atomically to disk
    {
        let current_state = state.read().await;
        if let Err(e) = current_state.save_db_atomic(&final_db) {
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
    let games_count = final_db.games.len();
    let synced_at = current_timestamp();

    {
        let mut write_guard = state.write().await;
        write_guard.database = final_db;
        write_guard.last_modified = std::time::SystemTime::now();
    }

    // 4. Broadcast update to connected clients
    state.read().await.broadcast_update();

    tracing::info!(
        "Successfully synced library database ({} games, mode: {}) at {}",
        games_count,
        if is_full_sync { "full" } else { "update" },
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

/// POST /api/purge
///
/// Purges all games from the remote database while preserving preferences.
/// Authenticated via Bearer token or X-Admin-Password.
pub async fn purge_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let current_state = state.read().await;
    if !current_state.is_admin_authorized(&headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "status": "error",
                "message": "Unauthorized: Invalid or missing admin credentials"
            })),
        ));
    }

    let mut db_to_save = current_state.database.clone();
    drop(current_state);

    db_to_save.games.clear();
    db_to_save.last_updated = current_timestamp();

    // Persist to disk atomically
    {
        let current_state = state.read().await;
        if let Err(e) = current_state.save_db_atomic(&db_to_save) {
            tracing::error!("Failed to atomically save purged database: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": format!("Disk write failure: {}", e)
                })),
            ));
        }
    }

    // Update in-memory state
    {
        let mut write_guard = state.write().await;
        write_guard.database = db_to_save;
        write_guard.last_modified = std::time::SystemTime::now();
    }

    // Broadcast update
    state.read().await.broadcast_update();

    tracing::info!("Successfully purged library database games");

    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "message": "Database games purged successfully",
            "games_count": 0,
            "purged_at": current_timestamp()
        })),
    )
        .into_response())
}

/// GET /api/events
///
/// Server-Sent Events (SSE) endpoint broadcasting database updates to connected web clients.
pub async fn events_handler(
    State(state): State<SharedState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.read().await.update_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(()) => Some(Ok(Event::default().event("update").data("refresh"))),
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
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

    let default_username = current_state
        .username_override
        .as_deref()
        .unwrap_or(&db.preferences.username);
    let username_override = query.username.as_deref().unwrap_or(default_username);
    let default_title = current_state
        .slogan_override
        .as_deref()
        .unwrap_or(&db.preferences.title_line);
    let title_override = query.title.as_deref().unwrap_or(default_title);

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

    let default_uname = current_state
        .username_override
        .as_deref()
        .unwrap_or(&current_state.database.preferences.username);
    achievements_data.username = query.username.unwrap_or_else(|| default_uname.to_string());

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

    let username = current_state
        .username_override
        .clone()
        .unwrap_or_else(|| db.preferences.username.clone());

    let slogan = current_state
        .slogan_override
        .clone()
        .unwrap_or_default();

    let has_avatar = current_state.avatar_path.exists();

    let summary = LibraryStatsSummary {
        username,
        slogan,
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
        has_avatar,
    };

    Json(summary)
}

/// GET /api/games
///
/// Returns JSON array of games with playtime, completion %, unlocked trophies.
pub async fn games_handler(State(state): State<SharedState>) -> Json<Vec<GameSummaryItem>> {
    let current_state = state.read().await;

    // 1. Query directly from SQLite
    if let Ok(conn) = current_state.conn.lock() {
        let stmt = conn.prepare(
            "SELECT id, name, platform, appid, unlocked, total, pct, platinum, playtime_minutes, last_played, source, last_scanned \
             FROM games \
             ORDER BY playtime_minutes DESC, pct DESC, name ASC;",
        );
        if let Ok(mut stmt) = stmt {
            let rows = stmt.query_map([], |row| {
                let playtime_minutes = row.get::<_, i64>(8)? as u32;
                Ok(GameSummaryItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    platform: row.get(2)?,
                    appid: row.get(3)?,
                    unlocked: row.get::<_, i64>(4)? as u32,
                    total: row.get::<_, i64>(5)? as u32,
                    pct: row.get::<_, i64>(6)? as u32,
                    platinum: row.get::<_, i64>(7)? != 0,
                    playtime_minutes,
                    playtime_formatted: format_playtime(playtime_minutes as u64),
                    last_played: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                    source: row.get(10)?,
                    last_scanned: row.get::<_, i64>(11)? as u64,
                })
            });

            if let Ok(mapped) = rows {
                let mut games = Vec::new();
                for game_res in mapped {
                    if let Ok(game) = game_res {
                        games.push(game);
                    }
                }
                return Json(games);
            }
        }
    }

    // 2. Fallback to in-memory cached state
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

/// GET /api/games/:id/trophies
///
/// Returns the list of individual trophies for the specified game as JSON.
pub async fn game_trophies_handler(
    State(state): State<SharedState>,
    axum::extract::Path(game_id): axum::extract::Path<String>,
) -> Json<Vec<trophyvault_core::storage::SavedTrophy>> {
    let current_state = state.read().await;

    // 1. Try querying SQLite trophies table directly
    if let Ok(conn) = current_state.conn.lock() {
        if let Ok(trophies) = trophyvault_core::db::get_trophies_for_game(&conn, &game_id) {
            if !trophies.is_empty() {
                return Json(trophies);
            }
        }
    }

    // 2. Query from in-memory state
    let db = &current_state.database;

    // By ID
    if let Some(game) = db.games.get(&game_id) {
        if !game.trophies.is_empty() {
            return Json(game.trophies.clone());
        }
    }

    // By AppID or case-insensitive name
    if let Some(game) = db.games.values().find(|g| {
        g.appid.as_deref() == Some(&game_id) || g.name.eq_ignore_ascii_case(&game_id)
    }) {
        if !game.trophies.is_empty() {
            return Json(game.trophies.clone());
        }
        // Try querying SQLite using the resolved game id
        if let Ok(conn) = current_state.conn.lock() {
            if let Ok(trophies) = trophyvault_core::db::get_trophies_for_game(&conn, &game.id) {
                if !trophies.is_empty() {
                    return Json(trophies);
                }
            }
        }
    }

    // Fallback for Bloodborne: if game is Bloodborne, populate default 40 trophies
    if game_id.eq_ignore_ascii_case("bloodborne") {
        return Json(trophyvault_core::bloodborne::default_bloodborne_trophies());
    }

    // Return empty array for games with no trophy manifest
    Json(Vec::new())
}

/// JSON game detail including trophies returned by /api/games/:id.
#[derive(Debug, Serialize)]
pub struct SingleGameDetailResponse {
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
    pub trophies: Vec<trophyvault_core::storage::SavedTrophy>,
}

/// GET /api/games/:id
///
/// Returns detailed game information including its full trophy list.
pub async fn single_game_handler(
    State(state): State<SharedState>,
    axum::extract::Path(game_id): axum::extract::Path<String>,
) -> Result<Json<SingleGameDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    let current_state = state.read().await;
    let db = &current_state.database;

    let target_game = db.games.get(&game_id).or_else(|| {
        db.games.values().find(|g| {
            g.appid.as_deref() == Some(&game_id) || g.name.eq_ignore_ascii_case(&game_id)
        })
    });

    match target_game {
        Some(game) => {
            let mut trophies = game.trophies.clone();
            if trophies.is_empty() {
                if let Ok(conn) = current_state.conn.lock() {
                    if let Ok(db_trophies) = trophyvault_core::db::get_trophies_for_game(&conn, &game.id) {
                        if !db_trophies.is_empty() {
                            trophies = db_trophies;
                        }
                    }
                }
            }
            if trophies.is_empty() && (game.id.eq_ignore_ascii_case("bloodborne") || game.name.eq_ignore_ascii_case("Bloodborne")) {
                trophies = trophyvault_core::bloodborne::default_bloodborne_trophies();
            }

            Ok(Json(SingleGameDetailResponse {
                id: game.id.clone(),
                name: game.name.clone(),
                platform: game.platform.clone(),
                appid: game.appid.clone(),
                unlocked: game.unlocked,
                total: game.total,
                pct: game.pct,
                platinum: game.platinum,
                playtime_minutes: game.playtime_minutes,
                playtime_formatted: format_playtime(game.playtime_minutes as u64),
                last_played: game.last_played,
                source: game.source.clone(),
                last_scanned: game.last_scanned,
                trophies,
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "status": "error",
                "message": format!("Game '{}' not found", game_id)
            })),
        )),
    }
}

/// GET /api/avatar
/// Returns custom avatar image if uploaded, or 404 NOT_FOUND.
pub async fn get_avatar_handler(State(state): State<SharedState>) -> Response {
    let current_state = state.read().await;
    let path = &current_state.avatar_path;
    if !path.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let content_type = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                "image/png"
            } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                "image/jpeg"
            } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
                "image/webp"
            } else {
                "image/png"
            };

            let mut res = bytes.into_response();
            let headers = res.headers_mut();
            headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
            headers.insert(header::CACHE_CONTROL, "public, max-age=60".parse().unwrap());
            res
        }
        Err(e) => {
            tracing::error!("Failed to read avatar file: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// POST /api/avatar
/// Uploads custom avatar image (PNG/JPEG/WebP) up to 5MB.
/// Authenticated via X-Admin-Password or Bearer token.
pub async fn upload_avatar_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let current_state = state.read().await;
    if !current_state.is_admin_password_authorized(&headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "status": "error",
                "message": "Unauthorized: Invalid or missing admin password"
            })),
        ));
    }

    if body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "message": "Bad Request: Image body is empty"
            })),
        ));
    }

    if body.len() > 5 * 1024 * 1024 {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "status": "error",
                "message": "Payload Too Large: Avatar exceeds 5MB limit"
            })),
        ));
    }

    if let Some(parent) = current_state.avatar_path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
    }

    if let Err(e) = tokio::fs::write(&current_state.avatar_path, &body).await {
        tracing::error!("Failed to write avatar: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "status": "error",
                "message": format!("Disk write failure: {}", e)
            })),
        ));
    }

    tracing::info!("Custom avatar saved to {}", current_state.avatar_path.display());

    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "message": "Avatar uploaded successfully",
            "avatar_url": "/api/avatar"
        })),
    ).into_response())
}

/// DELETE /api/avatar
/// Removes custom avatar.
/// Authenticated via X-Admin-Password or Bearer token.
pub async fn delete_avatar_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let current_state = state.read().await;
    if !current_state.is_admin_password_authorized(&headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "status": "error",
                "message": "Unauthorized: Invalid or missing admin password"
            })),
        ));
    }

    if current_state.avatar_path.exists() {
        if let Err(e) = tokio::fs::remove_file(&current_state.avatar_path).await {
            tracing::error!("Failed to delete avatar: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": format!("Disk removal failure: {}", e)
                })),
            ));
        }
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "message": "Avatar removed successfully"
        })),
    ).into_response())
}

async fn build_settings_status(state: &SharedState) -> SettingsStatusResponse {
    let guard = state.read().await;
    let now = current_timestamp();
    let api_key_configured = guard
        .settings
        .steam_api_key
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let cr_url = guard
        .settings
        .cloudredirect_api_url
        .clone()
        .unwrap_or_default();
    let cr_key_configured = guard
        .settings
        .cloudredirect_api_key
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    SettingsStatusResponse {
        steam: SteamSettingsStatus {
            api_key_configured,
            steam_id: guard.settings.steam_id.clone(),
            cooldown_secs: guard.settings.steam_sync_cooldown_secs,
            persona_name: guard.steam_persona_name.clone(),
            avatar_url: guard.steam_avatar_url.clone(),
            last_sync_at: guard.last_steam_sync_at,
            last_sync_status: guard.last_steam_sync_status.clone(),
            last_sync_message: guard.last_steam_sync_message.clone(),
            sync_in_progress: guard.sync_in_progress.load(Ordering::SeqCst),
            cooldown_remaining_secs: guard.cooldown_remaining_secs(now),
        },
        cloudredirect: CloudRedirectSettingsStatus {
            configured: !cr_url.trim().is_empty() && cr_key_configured,
            api_url: cr_url,
            api_key_configured: cr_key_configured,
            status: "placeholder",
        },
    }
}

/// GET /api/settings
///
/// Returns sanitized Two Settings status (Steam Web API & CloudRedirect API placeholder).
/// Never exposes secret API keys.
pub async fn get_settings_handler(State(state): State<SharedState>) -> Json<SettingsStatusResponse> {
    Json(build_settings_status(&state).await)
}

/// PUT/POST /api/settings
///
/// Updates Setting 1 (Steam API) and Setting 2 (CloudRedirect API placeholder)
/// and persists them to SQLite `metadata` table. Requires Admin Password.
pub async fn update_settings_handler(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateSettingsRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    {
        let guard = state.read().await;
        if !guard.is_admin_password_authorized(&headers) {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "status": "error",
                    "message": "Unauthorized: Invalid or missing admin password"
                })),
            ));
        }
    }

    if let Some(ref sid) = payload.steam_id {
        let trimmed = sid.trim();
        if trimmed.is_empty() || !trimmed.chars().all(|c| c.is_ascii_digit()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "status": "error",
                    "message": "Invalid Steam ID: must be a numeric SteamID64"
                })),
            ));
        }
    }

    {
        let mut guard = state.write().await;

        if payload.clear_steam_api_key == Some(true) {
            guard.settings.steam_api_key = None;
        } else if let Some(ref key) = payload.steam_api_key {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                guard.settings.steam_api_key = Some(trimmed.to_string());
            }
        }

        if let Some(ref sid) = payload.steam_id {
            guard.settings.steam_id = sid.trim().to_string();
        }

        if let Some(cooldown) = payload.steam_sync_cooldown_secs {
            guard.settings.steam_sync_cooldown_secs = cooldown;
        }

        if let Some(ref url) = payload.cloudredirect_api_url {
            let trimmed = url.trim();
            guard.settings.cloudredirect_api_url = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }

        if payload.clear_cloudredirect_api_key == Some(true) {
            guard.settings.cloudredirect_api_key = None;
        } else if let Some(ref key) = payload.cloudredirect_api_key {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                guard.settings.cloudredirect_api_key = Some(trimmed.to_string());
            }
        }

        if let Err(e) = guard.persist_settings_to_db() {
            tracing::error!("Failed to persist settings to SQLite metadata: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": format!("Failed to persist settings: {}", e)
                })),
            ));
        }
    }

    let status = build_settings_status(&state).await;
    Ok((StatusCode::OK, Json(status)).into_response())
}

/// POST /api/sync/steam
///
/// Triggers Steam Web API ingestion respecting `STEAM_SYNC_COOLDOWN_SECS`.
/// Non-blocking by default (`202 Accepted`), spawning a background task that
/// persists updates to SQLite and notifies connected browsers via SSE `/api/events`.
/// Passing `?force=true` requires admin authorization to bypass cooldown.
/// Passing `?wait=true` awaits completion inline.
pub async fn sync_steam_handler(
    State(state): State<SharedState>,
    Query(query): Query<SteamSyncQuery>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let force = query.force.unwrap_or(false);
    let wait = query.wait.unwrap_or(false);

    if force {
        let guard = state.read().await;
        if !guard.is_admin_authorized(&headers) {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "status": "error",
                    "message": "Unauthorized: force=true requires admin authorization"
                })),
            ));
        }
    }

    // Fast pre-flight check before spawning background sync
    {
        let guard = state.read().await;
        let has_key = guard
            .settings
            .steam_api_key
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if !has_key {
            return Ok((
                StatusCode::OK,
                Json(SteamSyncOutcome::NotConfigured {
                    message: "STEAM_API_KEY is not configured".to_string(),
                }),
            )
                .into_response());
        }

        if guard.sync_in_progress.load(Ordering::SeqCst) {
            return Ok((StatusCode::OK, Json(SteamSyncOutcome::InProgress)).into_response());
        }

        if !force {
            let now = current_timestamp();
            let remaining = guard.cooldown_remaining_secs(now);
            if remaining > 0 {
                return Ok((
                    StatusCode::OK,
                    Json(SteamSyncOutcome::Cooldown {
                        cooldown_remaining_secs: remaining,
                        cooldown_secs: guard.settings.steam_sync_cooldown_secs,
                        last_sync_at: guard.last_steam_sync_at,
                    }),
                )
                    .into_response());
            }
        }
    }

    if wait {
        let outcome = run_steam_sync(state, force).await;
        let status_code = match &outcome {
            SteamSyncOutcome::Failed { .. } => StatusCode::BAD_GATEWAY,
            _ => StatusCode::OK,
        };
        return Ok((status_code, Json(outcome)).into_response());
    }

    let cooldown_secs = state.read().await.settings.steam_sync_cooldown_secs;
    let state_bg = state.clone();
    tokio::spawn(async move {
        let _ = run_steam_sync(state_bg, force).await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(SteamSyncOutcome::Started { cooldown_secs }),
    )
        .into_response())
}
