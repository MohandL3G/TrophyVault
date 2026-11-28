use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;
use trophyvault_core::storage::{
    DatabasePreferences, LibraryDatabase, SavedGameEntry, SavedTrophy,
};
use trophyvault_server::{
    create_app,
    state::{AppState, ServerConfig},
};

fn create_test_db() -> LibraryDatabase {
    let mut games = HashMap::new();
    games.insert(
        "bloodborne".to_string(),
        SavedGameEntry {
            id: "bloodborne".to_string(),
            name: "Bloodborne".to_string(),
            platform: "PC Port / Recomp".to_string(),
            appid: Some("2485153835".to_string()),
            unlocked: 40,
            total: 40,
            pct: 100,
            platinum: true,
            base_playtime_minutes: 0,
            playtime_minutes: 5400,
            last_played: Some(1730000000),
            source: "manual".to_string(),
            last_scanned: 1730000000,
            trophies: vec![SavedTrophy {
                id: "0".to_string(),
                name: "Bloodborne".to_string(),
                description: "All trophies unlocked".to_string(),
                grade: "Platinum".to_string(),
                unlocked: true,
                unlocked_time: Some(1730000000),
            }],
        },
    );
    games.insert(
        "palworld".to_string(),
        SavedGameEntry {
            id: "palworld".to_string(),
            name: "Palworld".to_string(),
            platform: "Steam".to_string(),
            appid: Some("1623730".to_string()),
            unlocked: 15,
            total: 30,
            pct: 50,
            platinum: false,
            base_playtime_minutes: 0,
            playtime_minutes: 2400,
            last_played: Some(1730100000),
            source: "steam".to_string(),
            last_scanned: 1730100000,
            trophies: Vec::new(),
        },
    );

    LibraryDatabase {
        version: 1,
        last_updated: 1730200000,
        preferences: DatabasePreferences {
            username: "HunterOfYharnam".to_string(),
            title_line: "Trophy Collector".to_string(),
            theme: "CyberpunkCyan".to_string(),
            steam_path: None,
            steam_id: Some("76561198000000000".to_string()),
            export_path: None,
            auto_sync: true,
            sync_settings: None,
        },
        manual_games: Vec::new(),
        games,
    }
}

fn setup_test_app() -> (axum::Router, tempfile::TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("test_db.db");
    let static_dir = temp_dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();

    let api_key = "test-secret-token".to_string();
    let config = ServerConfig {
        port: 5050,
        api_key: api_key.clone(),
        admin_password: "admin-pass-123".to_string(),
        username: None,
        slogan: None,
        db_path,
        avatar_path: temp_dir.path().join("avatar.png"),
        static_dir: static_dir.clone(),
        ..ServerConfig::default()
    };

    let test_db = create_test_db();
    let state = Arc::new(RwLock::new(AppState::with_database(&config, test_db)));
    let app = create_app(state, static_dir);

    (app, temp_dir, api_key)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/health")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn test_stats_endpoint() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/stats")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let stats: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(stats["username"], "HunterOfYharnam");
    assert_eq!(stats["slogan"], "");
    assert_eq!(stats["has_avatar"], false);
    assert_eq!(stats["total_games"], 2);
    assert_eq!(stats["perfect_games"], 1);
    assert_eq!(stats["total_achievements"], 70);
    assert_eq!(stats["unlocked_achievements"], 55);
}

#[tokio::test]
async fn test_stats_endpoint_with_env_overrides() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let static_dir = temp_dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();

    let config = ServerConfig {
        port: 5050,
        api_key: "key".to_string(),
        admin_password: "admin-pass-123".to_string(),
        username: Some("CustomOverriddenUser".to_string()),
        slogan: Some("CYBER_HUNTER".to_string()),
        db_path: temp_dir.path().join("db.db"),
        avatar_path: temp_dir.path().join("avatar.png"),
        static_dir: static_dir.clone(),
        ..ServerConfig::default()
    };

    let test_db = create_test_db();
    let state = Arc::new(RwLock::new(AppState::with_database(&config, test_db)));
    let app = create_app(state, static_dir);

    let req = Request::builder().uri("/api/stats").method("GET").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let stats: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(stats["username"], "CustomOverriddenUser");
    assert_eq!(stats["slogan"], "CYBER_HUNTER");
}

