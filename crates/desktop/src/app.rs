//! GUI Application built using eframe / egui with Layout 1 (Modern Left Sidebar).

#[path = "theme.rs"]
pub mod theme;

use trophyvault_core::data::{AchievementsData, GameItem};
use trophyvault_core::generator::{
    generate_passport_svg, render_svg_to_pixmap, PassportOptions, PassportTheme,
};
use trophyvault_core::storage::{
    database_to_achievements_data, get_database_path_for_user,
    load_or_create_database_for_user, purge_remote_database, restore_from_remote, save_database,
    sync_library, sync_to_remote_with_mode, LibraryDatabase, ManualGameSetting,
    SyncMode, SyncSettings,
};
use trophyvault_core::steam::{
    detect_steam_users, find_active_steam_id, find_steam_dir, parse_non_steam_shortcuts,
    NonSteamShortcut, SteamUserAccount,
};
use eframe::egui;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)]
pub type TrophyVaultApp = PassportApp;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppTab {
    PassportCanvas,
    LibraryStats,
    BloodbornePort,
    CloudSync,
    Settings,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PassportMode {
    Overall,
    SingleGame,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DbSortColumn {
    Title,
    Platform,
    AppId,
    Achievements,
    Progress,
    Playtime,
    Source,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SortDirection {
    Ascending,
    Descending,
}

pub struct PassportApp {
    db: LibraryDatabase,
    data: Option<AchievementsData>,
    active_tab: AppTab,

    // Multi-User Profile State
    detected_users: Vec<SteamUserAccount>,
    current_steam_id: Option<String>,
    custom_steam_id_input: String,
    show_custom_id_popup: bool,

    // Options for Canvas
    mode: PassportMode,
    selected_game_name: String,
    game_search_query: String,
    username: String,
    title_line: String,
    theme: PassportTheme,
    output_path: PathBuf,

    // Add Custom Game Form
    new_game_name: String,
    new_game_dir: String,
    new_game_appid: String,
    new_game_parser: String,
    detected_shortcuts: Vec<NonSteamShortcut>,

    // Database Manager search & cached keys
    db_search_query: String,
    cached_db_keys: Vec<String>,
    db_sort_col: DbSortColumn,
    db_sort_dir: SortDirection,

    // Feedback & Preview
    status_notification: Option<(String, bool)>, // (message, is_error)
    preview_texture: Option<egui::TextureHandle>,
    last_rendered_svg: Option<String>,

    // Cloud Sync UI state
    show_api_key_plaintext: bool,
    cloud_sync_status_msg: Option<(String, bool)>,
    has_synced_on_exit: bool,
    pub show_purge_confirm_modal: bool,
}

impl PassportApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Apply Cyberpunk Dark HUD visuals
        theme::apply_theme(&cc.egui_ctx);

        let steam_dir = find_steam_dir();
        let detected_users = if let Some(s_dir) = &steam_dir {
            detect_steam_users(s_dir)
        } else {
            Vec::new()
        };

        // Determine active Steam account
        let active_user = detected_users
            .iter()
            .find(|u| u.is_most_recent)
            .or_else(|| detected_users.iter().find(|u| u.steam_id == "462943940"))
            .or_else(|| detected_users.first());

        let initial_steam_id = active_user.map(|u| u.steam_id.clone());

        // Load isolated database for this specific user
        let mut db = load_or_create_database_for_user(initial_steam_id.as_deref());
        if db.preferences.steam_id.is_none() {
            db.preferences.steam_id = initial_steam_id.clone();
        }

        // If user has a persona name and DB username is default, initialize with persona
        if let Some(user) = active_user {
            if db.preferences.username == "MOHANDL3G" && user.persona_name != "MOHANDL3G" {
                db.preferences.username = user.persona_name.clone();
            }
        }

        // Sync database on startup for this user
        let _report = sync_library(&mut db);

        let username = db.preferences.username.clone();
        let title_line = db.preferences.title_line.clone();

        let theme = match db.preferences.theme.as_str() {
            "TrophyGold" => PassportTheme::TrophyGold,
            "BloodborneCrimson" => PassportTheme::BloodborneCrimson,
            "EmeraldMatrix" => PassportTheme::EmeraldMatrix,
            "SteamPurple" => PassportTheme::SteamPurple,
            _ => PassportTheme::CyberpunkCyan,
        };

        let default_output = db
            .preferences
            .export_path
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| !p.ends_with("achievements_passport.png"))
            .unwrap_or_else(|| {
                dirs::picture_dir()
                    .or_else(dirs::desktop_dir)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("TrophyVault.png")
            });

        let mut app = Self {
            db,
            data: None,
            active_tab: AppTab::PassportCanvas,

            detected_users,
            current_steam_id: initial_steam_id,
            custom_steam_id_input: String::new(),
            show_custom_id_popup: false,

            mode: PassportMode::Overall,
            selected_game_name: "Bloodborne".to_string(),
            game_search_query: String::new(),
            username,
            title_line,
            theme,
            output_path: default_output,

            new_game_name: "Bloodborne".to_string(),
            new_game_dir: r"C:\Games\Decomps\Bloodborne\user".to_string(),
            new_game_appid: "2485153835".to_string(),
            new_game_parser: "bbport".to_string(),
            detected_shortcuts: Vec::new(),

            db_search_query: String::new(),
            cached_db_keys: Vec::new(),
            db_sort_col: DbSortColumn::Progress,
            db_sort_dir: SortDirection::Descending,

            status_notification: None,
            preview_texture: None,
            last_rendered_svg: None,

            show_api_key_plaintext: false,
            cloud_sync_status_msg: None,
            has_synced_on_exit: false,
            show_purge_confirm_modal: false,
        };

        if app.db.preferences.sync_settings.is_none() {
            app.db.preferences.sync_settings = Some(SyncSettings::default());
        }

        app.rebuild_cached_db_keys();
        app.refresh_data_and_preview(&cc.egui_ctx);
        app
    }

    fn rebuild_cached_db_keys(&mut self) {
        let mut keys: Vec<String> = self.db.games.keys().cloned().collect();
        let sort_col = self.db_sort_col;
        let sort_dir = self.db_sort_dir;

        keys.sort_by(|a, b| {
            let ga = match self.db.games.get(a) {
                Some(g) => g,
                None => return std::cmp::Ordering::Equal,
            };
            let gb = match self.db.games.get(b) {
                Some(g) => g,
                None => return std::cmp::Ordering::Equal,
            };

            let ord = match sort_col {
                DbSortColumn::Title => ga.name.to_lowercase().cmp(&gb.name.to_lowercase()),
                DbSortColumn::Platform => ga.platform.to_lowercase().cmp(&gb.platform.to_lowercase()),
                DbSortColumn::AppId => ga.appid.cmp(&gb.appid),
                DbSortColumn::Achievements => ga.unlocked.cmp(&gb.unlocked),
                DbSortColumn::Progress => ga.pct.cmp(&gb.pct).then_with(|| ga.unlocked.cmp(&gb.unlocked)),
                DbSortColumn::Playtime => ga.playtime_minutes.cmp(&gb.playtime_minutes),
                DbSortColumn::Source => ga.source.cmp(&gb.source),
            };

            match sort_dir {
                SortDirection::Ascending => ord.then_with(|| ga.name.cmp(&gb.name)),
                SortDirection::Descending => ord.reverse().then_with(|| ga.name.cmp(&gb.name)),
            }
        });
        self.cached_db_keys = keys;
    }

    fn refresh_data_and_preview(&mut self, ctx: &egui::Context) {
        let data = database_to_achievements_data(&self.db);
        if self.selected_game_name.is_empty() {
            if let Some(first) = data.games.first() {
                self.selected_game_name = first.name.clone();
            }
        }
        self.data = Some(data);
        self.generate_and_render_preview(ctx);
    }

    fn run_sync(&mut self, ctx: &egui::Context) {
        let report = sync_library(&mut self.db);
        self.status_notification = Some((
            format!(
                "✓ Library Synced: {} games in database • {} manual • {} updated",
                report.total_games, report.manual_scanned, report.steam_updated
            ),
            false,
        ));
        self.rebuild_cached_db_keys();
        self.refresh_data_and_preview(ctx);
    }

    fn selected_game(&self) -> Option<&GameItem> {
        let data = self.data.as_ref()?;
        data.games.iter().find(|g| g.name == self.selected_game_name)
    }

    fn generate_svg(&self) -> Option<String> {
        let data = self.data.as_ref()?;
        let selected_game = if self.mode == PassportMode::SingleGame {
            self.selected_game()
        } else {
            None
        };

        let options = PassportOptions {
            username: &self.username,
            title: &self.title_line,
            theme: self.theme,
            selected_game,
        };

        Some(generate_passport_svg(data, &options))
    }

    fn generate_and_render_preview(&mut self, ctx: &egui::Context) {
        if let Some(svg) = self.generate_svg() {
            self.last_rendered_svg = Some(svg.clone());
            // Zero-copy render directly from tiny_skia Pixmap buffer into egui texture
            match render_svg_to_pixmap(&svg, 1200, 850) {
                Ok(pixmap) => {
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [1200, 850],
                        pixmap.data(),
                    );
                    self.preview_texture = Some(ctx.load_texture(
                        "passport_preview",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
                Err(err) => {
                    self.status_notification = Some((format!("Render error: {}", err), true));
                }
            }
        }
    }

    fn save_png(&mut self, ctx: &egui::Context) {
        if let Some(svg) = self.generate_svg() {
            self.last_rendered_svg = Some(svg.clone());
            match render_svg_to_pixmap(&svg, 1200, 850) {
                Ok(pixmap) => {
                    match pixmap.encode_png() {
                        Ok(bytes) => {
                            if let Some(parent) = self.output_path.parent() {
                                let _ = fs::create_dir_all(parent);
                            }
                            if let Err(e) = fs::write(&self.output_path, bytes) {
                                self.status_notification =
                                    Some((format!("Failed to save image: {}", e), true));
                            } else {
                                self.status_notification = Some((
                                    format!("✓ Passport saved to: {}", self.output_path.display()),
                                    false,
                                ));
                            }
                        }
                        Err(e) => {
                            self.status_notification =
                                Some((format!("PNG Encode Error: {}", e), true));
                        }
                    }

                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [1200, 850],
                        pixmap.data(),
                    );
                    self.preview_texture = Some(ctx.load_texture(
                        "passport_preview",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
                Err(err) => {
                    self.status_notification = Some((format!("Render error: {}", err), true));
                }
            }
        }
    }

    fn save_svg(&mut self) {
        if let Some(svg) = self.generate_svg() {
            let svg_path = self.output_path.with_extension("svg");
            if let Some(parent) = svg_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(e) = fs::write(&svg_path, svg) {
                self.status_notification = Some((format!("Failed to save SVG: {}", e), true));
            } else {
                self.status_notification = Some((
                    format!("✓ Vector SVG saved to: {}", svg_path.display()),
                    false,
                ));
            }
        }
    }

    pub fn current_account_display_name(&self) -> String {
        if let Some(current_id) = &self.current_steam_id {
            if let Some(user) = self.detected_users.iter().find(|u| &u.steam_id == current_id) {
                format!("{} ({})", user.persona_name, user.steam_id)
            } else {
                format!("Steam ID {}", current_id)
            }
        } else {
            "Default Profile".to_string()
        }
    }

    pub fn switch_user_profile(&mut self, new_steam_id: Option<String>, ctx: &egui::Context) {
        if self.current_steam_id == new_steam_id {
            return;
        }

        // 1. Save previous database
        if let Err(e) = save_database(&self.db) {
            self.status_notification = Some((format!("Failed to save previous profile: {}", e), true));
        }

        // 2. Load newly selected user's isolated database
        self.current_steam_id = new_steam_id.clone();
        self.db = load_or_create_database_for_user(new_steam_id.as_deref());
        if self.db.preferences.steam_id.is_none() {
            self.db.preferences.steam_id = new_steam_id.clone();
        }

        // 3. Update active UI state from new profile
        if let Some(user) = self
            .detected_users
            .iter()
            .find(|u| Some(&u.steam_id) == self.current_steam_id.as_ref())
        {
            if self.db.preferences.username == "MOHANDL3G" && user.persona_name != "MOHANDL3G" {
                self.db.preferences.username = user.persona_name.clone();
            }
        }
        self.username = self.db.preferences.username.clone();
        self.title_line = self.db.preferences.title_line.clone();
        self.theme = match self.db.preferences.theme.as_str() {
            "TrophyGold" => PassportTheme::TrophyGold,
            "BloodborneCrimson" => PassportTheme::BloodborneCrimson,
            "EmeraldMatrix" => PassportTheme::EmeraldMatrix,
            "SteamPurple" => PassportTheme::SteamPurple,
            _ => PassportTheme::CyberpunkCyan,
        };

        if let Some(p) = &self.db.preferences.export_path {
            self.output_path = PathBuf::from(p);
        }

        // 4. Sync library for new profile
        let report = sync_library(&mut self.db);

        // 5. Rebuild cached keys & refresh preview
        self.rebuild_cached_db_keys();
        self.refresh_data_and_preview(ctx);

        let display_name = self.current_account_display_name();
        self.status_notification = Some((
            format!(
                "✓ Switched profile to: {} ({} games loaded)",
                display_name, report.total_games
            ),
            false,
        ));
    }

    fn detect_steam_shortcuts(&mut self) {
        if let Some(s_dir) = find_steam_dir() {
            let active_id = self
                .current_steam_id
                .as_deref()
                .or_else(|| self.db.preferences.steam_id.as_deref());
            let target_id = active_id
                .map(str::to_string)
                .or_else(|| find_active_steam_id(&s_dir));

            if let Some(s_id) = target_id {
                self.detected_shortcuts = parse_non_steam_shortcuts(&s_dir, &s_id);
                if self.detected_shortcuts.is_empty() {
                    self.status_notification = Some((
                        format!("No Non-Steam shortcuts found in Steam config for account {}", s_id),
                        false,
                    ));
                } else {
                    self.status_notification = Some((
                        format!(
                            "Found {} Non-Steam shortcut(s) for account {}",
                            self.detected_shortcuts.len(),
                            s_id
                        ),
                        false,
                    ));
                }
                return;
            }
        }
        self.status_notification = Some((
            "Steam directory or active user ID could not be detected".to_string(),
            true,
        ));
    }

    #[allow(dead_code)]
    pub fn trigger_cloud_sync(&mut self) {
        let mode = self
            .db
            .preferences
            .sync_settings
            .as_ref()
            .map(|s| s.sync_mode)
            .unwrap_or_default();
        self.trigger_cloud_sync_with_mode(mode);
    }

    pub fn trigger_cloud_sync_with_mode(&mut self, mode: SyncMode) {
        let settings = match &self.db.preferences.sync_settings {
            Some(s) => s.clone(),
            None => {
                let msg = "Cloud sync settings not configured.".to_string();
                self.status_notification = Some((msg.clone(), true));
                self.cloud_sync_status_msg = Some((msg, true));
                return;
            }
        };

        if !settings.is_configured() {
            let msg = "Cloud sync failed: Server URL or API key is not configured.".to_string();
            self.status_notification = Some((msg.clone(), true));
            self.cloud_sync_status_msg = Some((msg, true));
            return;
        }

        let mode_desc = match mode {
            SyncMode::Updates => "Updates",
            SyncMode::Full => "Full",
        };

        match sync_to_remote_with_mode(&self.db, &settings, mode) {
            Ok(resp_msg) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if let Some(s) = &mut self.db.preferences.sync_settings {
                    s.last_sync_timestamp = Some(now);
                    s.last_sync_status = Some(format!("Success ({})", mode_desc));
                }
                let _ = save_database(&self.db);
                let ok_msg = format!("✓ Cloud sync ({}) successful: {}", mode_desc, resp_msg);
                self.status_notification = Some((ok_msg.clone(), false));
                self.cloud_sync_status_msg = Some((ok_msg, false));
            }
            Err(err_msg) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if let Some(s) = &mut self.db.preferences.sync_settings {
                    s.last_sync_timestamp = Some(now);
                    s.last_sync_status = Some(format!("Failed ({}): {}", mode_desc, err_msg));
                }
                let _ = save_database(&self.db);
                let fail_msg = format!("✗ Cloud sync ({}) failed: {}", mode_desc, err_msg);
                self.status_notification = Some((fail_msg.clone(), true));
                self.cloud_sync_status_msg = Some((fail_msg, true));
            }
        }
    }

    /// Invokes `restore_from_remote` on storage to hydrate historical library & playtime on a fresh install.
    pub fn trigger_restore_from_remote(&mut self, ctx: &egui::Context) {
        let settings = match &self.db.preferences.sync_settings {
            Some(s) => s.clone(),
            None => {
                let msg = "Remote restore failed: Cloud sync settings not configured.".to_string();
                self.status_notification = Some((msg.clone(), true));
                self.cloud_sync_status_msg = Some((msg, true));
                return;
            }
        };

        if !settings.is_configured() {
            let msg = "Remote restore failed: Server URL or API key is not configured.".to_string();
            self.status_notification = Some((msg.clone(), true));
            self.cloud_sync_status_msg = Some((msg, true));
            return;
        }

        match restore_from_remote(&mut self.db, &settings) {
            Ok(resp_msg) => {
                let _ = save_database(&self.db);
                self.rebuild_cached_db_keys();
                self.refresh_data_and_preview(ctx);
                let ok_msg = format!("✓ Restored from Cloud: {}", resp_msg);
                self.status_notification = Some((ok_msg.clone(), false));
                self.cloud_sync_status_msg = Some((ok_msg, false));
            }
            Err(err_msg) => {
                let fail_msg = format!("✗ Restore from Cloud failed: {}", err_msg);
                self.status_notification = Some((fail_msg.clone(), true));
                self.cloud_sync_status_msg = Some((fail_msg, true));
            }
        }
    }

    pub fn trigger_purge_remote_db(&mut self) {
        let settings = match &self.db.preferences.sync_settings {
            Some(s) => s.clone(),
            None => {
                let msg = "Remote purge failed: Cloud sync settings not configured.".to_string();
                self.status_notification = Some((msg.clone(), true));
                self.cloud_sync_status_msg = Some((msg, true));
                return;
            }
        };

        if !settings.is_configured() {
            let msg = "Remote purge failed: Server URL or API key is not configured.".to_string();
            self.status_notification = Some((msg.clone(), true));
            self.cloud_sync_status_msg = Some((msg, true));
            return;
        }

        match purge_remote_database(&settings) {
            Ok(resp_msg) => {
                let ok_msg = format!("✓ Remote database purged successfully: {}", resp_msg);
                self.status_notification = Some((ok_msg.clone(), false));
                self.cloud_sync_status_msg = Some((ok_msg, false));
            }
            Err(err_msg) => {
                let fail_msg = format!("✗ Remote database purge failed: {}", err_msg);
                self.status_notification = Some((fail_msg.clone(), true));
                self.cloud_sync_status_msg = Some((fail_msg, true));
            }
        }
    }

    fn perform_exit_sync(&mut self) {
        if self.has_synced_on_exit {
            return;
        }
        self.has_synced_on_exit = true;

        if let Some(settings) = &self.db.preferences.sync_settings {
            if settings.auto_sync_on_exit && settings.is_configured() {
                eprintln!(
                    "[TrophyVault] Performing auto-sync on exit to {} (mode: {:?})...",
                    settings.server_url, settings.sync_mode
                );
                match sync_to_remote_with_mode(&self.db, settings, settings.sync_mode) {
                    Ok(msg) => eprintln!("[TrophyVault] Auto-sync on exit succeeded: {}", msg),
                    Err(err) => eprintln!("[TrophyVault] Auto-sync on exit failed: {}", err),
                }
            }
        }
    }
}

// --- Layout 1 Implementation ---
impl eframe::App for PassportApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Layout 1: Vertical Sidebar on the Left (~240px wide)
        self.render_left_sidebar(ctx);

        // Right Area: Main Canvas / View for the Selected Tab
        match self.active_tab {
            AppTab::PassportCanvas => self.show_canvas_tab(ctx),
            AppTab::LibraryStats => self.show_database_tab(ctx),
            AppTab::BloodbornePort => self.show_custom_games_tab(ctx),
            AppTab::CloudSync => self.render_cloud_sync_tab(ctx),
            AppTab::Settings => self.render_settings_tab(ctx),
        }

        // Modals
        self.render_custom_steam_id_modal(ctx);
        self.render_purge_confirm_modal(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.perform_exit_sync();
    }
}

impl Drop for PassportApp {
    fn drop(&mut self) {
        self.perform_exit_sync();
    }
}

impl PassportApp {
    /// Renders the Layout 1 modern vertical left sidebar (~240px wide).
    fn render_left_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left_sidebar_panel")
            .resizable(false)
            .exact_width(240.0)
            .frame(theme::sidebar_frame())
            .show(ctx, |ui| {
                // Top section: Branding header & nav buttons
                ui.vertical(|ui| {
                    // Header: TrophyVault branding, version, subtle glow
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("🏆 TrophyVault")
                                .size(20.0)
                                .color(theme::COLOR_ACCENT_CYAN)
                                .strong(),
                        );
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("v{} • CYBERPUNK HUD", env!("CARGO_PKG_VERSION")))
                                .size(10.5)
                                .color(theme::COLOR_TEXT_MUTED)
                                .monospace(),
                        );
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(14.0);

                    // Nav section title
                    ui.label(
                        egui::RichText::new("MAIN MENU")
                            .size(10.0)
                            .color(theme::COLOR_TEXT_MUTED)
                            .extra_letter_spacing(1.2),
                    );
                    ui.add_space(8.0);

                    // Vertical Navigation Buttons
                    if self.render_nav_item(ui, "📇", "Passport Canvas", self.active_tab == AppTab::PassportCanvas) {
                        self.active_tab = AppTab::PassportCanvas;
                    }
                    if self.render_nav_item(ui, "📚", "Library & Stats", self.active_tab == AppTab::LibraryStats) {
                        self.active_tab = AppTab::LibraryStats;
                    }
                    if self.render_nav_item(ui, "🩸", "Bloodborne Port", self.active_tab == AppTab::BloodbornePort) {
                        self.active_tab = AppTab::BloodbornePort;
                    }
                    if self.render_nav_item(ui, "☁", "Cloud Sync", self.active_tab == AppTab::CloudSync) {
                        self.active_tab = AppTab::CloudSync;
                    }
                    if self.render_nav_item(ui, "⚙", "Settings", self.active_tab == AppTab::Settings) {
                        self.active_tab = AppTab::Settings;
                    }
                });

                // Bottom Footer: Gamer Profile Card
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(8.0);
                    self.render_gamer_profile_card(ui);
                    ui.add_space(6.0);
                    ui.separator();
                });
            });
    }

    /// Helper to render stacked navigation buttons with cyan accent glow/pill background when active.
    fn render_nav_item(&self, ui: &mut egui::Ui, icon: &str, title: &str, is_active: bool) -> bool {
        let desired_size = egui::vec2(ui.available_width(), 38.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
        let is_hovered = response.hovered();

        let bg_color = if is_active {
            theme::COLOR_CYAN_PILL_BG
        } else if is_hovered {
            egui::Color32::from_rgb(26, 36, 52)
        } else {
            egui::Color32::TRANSPARENT
        };

        let stroke = if is_active {
            egui::Stroke::new(1.5_f32, theme::COLOR_ACCENT_CYAN)
        } else if is_hovered {
            egui::Stroke::new(1.0_f32, theme::COLOR_BORDER_SLATE)
        } else {
            egui::Stroke::NONE
        };

        ui.painter().rect(rect, egui::Rounding::same(8.0_f32), bg_color, stroke);

        let text_color = if is_active {
            theme::COLOR_ACCENT_CYAN
        } else if is_hovered {
            theme::COLOR_TEXT_PRIMARY
        } else {
            theme::COLOR_TEXT_SECONDARY
        };

        let content_rect = rect.shrink2(egui::vec2(12.0_f32, 0.0_f32));
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(
                    egui::RichText::new(icon)
                        .size(15.0)
                        .color(if is_active { theme::COLOR_ACCENT_CYAN } else { theme::COLOR_TEXT_MUTED }),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(title)
                        .size(13.0)
                        .strong()
                        .color(text_color),
                );
            });
        });

        ui.add_space(3.0);
        response.clicked()
    }

    /// Renders the gamer profile card at the bottom of the sidebar.
    fn render_gamer_profile_card(&mut self, ui: &mut egui::Ui) {
        let ovr_score = self.data.as_ref().map(|d| d.stats.ovr_score).unwrap_or(0);
        let total_games = self.data.as_ref().map(|d| d.stats.total_configs).unwrap_or(0);

        let (dot_color, sync_tooltip) = match &self.db.preferences.sync_settings {
            Some(s) if s.last_sync_status.as_deref().map(|st| st.starts_with("Success")).unwrap_or(false) => {
                (theme::COLOR_SUCCESS_GREEN, "Cloud Sync: Online & Synced")
            }
            Some(s) if s.last_sync_status.is_some() => {
                (theme::COLOR_ERROR_RED, "Cloud Sync: Last attempt failed")
            }
            _ => (theme::COLOR_TEXT_MUTED, "Cloud Sync: Idle / Unconfigured"),
        };

        let card_frame = theme::card_frame();
        let desired_width = ui.available_width();

        let response = card_frame.show(ui, |ui| {
            ui.set_width(desired_width);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("🎮")
                        .size(20.0)
                        .color(theme::COLOR_ACCENT_CYAN),
                );
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&self.username)
                                .size(13.0)
                                .strong()
                                .color(theme::COLOR_TEXT_PRIMARY),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            egui::RichText::new("●")
                                .size(11.0)
                                .color(dot_color),
                        )
                        .on_hover_text(sync_tooltip);
                    });
                    ui.label(
                        egui::RichText::new(format!("{} OVR • {} Games", ovr_score, total_games))
                            .size(11.0)
                            .color(theme::COLOR_ACCENT_GOLD),
                    );
                });
            });
        }).response;

        if response.on_hover_text("Click to open Settings & Profiles").clicked() {
            self.active_tab = AppTab::Settings;
        }
    }

    /// Renders global notification toast if present.
    fn render_notification_bar(&mut self, ui: &mut egui::Ui) {
        if let Some((msg, is_err)) = &self.status_notification {
            let bg_color = if *is_err {
                egui::Color32::from_rgba_premultiplied(239, 68, 68, 35)
            } else {
                egui::Color32::from_rgba_premultiplied(16, 185, 129, 35)
            };
            let text_color = if *is_err {
                theme::COLOR_ERROR_RED
            } else {
                theme::COLOR_SUCCESS_GREEN
            };

            let frame = egui::Frame::none()
                .fill(bg_color)
                .stroke(egui::Stroke::new(1.0_f32, text_color))
                .rounding(egui::Rounding::same(8.0_f32))
                .inner_margin(egui::Margin::symmetric(12.0_f32, 6.0_f32));

            let mut dismiss = false;
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(msg).color(text_color).size(13.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✖").clicked() {
                            dismiss = true;
                        }
                    });
                });
            });
            ui.add_space(6.0);

            if dismiss {
                self.status_notification = None;
            }
        }
    }

    // --- TAB: Passport Canvas ---
    fn show_canvas_tab(&mut self, ctx: &egui::Context) {
        let mut selection_changed = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_notification_bar(ui);

            ui.horizontal(|ui| {
                // Left Column: Controls (Mode, Themes, Player, Export)
                ui.allocate_ui_with_layout(
                    egui::vec2(340.0, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("canvas_ctrl_scroll")
                            .show(ui, |ui| {
                                ui.add_space(4.0);

                                // PASSPORT MODE
                                theme::card_frame().show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        egui::RichText::new("🎯 Passport Mode")
                                            .strong()
                                            .color(theme::COLOR_ACCENT_CYAN),
                                    );
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        let r1 = ui.radio_value(
                                            &mut self.mode,
                                            PassportMode::Overall,
                                            "Overall Library",
                                        );
                                        let r2 = ui.radio_value(
                                            &mut self.mode,
                                            PassportMode::SingleGame,
                                            "Single Game",
                                        );
                                        if r1.changed() || r2.changed() {
                                            selection_changed = true;
                                        }
                                    });

                                    if self.mode == PassportMode::SingleGame {
                                        ui.add_space(6.0);
                                        ui.label("Search Game:");
                                        ui.text_edit_singleline(&mut self.game_search_query);

                                        if let Some(data) = &self.data {
                                            let query = self.game_search_query.to_lowercase();
                                            let filtered_games: Vec<String> = data
                                                .games
                                                .iter()
                                                .filter(|g| query.is_empty() || g.name.to_lowercase().contains(&query))
                                                .map(|g| g.name.clone())
                                                .collect();

                                            let mut chosen_game = None;
                                            egui::ComboBox::from_label("Game")
                                                .selected_text(&self.selected_game_name)
                                                .show_ui(ui, |ui| {
                                                    for game_name in filtered_games.iter().take(40) {
                                                        let is_selected = self.selected_game_name == *game_name;
                                                        if ui.selectable_label(is_selected, game_name).clicked() {
                                                            chosen_game = Some(game_name.clone());
                                                        }
                                                    }
                                                });

                                            if let Some(game_name) = chosen_game {
                                                self.selected_game_name = game_name;
                                                selection_changed = true;
                                            }

                                            if let Some(game) = self.selected_game() {
                                                ui.add_space(6.0);
                                                let playtime_display = format!(
                                                    "{}h {}m",
                                                    game.playtime_minutes / 60,
                                                    game.playtime_minutes % 60
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "⏱ Playtime: {} • 🏆 Achievements: {}/{} ({}%)",
                                                        playtime_display, game.unlocked, game.total, game.pct
                                                    ))
                                                    .size(11.0)
                                                    .color(theme::COLOR_ACCENT_CYAN),
                                                );
                                            }
                                        }
                                    }
                                });

                                ui.add_space(8.0);

                                // VISUAL THEME
                                theme::card_frame().show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        egui::RichText::new("🎨 Visual Theme")
                                            .strong()
                                            .color(theme::COLOR_ACCENT_CYAN),
                                    );
                                    ui.add_space(4.0);

                                    let mut chosen_theme = None;
                                    egui::ComboBox::from_label("Preset")
                                        .selected_text(self.theme.name())
                                        .show_ui(ui, |ui| {
                                            for t in PassportTheme::all() {
                                                let is_selected = self.theme == t;
                                                if ui.selectable_label(is_selected, t.name()).clicked() {
                                                    chosen_theme = Some(t);
                                                }
                                            }
                                        });

                                    if let Some(t) = chosen_theme {
                                        self.theme = t;
                                        self.db.preferences.theme = match t {
                                            PassportTheme::TrophyGold => "TrophyGold".into(),
                                            PassportTheme::BloodborneCrimson => "BloodborneCrimson".into(),
                                            PassportTheme::EmeraldMatrix => "EmeraldMatrix".into(),
                                            PassportTheme::SteamPurple => "SteamPurple".into(),
                                            _ => "CyberpunkCyan".into(),
                                        };
                                        let _ = save_database(&self.db);
                                        selection_changed = true;
                                    }
                                });

                                ui.add_space(8.0);

                                // PLAYER PROFILE DETAILS
                                theme::card_frame().show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        egui::RichText::new("👤 Player Tag & Subtitle")
                                            .strong()
                                            .color(theme::COLOR_ACCENT_CYAN),
                                    );
                                    ui.add_space(4.0);

                                    ui.label("Player Tag / Username:");
                                    let user_res = ui.text_edit_singleline(&mut self.username);
                                    if user_res.changed() {
                                        selection_changed = true;
                                    }
                                    if user_res.lost_focus() {
                                        self.db.preferences.username = self.username.clone();
                                        let _ = save_database(&self.db);
                                    }

                                    ui.add_space(4.0);
                                    ui.label("Passport Subtitle:");
                                    let title_res = ui.text_edit_singleline(&mut self.title_line);
                                    if title_res.changed() {
                                        selection_changed = true;
                                    }
                                    if title_res.lost_focus() {
                                        self.db.preferences.title_line = self.title_line.clone();
                                        let _ = save_database(&self.db);
                                    }

                                    ui.horizontal(|ui| {
                                        if ui.button("Hunter of Yharnam").clicked() {
                                            self.title_line = "Hunter of Yharnam   •   Trophy Hunter".to_string();
                                            self.db.preferences.title_line = self.title_line.clone();
                                            let _ = save_database(&self.db);
                                            selection_changed = true;
                                        }
                                        if ui.button("Clear").clicked() {
                                            self.title_line.clear();
                                            self.db.preferences.title_line = self.title_line.clone();
                                            let _ = save_database(&self.db);
                                            selection_changed = true;
                                        }
                                    });
                                });

                                ui.add_space(8.0);

                                // EXPORT & SAVE
                                theme::card_frame().show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        egui::RichText::new("💾 Export Passport")
                                            .strong()
                                            .color(theme::COLOR_ACCENT_CYAN),
                                    );
                                    ui.add_space(4.0);

                                    ui.label("Export Destination:");
                                    ui.horizontal(|ui| {
                                        let mut path_str = self.output_path.to_string_lossy().to_string();
                                        if ui.text_edit_singleline(&mut path_str).changed() {
                                            self.output_path = PathBuf::from(&path_str);
                                            self.db.preferences.export_path = Some(path_str);
                                            let _ = save_database(&self.db);
                                        }
                                        if ui.button("Browse…").clicked() {
                                            if let Some(path) = rfd::FileDialog::new()
                                                .set_file_name("TrophyVault.png")
                                                .save_file()
                                            {
                                                self.output_path = path.clone();
                                                self.db.preferences.export_path =
                                                    Some(path.to_string_lossy().to_string());
                                                let _ = save_database(&self.db);
                                            }
                                        }
                                    });

                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_sized([140.0, 32.0], egui::Button::new("📸 Export PNG"))
                                            .clicked()
                                        {
                                            self.save_png(ctx);
                                        }
                                        if ui
                                            .add_sized([140.0, 32.0], egui::Button::new("🎨 Export SVG"))
                                            .clicked()
                                        {
                                            self.save_svg();
                                        }
                                    });

                                    ui.add_space(6.0);
                                    let current_sync_mode = self
                                        .db
                                        .preferences
                                        .sync_settings
                                        .as_ref()
                                        .map(|s| s.sync_mode)
                                        .unwrap_or_default();
                                    if ui
                                        .add_sized([ui.available_width(), 30.0], egui::Button::new("☁ Sync to Web Now"))
                                        .clicked()
                                    {
                                        self.trigger_cloud_sync_with_mode(current_sync_mode);
                                    }
                                });

                                ui.add_space(6.0);
                                ui.hyperlink_to(
                                    egui::RichText::new("GitHub Repository ↗")
                                        .color(egui::Color32::from_rgb(88, 166, 255)),
                                    "https://github.com/MohandL3G/TrophyVault",
                                );
                            });
                    },
                );

                ui.separator();

                // Right Column: Canvas Live Preview
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    if let Some(texture) = &self.preview_texture {
                        let available_size = ui.available_size();
                        let aspect_ratio = 1200.0 / 850.0;
                        let mut target_width = available_size.x.max(400.0);
                        let mut target_height = target_width / aspect_ratio;

                        if target_height > available_size.y {
                            target_height = available_size.y.max(300.0);
                            target_width = target_height * aspect_ratio;
                        }

                        ui.image((texture.id(), egui::vec2(target_width, target_height)));
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Rendering passport preview...");
                        });
                    }
                });
            });
        });

        if selection_changed {
            self.generate_and_render_preview(ctx);
        }
    }

    // --- TAB: Library & Stats ---
    fn show_database_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_notification_bar(ui);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("📚 Library & Statistics")
                        .color(theme::COLOR_ACCENT_CYAN)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🔄 Sync Library").clicked() {
                        self.run_sync(ctx);
                    }
                    let db_path = get_database_path_for_user(self.current_steam_id.as_deref());
                    if ui.button("📂 Open Folder").clicked() {
                        if let Some(parent) = db_path.parent() {
                            let _ = open::that(parent);
                        }
                    }
                });
            });

            ui.add_space(4.0);

            // Summary Metric Cards
            if let Some(data) = &self.data {
                let total_playtime_minutes: u64 = self.db.games.values().map(|g| g.playtime_minutes as u64).sum();
                let hours = total_playtime_minutes / 60;
                let mins = total_playtime_minutes % 60;

                ui.horizontal(|ui| {
                    let card_w = (ui.available_width() - 36.0) / 4.0;

                    theme::card_frame().show(ui, |ui| {
                        ui.set_width(card_w);
                        ui.label(egui::RichText::new("TOTAL GAMES").size(10.0).color(theme::COLOR_TEXT_MUTED));
                        ui.heading(egui::RichText::new(format!("{}", data.stats.total_configs)).color(theme::COLOR_ACCENT_CYAN));
                    });
                    theme::card_frame().show(ui, |ui| {
                        ui.set_width(card_w);
                        ui.label(egui::RichText::new("ACHIEVEMENTS").size(10.0).color(theme::COLOR_TEXT_MUTED));
                        ui.heading(egui::RichText::new(format!("{}", data.stats.unlocked_achievements)).color(theme::COLOR_SUCCESS_GREEN));
                    });
                    theme::card_frame().show(ui, |ui| {
                        ui.set_width(card_w);
                        ui.label(egui::RichText::new("OVR SCORE").size(10.0).color(theme::COLOR_TEXT_MUTED));
                        ui.heading(egui::RichText::new(format!("{}", data.stats.ovr_score)).color(theme::COLOR_ACCENT_GOLD));
                    });
                    theme::card_frame().show(ui, |ui| {
                        ui.set_width(card_w);
                        ui.label(egui::RichText::new("TOTAL PLAYTIME").size(10.0).color(theme::COLOR_TEXT_MUTED));
                        ui.heading(egui::RichText::new(format!("{}h {}m", hours, mins)).color(theme::COLOR_TEXT_PRIMARY));
                    });
                });
                ui.add_space(8.0);
            }

            // Search Filter
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍 Filter Games:").color(theme::COLOR_TEXT_SECONDARY));
                ui.text_edit_singleline(&mut self.db_search_query);
            });

            ui.add_space(6.0);

            // Table Columns layout
            let avail_w = ui.available_width().max(880.0);
            let col_platform = 120.0;
            let col_appid = 90.0;
            let col_ach = 105.0;
            let col_pct = 80.0;
            let col_playtime = 95.0;
            let col_source = 85.0;
            let col_action = 80.0;
            let fixed_sum = col_platform + col_appid + col_ach + col_pct + col_playtime + col_source + col_action;
            let col_title = (avail_w - fixed_sum - 35.0).max(240.0);

            let mut switch_sort = None;
            let sort_arrow = |col: DbSortColumn| -> &'static str {
                if self.db_sort_col == col {
                    match self.db_sort_dir {
                        SortDirection::Ascending => " ▲",
                        SortDirection::Descending => " ▼",
                    }
                } else {
                    ""
                }
            };

            // Table Header Row
            ui.horizontal(|ui| {
                if ui.add_sized(
                    [col_title, 22.0],
                    egui::Button::new(
                        egui::RichText::new(format!("Game Title{}", sort_arrow(DbSortColumn::Title)))
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    )
                    .frame(false),
                ).clicked() {
                    switch_sort = Some(DbSortColumn::Title);
                }

                if ui.add_sized(
                    [col_platform, 22.0],
                    egui::Button::new(
                        egui::RichText::new(format!("Platform{}", sort_arrow(DbSortColumn::Platform)))
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    )
                    .frame(false),
                ).clicked() {
                    switch_sort = Some(DbSortColumn::Platform);
                }

                if ui.add_sized(
                    [col_appid, 22.0],
                    egui::Button::new(
                        egui::RichText::new(format!("App ID{}", sort_arrow(DbSortColumn::AppId)))
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    )
                    .frame(false),
                ).clicked() {
                    switch_sort = Some(DbSortColumn::AppId);
                }

                if ui.add_sized(
                    [col_ach, 22.0],
                    egui::Button::new(
                        egui::RichText::new(format!("Achievements{}", sort_arrow(DbSortColumn::Achievements)))
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    )
                    .frame(false),
                ).clicked() {
                    switch_sort = Some(DbSortColumn::Achievements);
                }

                if ui.add_sized(
                    [col_pct, 22.0],
                    egui::Button::new(
                        egui::RichText::new(format!("Progress{}", sort_arrow(DbSortColumn::Progress)))
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    )
                    .frame(false),
                ).clicked() {
                    switch_sort = Some(DbSortColumn::Progress);
                }

                if ui.add_sized(
                    [col_playtime, 22.0],
                    egui::Button::new(
                        egui::RichText::new(format!("Playtime{}", sort_arrow(DbSortColumn::Playtime)))
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    )
                    .frame(false),
                ).clicked() {
                    switch_sort = Some(DbSortColumn::Playtime);
                }

                if ui.add_sized(
                    [col_source, 22.0],
                    egui::Button::new(
                        egui::RichText::new(format!("Source{}", sort_arrow(DbSortColumn::Source)))
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    )
                    .frame(false),
                ).clicked() {
                    switch_sort = Some(DbSortColumn::Source);
                }

                ui.add_sized(
                    [col_action, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Actions")
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    ),
                );
            });

            if let Some(col) = switch_sort {
                if self.db_sort_col == col {
                    self.db_sort_dir = match self.db_sort_dir {
                        SortDirection::Ascending => SortDirection::Descending,
                        SortDirection::Descending => SortDirection::Ascending,
                    };
                } else {
                    self.db_sort_col = col;
                    self.db_sort_dir = match col {
                        DbSortColumn::Title | DbSortColumn::Platform | DbSortColumn::Source => SortDirection::Ascending,
                        _ => SortDirection::Descending,
                    };
                }
                self.rebuild_cached_db_keys();
            }
            ui.separator();

            let query = self.db_search_query.to_lowercase();
            let filtered_keys: Vec<String> = self
                .cached_db_keys
                .iter()
                .filter(|key| {
                    if query.is_empty() {
                        true
                    } else if let Some(game) = self.db.games.get(*key) {
                        game.name.to_lowercase().contains(&query)
                            || game.platform.to_lowercase().contains(&query)
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();

            let mut to_delete_key: Option<String> = None;
            let row_height = 24.0;
            let total_rows = filtered_keys.len();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    for idx in row_range {
                        let key = &filtered_keys[idx];
                        if let Some(game) = self.db.games.get(key) {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [col_title, row_height],
                                    egui::Label::new(&game.name).truncate(),
                                );
                                ui.add_sized(
                                    [col_platform, row_height],
                                    egui::Label::new(&game.platform).truncate(),
                                );
                                ui.add_sized(
                                    [col_appid, row_height],
                                    egui::Label::new(game.appid.as_deref().unwrap_or("—")).truncate(),
                                );
                                ui.add_sized(
                                    [col_ach, row_height],
                                    egui::Label::new(format!("{}/{}", game.unlocked, game.total)),
                                );
                                ui.add_sized(
                                    [col_pct, row_height],
                                    egui::Label::new(format!("{}%", game.pct)),
                                );
                                // STRICT CONSTRAINT: Read-only playtime display, no manual edits!
                                ui.add_sized(
                                    [col_playtime, row_height],
                                    egui::Label::new(format!(
                                        "{}h {}m",
                                        game.playtime_minutes / 60,
                                        game.playtime_minutes % 60
                                    )),
                                );
                                ui.add_sized(
                                    [col_source, row_height],
                                    egui::Label::new(&game.source),
                                );

                                if ui
                                    .add_sized([col_action, 20.0], egui::Button::new("🗑 Delete"))
                                    .clicked()
                                {
                                    to_delete_key = Some(key.clone());
                                }
                            });
                        }
                    }
                });

            if let Some(key) = to_delete_key {
                self.db.games.remove(&key);
                let _ = save_database(&self.db);
                self.rebuild_cached_db_keys();
                self.refresh_data_and_preview(ctx);
            }
        });
    }

    // --- TAB: Bloodborne Port ---
    fn show_custom_games_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_notification_bar(ui);

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(4.0);
                ui.heading(
                    egui::RichText::new("🩸 Bloodborne Port & Custom Decomps")
                        .color(theme::COLOR_ACCENT_CYAN)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "Manage custom ports and decompilations such as Bloodborne PC Port (bbport / ShadPS4). Trophies are read directly from your save folder, and playtime is synchronized automatically.",
                    )
                    .color(theme::COLOR_TEXT_SECONDARY),
                );

                ui.add_space(10.0);

                // Form: Add Custom Game
                theme::card_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.heading("➕ Add or Configure Custom Game");
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label("Game Title:");
                        ui.text_edit_singleline(&mut self.new_game_name);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Parser Type:");
                        egui::ComboBox::from_id_salt("parser_combo")
                            .selected_text(&self.new_game_parser)
                            .show_ui(ui, |ui| {
                                let is_selected = self.new_game_parser == "bbport";
                                if ui.selectable_label(is_selected, "Bloodborne / bbport (ShadPS4)").clicked() {
                                    self.new_game_parser = "bbport".to_string();
                                }
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("User / Save Folder:");
                        ui.text_edit_singleline(&mut self.new_game_dir);
                        if ui.button("📁 Browse…").clicked() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                self.new_game_dir = folder.to_string_lossy().to_string();
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Non-Steam AppID:");
                        ui.text_edit_singleline(&mut self.new_game_appid);
                        if ui.button("🔍 Detect from Steam Shortcuts").clicked() {
                            self.detect_steam_shortcuts();
                        }
                    });

                    if !self.detected_shortcuts.is_empty() {
                        ui.add_space(4.0);
                        ui.label("Discovered Steam Shortcuts:");
                        let mut chosen_shortcut = None;
                        for sc in &self.detected_shortcuts {
                            ui.horizontal(|ui| {
                                ui.label(format!("• {} (AppID: {})", sc.appname, sc.appid));
                                if ui.button("Use").clicked() {
                                    chosen_shortcut = Some((sc.appname.clone(), sc.appid));
                                }
                            });
                        }
                        if let Some((name, appid)) = chosen_shortcut {
                            self.new_game_appid = appid.to_string();
                            if self.new_game_name.is_empty() {
                                self.new_game_name = name;
                            }
                        }
                    }

                    ui.add_space(8.0);
                    if ui.button("💾 Save & Add Custom Game").clicked() {
                        let id = self
                            .new_game_name
                            .to_lowercase()
                            .replace(' ', "_")
                            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");

                        let appid_opt = if self.new_game_appid.trim().is_empty() {
                            None
                        } else {
                            Some(self.new_game_appid.trim().to_string())
                        };

                        if let Some(existing) = self.db.manual_games.iter_mut().find(|m| m.id == id) {
                            existing.name = self.new_game_name.clone();
                            existing.user_dir = self.new_game_dir.clone();
                            existing.non_steam_appid = appid_opt;
                            existing.enabled = true;
                        } else {
                            self.db.manual_games.push(ManualGameSetting {
                                id,
                                name: self.new_game_name.clone(),
                                platform: "PC Port / Recomp".to_string(),
                                parser_type: self.new_game_parser.clone(),
                                user_dir: self.new_game_dir.clone(),
                                non_steam_appid: appid_opt,
                                enabled: true,
                            });
                        }

                        let _ = save_database(&self.db);
                        self.run_sync(ctx);
                    }
                });

                ui.add_space(14.0);

                // Configured Games List
                ui.heading("Configured Custom Games");
                ui.add_space(4.0);

                let manual_games_clone = self.db.manual_games.clone();
                let mut rescan_needed = false;
                let mut to_remove = None;

                for (idx, cfg) in manual_games_clone.iter().enumerate() {
                    theme::card_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.heading(
                                egui::RichText::new(&cfg.name)
                                    .color(theme::COLOR_ACCENT_CYAN),
                            );
                            ui.label(format!("(ID: {}, Parser: {})", cfg.id, cfg.parser_type));
                        });
                        ui.label(format!("📁 Folder: {}", cfg.user_dir));
                        if let Some(appid) = &cfg.non_steam_appid {
                            ui.label(format!("🆔 Non-Steam AppID: {}", appid));
                        }

                        if let Some(game) = self.db.games.get(&cfg.id) {
                            ui.label(
                                egui::RichText::new(format!(
                                    "🏆 Trophies: {} / {} ({}%) • Playtime: {} hrs",
                                    game.unlocked,
                                    game.total,
                                    game.pct,
                                    game.playtime_minutes / 60
                                ))
                                .color(theme::COLOR_SUCCESS_GREEN),
                            );
                        }

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("🔄 Rescan Game").clicked() {
                                rescan_needed = true;
                            }
                            if ui.button("🗑 Remove").clicked() {
                                to_remove = Some(idx);
                            }
                        });
                    });
                    ui.add_space(6.0);
                }

                if rescan_needed {
                    self.run_sync(ctx);
                }

                if let Some(idx) = to_remove {
                    self.db.manual_games.remove(idx);
                    let _ = save_database(&self.db);
                    self.run_sync(ctx);
                }
            });
        });
    }

    // --- TAB: Cloud Sync ---
    fn render_cloud_sync_tab(&mut self, ctx: &egui::Context) {
        let mut pref_changed = false;
        let mut trigger_sync_mode: Option<SyncMode> = None;
        let mut trigger_restore = false;
        let mut trigger_purge_confirm = false;
        let mut toggle_plaintext = false;
        let show_plaintext = self.show_api_key_plaintext;
        let cloud_status = self.cloud_sync_status_msg.clone();

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_notification_bar(ui);

            ui.add_space(4.0);
            ui.heading(
                egui::RichText::new("☁ Cloud Sync & Web Integration")
                    .color(theme::COLOR_ACCENT_CYAN)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(
                    "Configure synchronization with your self-hosted TrophyVault server to back up your trophies and export dynamic badges.",
                )
                .color(theme::COLOR_TEXT_SECONDARY),
            );
            ui.add_space(10.0);

            let sync_settings = self
                .db
                .preferences
                .sync_settings
                .get_or_insert_with(SyncSettings::default);

            // Server Settings Card
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width().min(680.0));

                ui.label(
                    egui::RichText::new("🌐 Remote Server Settings")
                        .strong()
                        .color(theme::COLOR_ACCENT_CYAN),
                );
                ui.add_space(6.0);

                if ui.checkbox(&mut sync_settings.enabled, "Enable Remote Cloud Sync").changed() {
                    pref_changed = true;
                }
                ui.add_space(6.0);

                ui.label("Server URL (e.g. https://vault.mohandl3g.ly):");
                let url_res = ui.add(
                    egui::TextEdit::singleline(&mut sync_settings.server_url)
                        .hint_text("https://vault.mohandl3g.ly")
                        .desired_width(f32::INFINITY),
                );
                if url_res.changed() {
                    pref_changed = true;
                }

                ui.add_space(6.0);

                ui.label("API Bearer Token / Key:");
                ui.horizontal(|ui| {
                    let key_res = ui.add(
                        egui::TextEdit::singleline(&mut sync_settings.api_key)
                            .password(!show_plaintext)
                            .hint_text("Enter server API key")
                            .desired_width(420.0),
                    );
                    if key_res.changed() {
                        pref_changed = true;
                    }

                    let btn_label = if show_plaintext { "🙈 Hide" } else { "👁 Show" };
                    if ui.button(btn_label).clicked() {
                        toggle_plaintext = true;
                    }
                });

                ui.add_space(6.0);

                if ui
                    .checkbox(&mut sync_settings.auto_sync_on_exit, "Auto-sync on app exit")
                    .changed()
                {
                    pref_changed = true;
                }

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("🔄 Sync Mode:")
                        .strong()
                        .color(theme::COLOR_ACCENT_CYAN),
                );
                ui.horizontal(|ui| {
                    if ui
                        .radio_value(&mut sync_settings.sync_mode, SyncMode::Updates, "Incremental Updates [Safe]")
                        .changed()
                    {
                        pref_changed = true;
                    }
                    if ui
                        .radio_value(&mut sync_settings.sync_mode, SyncMode::Full, "Full Sync [Mirror Local]")
                        .changed()
                    {
                        pref_changed = true;
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "• Incremental Updates: Upserts games & unlocks without removing remote entries.\n• Full Sync: Mirrors your exact local library, removing deleted games.",
                    )
                    .size(11.0)
                    .italics()
                    .color(theme::COLOR_TEXT_MUTED),
                );
            });

            ui.add_space(10.0);

            // Sync Actions Card
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width().min(680.0));

                ui.label(
                    egui::RichText::new("⚡ Cloud Actions")
                        .strong()
                        .color(theme::COLOR_ACCENT_CYAN),
                );
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui
                        .add_sized(
                            [140.0, 34.0],
                            egui::Button::new(
                                egui::RichText::new("🔄 Sync Updates")
                                    .strong()
                                    .color(theme::COLOR_ACCENT_CYAN),
                            ),
                        )
                        .on_hover_text("Send incremental updates to remote server")
                        .clicked()
                    {
                        trigger_sync_mode = Some(SyncMode::Updates);
                    }

                    if ui
                        .add_sized(
                            [150.0, 34.0],
                            egui::Button::new(
                                egui::RichText::new("⚡ Full Sync (Mirror)")
                                    .strong()
                                    .color(theme::COLOR_ACCENT_GOLD),
                            ),
                        )
                        .on_hover_text("Completely overwrite remote server database with local library")
                        .clicked()
                    {
                        trigger_sync_mode = Some(SyncMode::Full);
                    }

                    // ⬇ Restore from Cloud Button: Invokes restore_from_remote
                    if ui
                        .add_sized(
                            [160.0, 34.0],
                            egui::Button::new(
                                egui::RichText::new("⬇ Restore from Cloud")
                                    .strong()
                                    .color(theme::COLOR_SUCCESS_GREEN),
                            ),
                        )
                        .on_hover_text("Hydrate historical library, achievements, and playtime from remote server on fresh install")
                        .clicked()
                    {
                        trigger_restore = true;
                    }

                    if ui
                        .add_sized(
                            [150.0, 34.0],
                            egui::Button::new(
                                egui::RichText::new("🗑 Purge Remote DB")
                                    .strong()
                                    .color(theme::COLOR_ERROR_RED),
                            ),
                        )
                        .on_hover_text("Purge and delete all games from remote web server database")
                        .clicked()
                    {
                        trigger_purge_confirm = true;
                    }
                });

                ui.add_space(8.0);

                if let Some(ts) = sync_settings.last_sync_timestamp {
                    let status_str = sync_settings.last_sync_status.as_deref().unwrap_or("Unknown");
                    let status_color = if status_str.starts_with("Success") {
                        theme::COLOR_SUCCESS_GREEN
                    } else {
                        theme::COLOR_ERROR_RED
                    };

                    ui.horizontal(|ui| {
                        ui.label("Last Sync Status:");
                        ui.label(egui::RichText::new(status_str).color(status_color).strong());
                    });
                    ui.horizontal(|ui| {
                        ui.label("Last Sync Timestamp (Unix):");
                        ui.label(egui::RichText::new(format!("{}", ts)).monospace().color(theme::COLOR_TEXT_SECONDARY));
                    });
                } else {
                    ui.label(
                        egui::RichText::new("Never synced yet.")
                            .italics()
                            .color(theme::COLOR_TEXT_MUTED),
                    );
                }

                if let Some((msg, is_err)) = &cloud_status {
                    ui.add_space(6.0);
                    let color = if *is_err { theme::COLOR_ERROR_RED } else { theme::COLOR_SUCCESS_GREEN };
                    ui.label(egui::RichText::new(msg).color(color).strong());
                }
            });
        });

        if toggle_plaintext {
            self.show_api_key_plaintext = !self.show_api_key_plaintext;
        }

        if pref_changed {
            let _ = save_database(&self.db);
        }

        if let Some(mode) = trigger_sync_mode {
            let _ = save_database(&self.db);
            self.trigger_cloud_sync_with_mode(mode);
        }

        if trigger_restore {
            self.trigger_restore_from_remote(ctx);
        }

        if trigger_purge_confirm {
            self.show_purge_confirm_modal = true;
        }
    }

    // --- TAB: Settings ---
    fn render_settings_tab(&mut self, ctx: &egui::Context) {
        let mut pref_changed = false;
        let mut switched_profile: Option<Option<String>> = None;
        let mut open_custom_popup = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_notification_bar(ui);

            ui.add_space(4.0);
            ui.heading(
                egui::RichText::new("⚙ App Settings & User Profiles")
                    .color(theme::COLOR_ACCENT_CYAN)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Manage Steam profiles, player identity, themes, and local database settings.")
                    .color(theme::COLOR_TEXT_SECONDARY),
            );
            ui.add_space(10.0);

            // User Profiles Card
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width().min(680.0));
                ui.label(
                    egui::RichText::new("👤 Multi-User Steam Profiles")
                        .strong()
                        .color(theme::COLOR_ACCENT_CYAN),
                );
                ui.add_space(6.0);

                let current_label = self.current_account_display_name();
                ui.horizontal(|ui| {
                    ui.label("Active Account:");
                    egui::ComboBox::from_id_salt("settings_profile_switcher")
                        .selected_text(
                            egui::RichText::new(format!("👤 {}", current_label))
                                .color(theme::COLOR_ACCENT_GOLD)
                                .strong(),
                        )
                        .show_ui(ui, |ui| {
                            for user in &self.detected_users {
                                let is_sel = self.current_steam_id.as_deref() == Some(&user.steam_id);
                                let item_label = if user.is_most_recent {
                                    format!("★ {} ({})", user.persona_name, user.steam_id)
                                } else {
                                    format!("👤 {} ({})", user.persona_name, user.steam_id)
                                };

                                if ui.selectable_label(is_sel, item_label).clicked() {
                                    switched_profile = Some(Some(user.steam_id.clone()));
                                }
                            }
                            if !self.detected_users.is_empty() {
                                ui.separator();
                            }
                            if ui.selectable_label(false, "➕ Custom Steam ID...").clicked() {
                                open_custom_popup = true;
                            }
                        });
                });

                ui.add_space(6.0);
                let db_path = get_database_path_for_user(self.current_steam_id.as_deref());
                ui.label(format!("Isolated Profile DB: {}", db_path.display()));

                ui.add_space(6.0);
                if ui.button("📂 Open Profile Folder").clicked() {
                    if let Some(parent) = db_path.parent() {
                        let _ = open::that(parent);
                    }
                }
            });

            ui.add_space(10.0);

            // Player Identity Card
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width().min(680.0));
                ui.label(
                    egui::RichText::new("🏷 Player Identity & Passport Subtitle")
                        .strong()
                        .color(theme::COLOR_ACCENT_CYAN),
                );
                ui.add_space(6.0);

                ui.label("Player Tag / Username:");
                if ui.text_edit_singleline(&mut self.username).changed() {
                    self.db.preferences.username = self.username.clone();
                    pref_changed = true;
                }

                ui.add_space(4.0);
                ui.label("Passport Subtitle:");
                if ui.text_edit_singleline(&mut self.title_line).changed() {
                    self.db.preferences.title_line = self.title_line.clone();
                    pref_changed = true;
                }

                ui.horizontal(|ui| {
                    if ui.button("Hunter of Yharnam").clicked() {
                        self.title_line = "Hunter of Yharnam   •   Trophy Hunter".to_string();
                        self.db.preferences.title_line = self.title_line.clone();
                        pref_changed = true;
                    }
                    if ui.button("Clear Subtitle").clicked() {
                        self.title_line.clear();
                        self.db.preferences.title_line = self.title_line.clone();
                        pref_changed = true;
                    }
                });
            });

            ui.add_space(10.0);

            // Appearance & Themes Card
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width().min(680.0));
                ui.label(
                    egui::RichText::new("🎨 Appearance & Defaults")
                        .strong()
                        .color(theme::COLOR_ACCENT_CYAN),
                );
                ui.add_space(6.0);

                let mut chosen_theme = None;
                egui::ComboBox::from_label("Default Theme Preset")
                    .selected_text(self.theme.name())
                    .show_ui(ui, |ui| {
                        for t in PassportTheme::all() {
                            let is_selected = self.theme == t;
                            if ui.selectable_label(is_selected, t.name()).clicked() {
                                chosen_theme = Some(t);
                            }
                        }
                    });

                if let Some(t) = chosen_theme {
                    self.theme = t;
                    self.db.preferences.theme = match t {
                        PassportTheme::TrophyGold => "TrophyGold".into(),
                        PassportTheme::BloodborneCrimson => "BloodborneCrimson".into(),
                        PassportTheme::EmeraldMatrix => "EmeraldMatrix".into(),
                        PassportTheme::SteamPurple => "SteamPurple".into(),
                        _ => "CyberpunkCyan".into(),
                    };
                    pref_changed = true;
                }
            });
        });

        if let Some(target_id) = switched_profile {
            self.switch_user_profile(target_id, ctx);
        }
        if open_custom_popup {
            self.show_custom_id_popup = true;
        }
        if pref_changed {
            let _ = save_database(&self.db);
        }
    }

    /// Renders custom Steam ID input modal popup.
    fn render_custom_steam_id_modal(&mut self, ctx: &egui::Context) {
        if !self.show_custom_id_popup {
            return;
        }

        let mut close_popup = false;
        let mut custom_target: Option<String> = None;

        egui::Window::new("➕ Switch to Custom Steam ID")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label("Enter numeric Steam32 Account ID (e.g. 462943940) or Steam64 Community ID (e.g. 76561198423209668):");
                ui.add_space(6.0);
                ui.text_edit_singleline(&mut self.custom_steam_id_input);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Confirm").clicked() {
                        let trimmed = self.custom_steam_id_input.trim();
                        if let Ok(s64) = trimmed.parse::<u64>() {
                            if let Some(s32) = trophyvault_core::steam::steam64_to_steam32(s64) {
                                custom_target = Some(s32.to_string());
                            } else {
                                custom_target = Some(s64.to_string());
                            }
                        } else if !trimmed.is_empty() {
                            custom_target = Some(trimmed.to_string());
                        }
                        close_popup = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_popup = true;
                    }
                });
            });

        if let Some(target) = custom_target {
            self.switch_user_profile(Some(target), ctx);
            self.custom_steam_id_input.clear();
        }
        if close_popup {
            self.show_custom_id_popup = false;
        }
    }

    /// Renders purge remote DB confirmation modal.
    fn render_purge_confirm_modal(&mut self, ctx: &egui::Context) {
        if !self.show_purge_confirm_modal {
            return;
        }

        let mut close_modal = false;
        let mut do_purge = false;

        egui::Window::new("⚠️ Confirm Remote Database Purge")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Are you sure you want to purge all games from the remote Web database? This action cannot be undone.",
                    )
                    .color(theme::COLOR_ERROR_RED)
                    .strong(),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new("Yes, Purge Remote DB")
                                .color(theme::COLOR_ERROR_RED)
                                .strong(),
                        )
                        .clicked()
                    {
                        do_purge = true;
                        close_modal = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close_modal = true;
                    }
                });
                ui.add_space(4.0);
            });

        if do_purge {
            self.trigger_purge_remote_db();
        }
        if close_modal {
            self.show_purge_confirm_modal = false;
        }
    }
}
