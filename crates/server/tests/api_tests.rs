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
            playtime_minutes: 5400,
            last_played: Some(1730000000),
            source: "manual".to_string(),
            last_scanned: 1730000000,
            trophies: vec![SavedTrophy {
                id: 0,
                name: "Bloodborne".to_string(),
                description: "All trophies unlocked".to_string(),
                grade: "P".to_string(),
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
    let db_path = temp_dir.path().join("test_db.json");
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
        db_path: temp_dir.path().join("db.json"),
        avatar_path: temp_dir.path().join("avatar.png"),
        static_dir: static_dir.clone(),
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

    // Verify disk persistence
    let db_path = temp.path().join("test_db.json");
    assert!(db_path.exists());
    let saved_content = std::fs::read_to_string(&db_path).unwrap();
    assert!(saved_content.contains("UpdatedHunter"));
    assert!(saved_content.contains("Elden Ring"));

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
        db_path: temp_dir.path().join("db.json"),
        avatar_path: temp_dir.path().join("avatar.png"),
        static_dir: static_dir.clone(),
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

    // 6. Verify disk persistence
    let db_path = temp.path().join("test_db.json");
    let content = std::fs::read_to_string(&db_path).unwrap();
    let disk_db: LibraryDatabase = serde_json::from_str(&content).unwrap();
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