#[tokio::test]
async fn test_avatar_endpoints_crud_and_auth() {
    let (app, temp_dir, _key) = setup_test_app();

    // 1. GET /api/avatar when not uploaded -> 404
    let req = Request::builder().uri("/api/avatar").method("GET").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 2. POST /api/avatar unauthorized (no password) -> 401
    let fake_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01];
    let req = Request::builder()
        .uri("/api/avatar")
        .method("POST")
        .body(Body::from(fake_png.clone()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 3. POST /api/avatar authorized with X-Admin-Password -> 200
    let req = Request::builder()
        .uri("/api/avatar")
        .method("POST")
        .header("X-Admin-Password", "admin-pass-123")
        .body(Body::from(fake_png.clone()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify avatar file was written
    assert!(temp_dir.path().join("avatar.png").exists());

    // 4. GET /api/avatar -> 200 with image/png
    let req = Request::builder().uri("/api/avatar").method("GET").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "image/png");
    let downloaded = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(downloaded.to_vec(), fake_png);

    // 5. DELETE /api/avatar unauthorized -> 401
    let req = Request::builder().uri("/api/avatar").method("DELETE").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 6. DELETE /api/avatar authorized with Bearer token (admin password) -> 200
    let req = Request::builder()
        .uri("/api/avatar")
        .method("DELETE")
        .header("Authorization", "Bearer admin-pass-123")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // File should be deleted
    assert!(!temp_dir.path().join("avatar.png").exists());

    // 7. GET /api/avatar again -> 404
    let req = Request::builder().uri("/api/avatar").method("GET").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_games_endpoint() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/games")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let games: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(games.len(), 2);
    // Highest playtime first (Bloodborne with 5400m)
    assert_eq!(games[0]["name"], "Bloodborne");
    assert_eq!(games[0]["pct"], 100);
    assert_eq!(games[0]["platinum"], true);
}

#[tokio::test]
async fn test_passport_svg_endpoint() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/passport.svg?theme=gold&mode=overall")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify Camo headers
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/svg+xml; charset=utf-8"
    );
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-cache, no-store, must-revalidate, max-age=0, s-maxage=0"
    );
    assert_eq!(res.headers().get(header::PRAGMA).unwrap(), "no-cache");
    assert_eq!(res.headers().get(header::EXPIRES).unwrap(), "0");
    assert!(res.headers().get(header::ETAG).unwrap().to_str().unwrap().starts_with("W/\""));

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let svg_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(svg_str.contains("<svg"));
    assert!(svg_str.contains("</svg>"));
    assert!(svg_str.contains("HunterOfYharnam"));
}

#[tokio::test]
async fn test_badge_svg_endpoint() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/badge.svg?theme=cyberpunk")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify Camo headers
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/svg+xml; charset=utf-8"
    );
    assert_eq!(
        res.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-cache, no-store, must-revalidate, max-age=0, s-maxage=0"
    );
    assert!(res.headers().contains_key(header::ETAG));

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let svg_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(svg_str.contains("viewBox=\"0 0 400 100\""));
    assert!(svg_str.contains("TROPHYVAULT // HUD"));
    assert!(svg_str.contains("GAMES"));
    assert!(svg_str.contains("PERFECT"));
}

#[tokio::test]
async fn test_sync_unauthorized() {
    let (app, _temp, _key) = setup_test_app();

    // Missing header
    let req = Request::builder()
        .uri("/api/sync")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(&create_test_db()).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // Invalid token
    let req2 = Request::builder()
        .uri("/api/sync")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer wrong-token")
        .body(Body::from(serde_json::to_string(&create_test_db()).unwrap()))
        .unwrap();

    let res2 = app.oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_sync_success_and_state_update() {
    let (app, temp, api_key) = setup_test_app();

    let mut new_db = create_test_db();
    new_db.preferences.username = "UpdatedHunter".to_string();
    new_db.games.insert(
        "eldenring".to_string(),
        SavedGameEntry {
            id: "eldenring".to_string(),
            name: "Elden Ring".to_string(),
            platform: "Steam".to_string(),
            appid: Some("1245620".to_string()),
            unlocked: 42,
            total: 42,
            pct: 100,
            platinum: true,
            base_playtime_minutes: 0,
            playtime_minutes: 12000,
            last_played: Some(1730300000),
            source: "steam".to_string(),
            last_scanned: 1730300000,
            trophies: Vec::new(),
        },
    );

    let req = Request::builder()
        .uri("/api/sync")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::from(serde_json::to_string(&new_db).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let resp: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["games_count"], 3);

    // Verify disk persistence in SQLite
    let db_path = temp.path().join("test_db.db");
    assert!(db_path.exists());
    let conn = trophyvault_core::db::open_or_create_db(&db_path).unwrap();
    let disk_db = trophyvault_core::db::load_library_database_from_sqlite(&conn).unwrap();
    assert_eq!(disk_db.preferences.username, "UpdatedHunter");
    assert!(disk_db.games.contains_key("eldenring"));

    // Verify GET /api/stats now shows 3 games and UpdatedHunter
    let stats_req = Request::builder()
        .uri("/api/stats")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let stats_res = app.oneshot(stats_req).await.unwrap();
    assert_eq!(stats_res.status(), StatusCode::OK);
    let stats_bytes = stats_res.into_body().collect().await.unwrap().to_bytes();
    let stats: serde_json::Value = serde_json::from_slice(&stats_bytes).unwrap();
    assert_eq!(stats["username"], "UpdatedHunter");
    assert_eq!(stats["total_games"], 3);
    assert_eq!(stats["perfect_games"], 2);
}

#[tokio::test]
async fn test_static_index_serving() {
    let static_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");
    let temp_dir = tempfile::tempdir().unwrap();
    let config = ServerConfig {
        port: 5050,
        api_key: "key".to_string(),
        admin_password: "admin-pass-123".to_string(),
        username: None,
        slogan: None,
        db_path: temp_dir.path().join("db.db"),
        avatar_path: temp_dir.path().join("avatar.png"),
        static_dir: static_dir.clone(),
        ..ServerConfig::default()
    };
    let state = Arc::new(RwLock::new(AppState::new(&config)));
    let app = create_app(state, static_dir);

    let req = Request::builder().uri("/").method("GET").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("TrophyVault") || html.contains("<!DOCTYPE html>"));
}

