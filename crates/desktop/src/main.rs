#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use app::PassportApp;
use std::sync::Arc;

fn load_app_icon() -> Option<Arc<eframe::egui::IconData>> {
    let icon_bytes = include_bytes!("../assets/trophyvault-icon-256.png");
    if let Ok(img) = image::load_from_memory_with_format(icon_bytes, image::ImageFormat::Png) {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        Some(Arc::new(eframe::egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }))
    } else {
        None
    }
}

fn handle_sync_command() {
    let mut db = trophyvault_core::storage::load_active_user_database();

    // Scan and update local library first
    let report = trophyvault_core::storage::sync_library(&mut db);
    println!(
        "[TrophyVault] Local library scanned: {} games found ({} manual, {} steam updated)",
        report.total_games, report.manual_scanned, report.steam_updated
    );

    let sync_settings = match &db.preferences.sync_settings {
        Some(s) if s.is_configured() => s.clone(),
        _ => {
            eprintln!(
                "Error: Remote sync is not configured. Please launch TrophyVault GUI and configure the Server URL and API Key in Cloud Sync & Settings."
            );
            std::process::exit(1);
        }
    };

    println!(
        "[TrophyVault] Synchronizing {} games with remote server '{}'...",
        db.games.len(),
        sync_settings.server_url
    );

    match trophyvault_core::storage::sync_to_remote(&db, &sync_settings) {
        Ok(msg) => {
            println!("[TrophyVault] ✓ Sync successful: {}", msg);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if let Some(s) = &mut db.preferences.sync_settings {
                s.last_sync_timestamp = Some(now);
                s.last_sync_status = Some("Success".to_string());
            }
            let _ = trophyvault_core::storage::save_database_for_user(
                &db,
                db.preferences.steam_id.as_deref(),
            );
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("[TrophyVault] ✗ Sync failed: {}", err);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if let Some(s) = &mut db.preferences.sync_settings {
                s.last_sync_timestamp = Some(now);
                s.last_sync_status = Some(format!("Failed: {}", err));
            }
            let _ = trophyvault_core::storage::save_database_for_user(
                &db,
                db.preferences.steam_id.as_deref(),
            );
            std::process::exit(1);
        }
    }
}

fn handle_daemon_command() -> ! {
    println!("=== TrophyVault Remote Sync Daemon ===");
    println!("Continuous sync polling daemon active (interval: 60s)");
    println!("Press Ctrl+C to terminate");

    loop {
        let mut db = trophyvault_core::storage::load_active_user_database();
        let sync_settings_opt = db.preferences.sync_settings.clone();
        if let Some(settings) = sync_settings_opt {
            if settings.enabled && settings.is_configured() {
                let _report = trophyvault_core::storage::sync_library(&mut db);
                let _ = trophyvault_core::storage::save_database_for_user(
                    &db,
                    db.preferences.steam_id.as_deref(),
                );

                let target_url = settings.server_url.clone();
                match trophyvault_core::storage::sync_to_remote(&db, &settings) {
                    Ok(msg) => {
                        println!(
                            "[daemon] [{}] ✓ Synced {} games to '{}': {}",
                            chrono_like_time(),
                            db.games.len(),
                            target_url,
                            msg
                        );
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        if let Some(s) = &mut db.preferences.sync_settings {
                            s.last_sync_timestamp = Some(now);
                            s.last_sync_status = Some("Success".to_string());
                        }
                        let _ = trophyvault_core::storage::save_database_for_user(
                            &db,
                            db.preferences.steam_id.as_deref(),
                        );
                    }
                    Err(err) => {
                        eprintln!(
                            "[daemon] [{}] ✗ Sync to '{}' failed: {}",
                            chrono_like_time(),
                            target_url,
                            err
                        );
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        if let Some(s) = &mut db.preferences.sync_settings {
                            s.last_sync_timestamp = Some(now);
                            s.last_sync_status = Some(format!("Failed: {}", err));
                        }
                        let _ = trophyvault_core::storage::save_database_for_user(
                            &db,
                            db.preferences.steam_id.as_deref(),
                        );
                    }
                }
            } else if !settings.enabled {
                println!("[daemon] [{}] Cloud sync is disabled in settings.", chrono_like_time());
            } else {
                eprintln!(
                    "[daemon] [{}] Cloud sync unconfigured (missing server URL or API key).",
                    chrono_like_time()
                );
            }
        } else {
            println!("[daemon] [{}] No sync settings configured.", chrono_like_time());
        }

        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

fn chrono_like_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("TrophyVault - Luxury Gaming Achievements Passport Desktop");
        println!();
        println!("Usage:");
        println!("  TrophyVault                 Launch interactive GUI application");
        println!("  TrophyVault --sync          Perform immediate remote sync to configured web server and exit");
        println!("  TrophyVault --daemon        Run background file-watch / sync polling daemon");
        println!("  TrophyVault --help, -h      Show this help information");
        std::process::exit(0);
    }

    if args.iter().any(|a| a == "--sync") {
        handle_sync_command();
    }

    if args.iter().any(|a| a == "--daemon") {
        handle_daemon_command();
    }

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("TrophyVault")
        .with_inner_size([1280.0, 860.0])
        .with_min_inner_size([960.0, 640.0])
        .with_active(true);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "TrophyVault",
        native_options,
        Box::new(|cc| Ok(Box::new(PassportApp::new(cc)))),
    )
}