#[tokio::test]
async fn test_default_fallback_html() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
    assert!(ct.contains("text/html"));
    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("TROPHYVAULT // SERVER"));
}

#[tokio::test]
async fn test_incremental_update_sync_preserves_unmentioned_games() {
    let (app, _temp, api_key) = setup_test_app();

    // Initial server db has "bloodborne" (A) and "palworld" (B)
    let games_req = Request::builder().uri("/api/games").method("GET").body(Body::empty()).unwrap();
    let games_res = app.clone().oneshot(games_req).await.unwrap();
    assert_eq!(games_res.status(), StatusCode::OK);
    let bytes = games_res.into_body().collect().await.unwrap().to_bytes();
    let initial_games: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(initial_games.len(), 2);

    // Client sends payload containing only A (updated) and C (added); B is omitted!
    let mut payload = LibraryDatabase {
        version: 1,
        last_updated: 1730500000,
        preferences: DatabasePreferences::default(),
        manual_games: Vec::new(),
        games: HashMap::new(),
    };

    // A: bloodborne updated
    payload.games.insert(
        "bloodborne".to_string(),
        SavedGameEntry {
            id: "bloodborne".to_string(),
            name: "Bloodborne Remastered".to_string(),
            platform: "PC Port / Recomp".to_string(),
            appid: Some("2485153835".to_string()),
            unlocked: 40,
            total: 40,
            pct: 100,
            platinum: true,
            base_playtime_minutes: 0,
            playtime_minutes: 9999,
            last_played: Some(1730500000),
            source: "manual".to_string(),
            last_scanned: 1730500000,
            trophies: Vec::new(),
        },
    );

    // C: eldenring added
    payload.games.insert(
        "eldenring".to_string(),
        SavedGameEntry {
            id: "eldenring".to_string(),
            name: "Elden Ring".to_string(),
            platform: "Steam".to_string(),
            appid: Some("1245620".to_string()),
            unlocked: 42,
            total: 42,
            pct: 100,
            platinum: true,
            base_playtime_minutes: 0,
            playtime_minutes: 15000,
            last_played: Some(1730500000),
            source: "steam".to_string(),
            last_scanned: 1730500000,
            trophies: Vec::new(),
        },
    );

    // Test default sync mode (mode=update)
    let sync_req = Request::builder()
        .uri("/api/sync?mode=update")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();

    let sync_res = app.clone().oneshot(sync_req).await.unwrap();
    assert_eq!(sync_res.status(), StatusCode::OK);

    // Verify GET /api/games: MUST contain A, B, and C! (B is preserved)
    let check_req = Request::builder().uri("/api/games").method("GET").body(Body::empty()).unwrap();
    let check_res = app.oneshot(check_req).await.unwrap();
    assert_eq!(check_res.status(), StatusCode::OK);
    let check_bytes = check_res.into_body().collect().await.unwrap().to_bytes();
    let final_games: Vec<serde_json::Value> = serde_json::from_slice(&check_bytes).unwrap();

    assert_eq!(final_games.len(), 3);
    let ids: Vec<&str> = final_games.iter().map(|g| g["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"bloodborne"));
    assert!(ids.contains(&"palworld")); // B preserved!
    assert!(ids.contains(&"eldenring")); // C added!

    // Verify A was updated
    let bb = final_games.iter().find(|g| g["id"] == "bloodborne").unwrap();
    assert_eq!(bb["name"], "Bloodborne Remastered");
    assert_eq!(bb["playtime_minutes"], 9999);
}

#[tokio::test]
async fn test_full_sync_overwrites_entire_games_database() {
    let (app, _temp, api_key) = setup_test_app();

    // Initial server db has "bloodborne" (A) and "palworld" (B)
    // Client sends full sync payload with ONLY game C ("eldenring")
    let mut payload = LibraryDatabase {
        version: 1,
        last_updated: 1730500000,
        preferences: DatabasePreferences::default(),
        manual_games: Vec::new(),
        games: HashMap::new(),
    };

    payload.games.insert(
        "eldenring".to_string(),
        SavedGameEntry {
            id: "eldenring".to_string(),
            name: "Elden Ring".to_string(),
            platform: "Steam".to_string(),
            appid: Some("1245620".to_string()),
            unlocked: 42,
            total: 42,
            pct: 100,
            platinum: true,
            base_playtime_minutes: 0,
            playtime_minutes: 15000,
            last_played: Some(1730500000),
            source: "steam".to_string(),
            last_scanned: 1730500000,
            trophies: Vec::new(),
        },
    );

    let sync_req = Request::builder()
        .uri("/api/sync?mode=full")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();

    let sync_res = app.clone().oneshot(sync_req).await.unwrap();
    assert_eq!(sync_res.status(), StatusCode::OK);

    // Verify GET /api/games: MUST ONLY contain C ("eldenring")
    let check_req = Request::builder().uri("/api/games").method("GET").body(Body::empty()).unwrap();
    let check_res = app.oneshot(check_req).await.unwrap();
    assert_eq!(check_res.status(), StatusCode::OK);
    let check_bytes = check_res.into_body().collect().await.unwrap().to_bytes();
    let final_games: Vec<serde_json::Value> = serde_json::from_slice(&check_bytes).unwrap();

    assert_eq!(final_games.len(), 1);
    assert_eq!(final_games[0]["id"], "eldenring");
    assert_eq!(final_games[0]["name"], "Elden Ring");
}

#[tokio::test]
async fn test_purge_endpoint_auth_and_clearing() {
    let (app, temp, api_key) = setup_test_app();

    // 1. Unauthorized purge (no auth header) -> 401
    let req1 = Request::builder()
        .uri("/api/purge")
        .method("POST")
        .body(Body::empty())
        .unwrap();
    let res1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(res1.status(), StatusCode::UNAUTHORIZED);

    // 2. Unauthorized purge (invalid token) -> 401
    let req2 = Request::builder()
        .uri("/api/purge")
        .method("POST")
        .header(header::AUTHORIZATION, "Bearer invalid-token")
        .body(Body::empty())
        .unwrap();
    let res2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::UNAUTHORIZED);

    // 3. Authorized purge via Bearer token -> 200
    let req3 = Request::builder()
        .uri("/api/purge")
        .method("POST")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::empty())
        .unwrap();
    let res3 = app.clone().oneshot(req3).await.unwrap();
    assert_eq!(res3.status(), StatusCode::OK);
    let body_bytes = res3.into_body().collect().await.unwrap().to_bytes();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["status"], "ok");
    assert_eq!(body_json["games_count"], 0);

    // 4. Verify GET /api/games is now empty
    let games_req = Request::builder().uri("/api/games").method("GET").body(Body::empty()).unwrap();
    let games_res = app.clone().oneshot(games_req).await.unwrap();
    assert_eq!(games_res.status(), StatusCode::OK);
    let bytes = games_res.into_body().collect().await.unwrap().to_bytes();
    let games: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(games.len(), 0);

    // 5. Verify GET /api/stats reflects empty games
    let stats_req = Request::builder().uri("/api/stats").method("GET").body(Body::empty()).unwrap();
    let stats_res = app.oneshot(stats_req).await.unwrap();
    assert_eq!(stats_res.status(), StatusCode::OK);
    let bytes = stats_res.into_body().collect().await.unwrap().to_bytes();
    let stats: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(stats["total_games"], 0);
    assert_eq!(stats["perfect_games"], 0);

    // 6. Verify disk persistence in SQLite
    let db_path = temp.path().join("test_db.db");
    assert!(db_path.exists());
    let conn = trophyvault_core::db::open_or_create_db(&db_path).unwrap();
    let disk_db = trophyvault_core::db::load_library_database_from_sqlite(&conn).unwrap();
    assert!(disk_db.games.is_empty());
}

#[tokio::test]
async fn test_events_endpoint_sse() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/events")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
}

#[tokio::test]
async fn test_avatar_upload_and_delete_with_admin_password() {
    let (app, temp_dir, _key) = setup_test_app();
    let fake_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01];

    // 1. Upload PNG bytes with X-Admin-Password succeeds
    let req = Request::builder()
        .uri("/api/avatar")
        .method("POST")
        .header("X-Admin-Password", "admin-pass-123")
        .body(Body::from(fake_png.clone()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(temp_dir.path().join("avatar.png").exists());

    // 2. GET /api/avatar returns 200
    let req = Request::builder().uri("/api/avatar").method("GET").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "image/png");
    let downloaded = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(downloaded.to_vec(), fake_png);

    // 3. DELETE /api/avatar succeeds
    let req = Request::builder()
        .uri("/api/avatar")
        .method("DELETE")
        .header("X-Admin-Password", "admin-pass-123")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(!temp_dir.path().join("avatar.png").exists());
}

#[tokio::test]
async fn test_avatar_upload_fails_with_api_key_only() {
    let (app, temp_dir, api_key) = setup_test_app();
    let fake_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01];

    // Attempting avatar upload with Bearer <api_key> returns 401 UNAUTHORIZED
    let req = Request::builder()
        .uri("/api/avatar")
        .method("POST")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::from(fake_png))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(!temp_dir.path().join("avatar.png").exists());

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["status"], "error");
    assert_eq!(body["message"], "Unauthorized: Invalid or missing admin password");
}

#[tokio::test]
async fn test_server_migrates_existing_json_to_sqlite_on_startup() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let json_path = temp_dir.path().join("library_database.json");
    let db_path = temp_dir.path().join("trophyvault.db");
    let static_dir = temp_dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();

    // 1. Create a legacy JSON database file
    let sample_db = create_test_db();
    let json_str = serde_json::to_string_pretty(&sample_db).unwrap();
    std::fs::write(&json_path, json_str).unwrap();

    // 2. Initialize AppState using AppState::new (which executes auto-migration)
    let config = ServerConfig {
        port: 5050,
        api_key: "test-token".to_string(),
        admin_password: "admin-pass-123".to_string(),
        username: None,
        slogan: None,
        db_path: db_path.clone(),
        avatar_path: temp_dir.path().join("avatar.png"),
        static_dir: static_dir.clone(),
        ..ServerConfig::default()
    };

    let state = Arc::new(RwLock::new(AppState::new(&config)));
    let app = create_app(state, static_dir);

    // 3. Verify SQLite database was created
    assert!(db_path.exists());

    // 4. Verify /api/games returns migrated games querying through SQLite
    let req = Request::builder()
        .uri("/api/games")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    let games: Vec<serde_json::Value> = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(games.len(), 2);
    assert_eq!(games[0]["id"], "bloodborne");
    assert_eq!(games[0]["playtime_minutes"], 5400);

    // 5. Verify /api/stats reflects the migrated games and preferences
    let stats_req = Request::builder()
        .uri("/api/stats")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let stats_res = app.oneshot(stats_req).await.unwrap();
    assert_eq!(stats_res.status(), StatusCode::OK);

    let stats_bytes = stats_res.into_body().collect().await.unwrap().to_bytes();
    let stats: serde_json::Value = serde_json::from_slice(&stats_bytes).unwrap();
    assert_eq!(stats["username"], "HunterOfYharnam");
    assert_eq!(stats["total_games"], 2);
    assert_eq!(stats["perfect_games"], 1);

    // 6. Direct query on SQLite table to verify trophies table persistence
    let conn = trophyvault_core::db::open_or_create_db(&db_path).unwrap();
    let trophy_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM trophies", [], |r| r.get(0))
        .unwrap();
    assert_eq!(trophy_count, 1);
}

#[tokio::test]
async fn test_server_config_path_resolution_and_migration() {
    use trophyvault_server::state::resolve_sqlite_path;
    use std::path::Path;

    assert_eq!(
        resolve_sqlite_path(Path::new("/data/library_database.json")),
        Path::new("/data/library_database.db")
    );
    assert_eq!(
        resolve_sqlite_path(Path::new("/data/trophyvault.db")),
        Path::new("/data/trophyvault.db")
    );
    assert_eq!(
        resolve_sqlite_path(Path::new("./data/trophyvault.db")),
        Path::new("./data/trophyvault.db")
    );
}

#[tokio::test]
async fn test_server_playtime_protection_update_mode() {
    let (app, _temp, api_key) = setup_test_app();

    // In initial setup, Bloodborne has 5400 minutes.
    // Client attempts to sync payload with lower playtime (e.g. 120 minutes)
    let mut payload = create_test_db();
    if let Some(bb) = payload.games.get_mut("bloodborne") {
        bb.playtime_minutes = 120;
    }

    let req = Request::builder()
        .uri("/api/sync?mode=update")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify playtime_minutes remained at max(5400, 120) = 5400
    let games_req = Request::builder().uri("/api/games").method("GET").body(Body::empty()).unwrap();
    let games_res = app.oneshot(games_req).await.unwrap();
    let bytes = games_res.into_body().collect().await.unwrap().to_bytes();
    let games: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    let bb = games.iter().find(|g| g["id"] == "bloodborne").unwrap();
    assert_eq!(bb["playtime_minutes"], 5400);
}

#[tokio::test]
async fn test_server_playtime_protection_full_mode() {
    let (app, _temp, api_key) = setup_test_app();

    // In initial setup, Bloodborne has 5400 minutes.
    // Client attempts a full sync with lower playtime (e.g. 0 minutes)
    let mut payload = create_test_db();
    if let Some(bb) = payload.games.get_mut("bloodborne") {
        bb.playtime_minutes = 0;
    }

    let req = Request::builder()
        .uri("/api/sync?mode=full")
        .method("POST")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify playtime_minutes remained at max(5400, 0) = 5400
    let games_req = Request::builder().uri("/api/games").method("GET").body(Body::empty()).unwrap();
    let games_res = app.oneshot(games_req).await.unwrap();
    let bytes = games_res.into_body().collect().await.unwrap().to_bytes();
    let games: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    let bb = games.iter().find(|g| g["id"] == "bloodborne").unwrap();
    assert_eq!(bb["playtime_minutes"], 5400);
}

#[tokio::test]
async fn test_game_trophies_endpoint() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/games/bloodborne/trophies")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let trophies: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(!trophies.is_empty());
    assert_eq!(trophies[0]["id"], "0");
    assert_eq!(trophies[0]["name"], "Bloodborne");
    assert_eq!(trophies[0]["grade"], "Platinum");
    assert_eq!(trophies[0]["unlocked"], true);
}

#[tokio::test]
async fn test_single_game_detail_endpoint() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/games/bloodborne")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let game: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(game["id"], "bloodborne");
    assert_eq!(game["name"], "Bloodborne");
    assert_eq!(game["trophies"][0]["name"], "Bloodborne");
}

#[tokio::test]
async fn test_settings_get_never_exposes_secrets_and_defaults() {
    let (app, _temp, _key) = setup_test_app();

    let req = Request::builder()
        .uri("/api/settings")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["steam"]["api_key_configured"], false);
    assert_eq!(json["steam"]["steam_id"], "76561198423209668");
    assert_eq!(json["steam"]["cooldown_secs"], 300);
    assert_eq!(json["steam"]["sync_in_progress"], false);
    assert_eq!(json["cloudredirect"]["configured"], false);
    assert_eq!(json["cloudredirect"]["api_key_configured"], false);
    assert_eq!(json["cloudredirect"]["status"], "placeholder");
    assert!(json["steam"].get("api_key").is_none());
    assert!(json["cloudredirect"].get("api_key").is_none());
}

#[tokio::test]
async fn test_settings_update_requires_admin_and_persists_to_sqlite() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("settings_test.db");
    let static_dir = temp_dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();

    let config = ServerConfig {
        port: 5050,
        api_key: "sync-only-token".to_string(),
        admin_password: "super-admin-pass".to_string(),
        db_path: db_path.clone(),
        avatar_path: temp_dir.path().join("avatar.png"),
        static_dir: static_dir.clone(),
        ..ServerConfig::default()
    };

    let state = Arc::new(RwLock::new(AppState::new(&config)));
    let app = create_app(state, static_dir.clone());

    let update_body = serde_json::json!({
        "steam_api_key": "secret-steam-key-12345",
        "steam_id": "76561198000012345",
        "steam_sync_cooldown_secs": 120,
        "cloudredirect_api_url": "https://cr.mohandl3g.ly",
        "cloudredirect_api_key": "cr-secret-999"
    });

    // 1. Unauthenticated -> 401
    let res1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .method("PUT")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(update_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res1.status(), StatusCode::UNAUTHORIZED);

    // 2. API_KEY alone -> 401 (admin password strictly required)
    let res2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .method("PUT")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer sync-only-token")
                .body(Body::from(update_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res2.status(), StatusCode::UNAUTHORIZED);

    // 3. Valid X-Admin-Password -> 200 OK
    let res3 = app
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .method("PUT")
                .header(header::CONTENT_TYPE, "application/json")
                .header("X-Admin-Password", "super-admin-pass")
                .body(Body::from(update_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res3.status(), StatusCode::OK);

    let bytes = res3.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body_str.contains("secret-steam-key-12345"));
    assert!(!body_str.contains("cr-secret-999"));

    let json: serde_json::Value = serde_json::from_str(&body_str).unwrap();
    assert_eq!(json["steam"]["api_key_configured"], true);
    assert_eq!(json["steam"]["steam_id"], "76561198000012345");
    assert_eq!(json["steam"]["cooldown_secs"], 120);
    assert_eq!(json["cloudredirect"]["configured"], true);
    assert_eq!(json["cloudredirect"]["api_url"], "https://cr.mohandl3g.ly");
    assert_eq!(json["cloudredirect"]["api_key_configured"], true);

    // 4. Reload AppState from disk and verify SQLite metadata persistence survives restart
    let reloaded_state = AppState::new(&config);
    assert_eq!(
        reloaded_state.settings.steam_api_key.as_deref(),
        Some("secret-steam-key-12345")
    );
    assert_eq!(reloaded_state.settings.steam_id, "76561198000012345");
    assert_eq!(reloaded_state.settings.steam_sync_cooldown_secs, 120);
    assert_eq!(
        reloaded_state.settings.cloudredirect_api_url.as_deref(),
        Some("https://cr.mohandl3g.ly")
    );
    assert_eq!(
        reloaded_state.settings.cloudredirect_api_key.as_deref(),
        Some("cr-secret-999")
    );
}

#[tokio::test]
async fn test_steam_sync_mock_server_ingestion_monotonic_and_cooldown() {
    use axum::extract::Query as AxumQuery;
    use axum::routing::get;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let owned_calls = Arc::new(AtomicUsize::new(0));
    let owned_calls_clone = Arc::clone(&owned_calls);

    let mock_router = axum::Router::new()
        .route(
            "/ISteamUser/GetPlayerSummaries/v0002/",
            get(|| async {
                axum::Json(serde_json::json!({
                    "response": {
                        "players": [{
                            "steamid": "76561198423209668",
                            "personaname": "MohandL3G",
                            "profileurl": "https://steamcommunity.com/id/MohandL3G/",
                            "avatarfull": "https://avatars.steamstatic.com/test_full.jpg"
                        }]
                    }
                }))
            }),
        )
        .route(
            "/IPlayerService/GetOwnedGames/v0001/",
            get(move || {
                let counter = Arc::clone(&owned_calls_clone);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    axum::Json(serde_json::json!({
                        "response": {
                            "game_count": 3,
                            "games": [
                                {
                                    "appid": 1086940,
                                    "name": "Baldur's Gate 3",
                                    "playtime_forever": 14500,
                                    "rtime_last_played": 1732700000,
                                    "has_community_visible_stats": true
                                },
                                {
                                    "appid": 1623730,
                                    "name": "Palworld",
                                    "playtime_forever": 100,
                                    "rtime_last_played": 1732600000,
                                    "has_community_visible_stats": true
                                },
                                {
                                    "appid": 999999,
                                    "name": "Private Stats Game",
                                    "playtime_forever": 50,
                                    "rtime_last_played": 1732500000,
                                    "has_community_visible_stats": true
                                }
                            ]
                        }
                    }))
                }
            }),
        )
        .route(
            "/ISteamUserStats/GetSchemaForGame/v0002/",
            get(|AxumQuery(params): AxumQuery<HashMap<String, String>>| async move {
                let appid = params.get("appid").map(String::as_str).unwrap_or("");
                if appid == "1086940" {
                    (
                        StatusCode::OK,
                        axum::Json(serde_json::json!({
                            "game": {
                                "gameName": "Baldur's Gate 3",
                                "availableGameStats": {
                                    "achievements": [
                                        {
                                            "name": "BG3_HERO",
                                            "displayName": "Hero of the Gate",
                                            "description": "Save Baldur's Gate",
                                            "hidden": 0
                                        },
                                        {
                                            "name": "BG3_SECRET",
                                            "displayName": "Dark Urge",
                                            "hidden": 1
                                        }
                                    ]
                                }
                            }
                        })),
                    )
                } else {
                    (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({})))
                }
            }),
        )
        .route(
            "/ISteamUserStats/GetPlayerAchievements/v0001/",
            get(|AxumQuery(params): AxumQuery<HashMap<String, String>>| async move {
                let appid = params.get("appid").map(String::as_str).unwrap_or("");
                if appid == "1086940" {
                    (
                        StatusCode::OK,
                        axum::Json(serde_json::json!({
                            "playerstats": {
                                "steamID": "76561198423209668",
                                "gameName": "Baldur's Gate 3",
                                "success": true,
                                "achievements": [
                                    { "apiname": "BG3_HERO", "achieved": 1, "unlocktime": 1732000100 },
                                    { "apiname": "BG3_SECRET", "achieved": 0, "unlocktime": 0 }
                                ]
                            }
                        })),
                    )
                } else {
                    (
                        StatusCode::FORBIDDEN,
                        axum::Json(serde_json::json!({
                            "playerstats": {
                                "success": false,
                                "error": "Profile is not public"
                            }
                        })),
                    )
                }
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock_router).await;
    });

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("steam_sync_test.db");
    let static_dir = temp_dir.path().join("static");
    std::fs::create_dir_all(&static_dir).unwrap();

    let config = ServerConfig {
        port: 5050,
        api_key: "sync-token".to_string(),
        admin_password: "admin-pass-123".to_string(),
        db_path: db_path.clone(),
        avatar_path: temp_dir.path().join("avatar.png"),
        static_dir: static_dir.clone(),
        steam_api_key: Some("mock-steam-key".to_string()),
        steam_id: "76561198423209668".to_string(),
        steam_sync_cooldown_secs: 300,
        steam_api_base_url: format!("http://{}", mock_addr),
        ..ServerConfig::default()
    };

    // Seed DB with Bloodborne (5400m) + Palworld (2400m)
    let state = Arc::new(RwLock::new(AppState::with_database(&config, create_test_db())));
    let app = create_app(state.clone(), static_dir);

    // 1. Trigger Steam sync inline via ?wait=true
    let sync_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/sync/steam?wait=true")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sync_res.status(), StatusCode::OK);
    let sync_bytes = sync_res.into_body().collect().await.unwrap().to_bytes();
    let sync_json: serde_json::Value = serde_json::from_slice(&sync_bytes).unwrap();
    assert_eq!(sync_json["status"], "completed");
    assert_eq!(sync_json["games_fetched"], 3);
    assert_eq!(sync_json["games_with_trophies"], 1);
    assert_eq!(sync_json["achievements_skipped_or_private"], 2);
    assert_eq!(sync_json["total_games_in_db"], 4); // bloodborne + palworld + BG3 + private game
    assert_eq!(owned_calls.load(Ordering::SeqCst), 1);

    // 2. Verify SQLite games, trophies, and metadata tables
    let games_res = app
        .clone()
        .oneshot(Request::builder().uri("/api/games").method("GET").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let games_bytes = games_res.into_body().collect().await.unwrap().to_bytes();
    let games: Vec<serde_json::Value> = serde_json::from_slice(&games_bytes).unwrap();
    assert_eq!(games.len(), 4);

    // Bloodborne preserved
    let bb = games.iter().find(|g| g["id"] == "bloodborne").unwrap();
    assert_eq!(bb["playtime_minutes"], 5400);
    assert_eq!(bb["source"], "manual");

    // Palworld playtime protected at MAX(2400, 100) = 2400
    let pw = games.iter().find(|g| g["appid"] == "1623730").unwrap();
    assert_eq!(pw["playtime_minutes"], 2400);

    // BG3 trophies ingested
    let bg3_trophies_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/games/1086940/trophies")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bg3_trophies_bytes = bg3_trophies_res.into_body().collect().await.unwrap().to_bytes();
    let bg3_trophies: Vec<serde_json::Value> = serde_json::from_slice(&bg3_trophies_bytes).unwrap();
    assert_eq!(bg3_trophies.len(), 2);
    assert_eq!(bg3_trophies[0]["id"], "BG3_HERO");
    assert_eq!(bg3_trophies[0]["unlocked"], true);
    assert_eq!(bg3_trophies[0]["unlocked_time"], 1732000100);
    assert_eq!(bg3_trophies[1]["id"], "BG3_SECRET");
    assert_eq!(bg3_trophies[1]["unlocked"], false);

    // Metadata table persisted
    {
        let conn = trophyvault_core::db::open_or_create_db(&db_path).unwrap();
        let meta = trophyvault_core::db::get_all_metadata(&conn).unwrap();
        assert_eq!(meta.get("last_steam_sync_status").map(String::as_str), Some("ok"));
        assert!(meta.contains_key("last_steam_sync_at"));
        assert_eq!(meta.get("steam_persona_name").map(String::as_str), Some("MohandL3G"));
    }

    // 3. F5 / Repeat POST /api/sync/steam hits cooldown and does NOT call Valve API again!
    let cooldown_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/sync/steam")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cooldown_res.status(), StatusCode::OK);
    let cd_bytes = cooldown_res.into_body().collect().await.unwrap().to_bytes();
    let cd_json: serde_json::Value = serde_json::from_slice(&cd_bytes).unwrap();
    assert_eq!(cd_json["status"], "cooldown");
    assert!(cd_json["cooldown_remaining_secs"].as_u64().unwrap() > 0);
    assert_eq!(owned_calls.load(Ordering::SeqCst), 1);

    // 4. force=true without admin auth -> 401
    let force_unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/sync/steam?force=true")
                .method("POST")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(force_unauth.status(), StatusCode::UNAUTHORIZED);

    // 5. force=true with admin auth + non-blocking mode -> 202 Accepted
    let force_auth = app
        .oneshot(
            Request::builder()
                .uri("/api/sync/steam?force=true")
                .method("POST")
                .header("X-Admin-Password", "admin-pass-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(force_auth.status(), StatusCode::ACCEPTED);
    let force_bytes = force_auth.into_body().collect().await.unwrap().to_bytes();
    let force_json: serde_json::Value = serde_json::from_slice(&force_bytes).unwrap();
    assert_eq!(force_json["status"], "started");
}

