//! GUI Application built using eframe / egui with Layout 1 (Modern Left Sidebar).

#[path = "theme.rs"]
pub mod theme;

use trophyvault_core::data::AchievementsData;
use trophyvault_core::generator::PassportTheme;
use trophyvault_core::storage::{
    database_to_achievements_data, get_database_path_for_user, load_or_create_database_for_user,
    purge_remote_database, restore_from_remote, save_database, sync_library,
    sync_to_remote_with_mode, LibraryDatabase, ManualGameSetting, SyncMode, SyncSettings,
};
use trophyvault_core::steam::{
    detect_steam_users, find_active_steam_id, find_steam_dir, parse_non_steam_shortcuts,
    NonSteamShortcut, SteamUserAccount,
};
use eframe::egui;

#[allow(dead_code)]
pub type TrophyVaultApp = PassportApp;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum AppTab {
    #[default]
    LibraryStats,
    BloodbornePort,
    CloudSync,
    Settings,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum PlatformFilter {
    #[default]
    All,
    Steam,
    Ports,
    Perfect100Only,
}

#[allow(dead_code)]
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

    // Player Preferences & Defaults
    username: String,
    title_line: String,
    theme: PassportTheme,

    // Add Custom Game Form
    new_game_name: String,
    new_game_dir: String,
    new_game_appid: String,
    new_game_parser: String,
    detected_shortcuts: Vec<NonSteamShortcut>,

    // Database / Library & Stats Toolbar State
    db_search_query: String,
    cached_db_keys: Vec<String>,
    db_sort_col: DbSortColumn,
    db_sort_dir: SortDirection,
    platform_filter: PlatformFilter,

    // Feedback & Notifications
    status_notification: Option<(String, bool)>, // (message, is_error)

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

        let mut app = Self {
            db,
            data: None,
            active_tab: AppTab::LibraryStats, // Default showcase tab!

            detected_users,
            current_steam_id: initial_steam_id,
            custom_steam_id_input: String::new(),
            show_custom_id_popup: false,

            username,
            title_line,
            theme,

            new_game_name: "Bloodborne".to_string(),
            new_game_dir: r"C:\Games\Decomps\Bloodborne\user".to_string(),
            new_game_appid: "2485153835".to_string(),
            new_game_parser: "bbport".to_string(),
            detected_shortcuts: Vec::new(),

            db_search_query: String::new(),
            cached_db_keys: Vec::new(),
            db_sort_col: DbSortColumn::Playtime,
            db_sort_dir: SortDirection::Descending,
            platform_filter: PlatformFilter::All,

            status_notification: None,

            show_api_key_plaintext: false,
            cloud_sync_status_msg: None,
            has_synced_on_exit: false,
            show_purge_confirm_modal: false,
        };

        if app.db.preferences.sync_settings.is_none() {
            app.db.preferences.sync_settings = Some(SyncSettings::default());
        }

        app.rebuild_cached_db_keys();
        app.refresh_data();
        app
    }

    fn refresh_data(&mut self) {
        self.data = Some(database_to_achievements_data(&self.db));
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

    fn run_sync(&mut self, _ctx: &egui::Context) {
        let report = sync_library(&mut self.db);
        self.status_notification = Some((
            format!(
                "✓ Library Synced: {} games in database • {} manual • {} updated",
                report.total_games, report.manual_scanned, report.steam_updated
            ),
            false,
        ));
        self.rebuild_cached_db_keys();
        self.refresh_data();
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

    pub fn switch_user_profile(&mut self, new_steam_id: Option<String>, _ctx: &egui::Context) {
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

        // 4. Sync library for new profile
        let report = sync_library(&mut self.db);

        // 5. Rebuild cached keys & refresh data
        self.rebuild_cached_db_keys();
        self.refresh_data();

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
    pub fn trigger_restore_from_remote(&mut self, _ctx: &egui::Context) {
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
                self.refresh_data();
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

        // Right Area: Main View for the Selected Tab
        match self.active_tab {
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

                    // Vertical Navigation Buttons (4 Tabs)
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

    /// Helper to render stacked navigation buttons with sleek translucent cyan glass pill and crisp white/cyan text.
    fn render_nav_item(&self, ui: &mut egui::Ui, icon: &str, title: &str, is_active: bool) -> bool {
        let desired_size = egui::vec2(ui.available_width(), 42.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
        let is_hovered = response.hovered();

        let bg_color = if is_active {
            theme::COLOR_CYAN_PILL_BG
        } else if is_hovered {
            theme::COLOR_HOVER_BG
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

        // Active indicator left bar
        if is_active {
            ui.painter().line_segment(
                [
                    egui::pos2(rect.min.x + 3.0, rect.min.y + 10.0),
                    egui::pos2(rect.min.x + 3.0, rect.max.y - 10.0),
                ],
                egui::Stroke::new(3.0_f32, theme::COLOR_ACCENT_CYAN),
            );
        }

        // Active: crisp white text with glowing cyan icon. Inactive: muted slate text #94a3b8.
        let text_color = if is_active {
            theme::COLOR_WHITE
        } else if is_hovered {
            theme::COLOR_TEXT_PRIMARY
        } else {
            theme::COLOR_TEXT_SECONDARY
        };

        let icon_color = if is_active {
            theme::COLOR_ACCENT_CYAN
        } else if is_hovered {
            theme::COLOR_TEXT_PRIMARY
        } else {
            theme::COLOR_TEXT_MUTED
        };

        // Render directly with painter for perfect contrast and no child widget click-stealing
        ui.painter().text(
            egui::pos2(rect.min.x + 16.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(16.0),
            icon_color,
        );
        ui.painter().text(
            egui::pos2(rect.min.x + 44.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(13.5),
            text_color,
        );

        ui.add_space(4.0);
        response.clicked()
    }

    /// Renders the gamer profile card at the bottom of the sidebar with avatar circle/initial chip.
    fn render_gamer_profile_card(&mut self, ui: &mut egui::Ui) {
        let ovr_score = self.data.as_ref().map(|d| d.stats.ovr_score).unwrap_or(0);
        let total_games = self.data.as_ref().map(|d| d.stats.total_configs).unwrap_or(self.db.games.len() as u32);

        let (dot_color, sync_tooltip) = match &self.db.preferences.sync_settings {
            Some(s) if s.last_sync_status.as_deref().map(|st| st.starts_with("Success")).unwrap_or(false) => {
                (theme::COLOR_SUCCESS_GREEN, "Cloud Sync: Online & Synced")
            }
            Some(s) if s.last_sync_status.is_some() => {
                (theme::COLOR_ERROR_RED, "Cloud Sync: Last attempt failed")
            }
            _ => (theme::COLOR_TEXT_MUTED, "Cloud Sync: Idle / Unconfigured"),
        };

        let initial = self
            .username
            .chars()
            .next()
            .unwrap_or('M')
            .to_uppercase()
            .to_string();

        let card_frame = theme::card_frame();
        let desired_width = ui.available_width();

        let response = card_frame.show(ui, |ui| {
            ui.set_width(desired_width);
            ui.horizontal(|ui| {
                // Avatar circle chip with glowing cyan border
                let chip_size = egui::vec2(34.0, 34.0);
                let (chip_rect, _) = ui.allocate_exact_size(chip_size, egui::Sense::hover());
                ui.painter().rect(
                    chip_rect,
                    egui::Rounding::same(17.0_f32),
                    theme::COLOR_BG_OBSIDIAN,
                    egui::Stroke::new(1.5_f32, theme::COLOR_ACCENT_CYAN),
                );
                let initial_pos = chip_rect.center() - egui::vec2(0.0, 1.0);
                ui.painter().text(
                    initial_pos,
                    egui::Align2::CENTER_CENTER,
                    &initial,
                    egui::FontId::proportional(14.0),
                    theme::COLOR_ACCENT_CYAN,
                );

                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&self.username)
                                .size(13.0)
                                .strong()
                                .color(theme::COLOR_WHITE),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new("●")
                                .size(10.0)
                                .color(dot_color),
                        )
                        .on_hover_text(sync_tooltip);
                    });

                    ui.horizontal(|ui| {
                        // OVR score pill
                        let pill_frame = egui::Frame::none()
                            .fill(theme::COLOR_GOLD_PILL_BG)
                            .stroke(egui::Stroke::new(1.0_f32, theme::COLOR_ACCENT_GOLD))
                            .rounding(egui::Rounding::same(4.0_f32))
                            .inner_margin(egui::Margin::symmetric(4.0_f32, 1.0_f32));

                        pill_frame.show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("{} OVR", ovr_score))
                                    .size(9.5)
                                    .strong()
                                    .color(theme::COLOR_ACCENT_GOLD),
                            );
                        });

                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(format!("{} Games", total_games))
                                .size(10.5)
                                .color(theme::COLOR_TEXT_SECONDARY),
                        );
                    });
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

    // --- MAIN SHOWCASE TAB: Library & Statistics ---
    fn show_database_tab(&mut self, ctx: &egui::Context) {
        let mut sort_changed = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_notification_bar(ui);

            // Title & Action Bar
            ui.add_space(2.0);
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
                    if ui.button("📂 Open DB Folder").clicked() {
                        if let Some(parent) = db_path.parent() {
                            let _ = open::that(parent);
                        }
                    }
                });
            });

            ui.add_space(8.0);

            // Top Hero Telemetry row: 5 glowing stat cards
            let (total_games, perfect_games, total_playtime_minutes, completion_rate, ovr_score) =
                if let Some(data) = &self.data {
                    (
                        data.stats.total_configs,
                        data.stats.perfect_games,
                        data.stats.total_playtime_minutes,
                        data.stats.completion_rate,
                        data.stats.ovr_score,
                    )
                } else {
                    let tg = self.db.games.len() as u32;
                    let pg = self
                        .db
                        .games
                        .values()
                        .filter(|g| g.total > 0 && g.unlocked >= g.total)
                        .count() as u32;
                    let pm: u64 = self.db.games.values().map(|g| g.playtime_minutes as u64).sum();
                    let total_unlocked: u32 = self.db.games.values().map(|g| g.unlocked).sum();
                    let total_ach: u32 = self.db.games.values().map(|g| g.total).sum();
                    let cr = if total_ach > 0 {
                        (total_unlocked as f64 / total_ach as f64) * 100.0
                    } else {
                        0.0
                    };
                    (tg, pg, pm, cr, 0)
                };

            ui.horizontal(|ui| {
                let card_w = ((ui.available_width() - 32.0) / 5.0 - 20.0).max(110.0);

                // 1. Total Games
                theme::hero_stat_frame(None).show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.label(egui::RichText::new("TOTAL GAMES").size(10.0).color(theme::COLOR_TEXT_MUTED));
                    ui.heading(
                        egui::RichText::new(format!("{}", total_games))
                            .color(theme::COLOR_ACCENT_CYAN)
                            .strong(),
                    );
                });

                // 2. 100% Perfect Games (Gold accent)
                theme::hero_stat_frame(Some(theme::COLOR_ACCENT_GOLD)).show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.label(egui::RichText::new("★ 100% PERFECT").size(10.0).color(theme::COLOR_ACCENT_GOLD));
                    ui.heading(
                        egui::RichText::new(format!("{}", perfect_games))
                            .color(theme::COLOR_ACCENT_GOLD)
                            .strong(),
                    );
                });

                // 3. Total Playtime
                theme::hero_stat_frame(None).show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.label(egui::RichText::new("TOTAL PLAYTIME").size(10.0).color(theme::COLOR_TEXT_MUTED));
                    let hours = total_playtime_minutes / 60;
                    let mins = total_playtime_minutes % 60;
                    ui.heading(
                        egui::RichText::new(format!("{}h {}m", hours, mins))
                            .color(theme::COLOR_WHITE)
                            .strong(),
                    );
                });

                // 4. Completion Rate
                theme::hero_stat_frame(None).show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.label(egui::RichText::new("COMPLETION RATE").size(10.0).color(theme::COLOR_TEXT_MUTED));
                    ui.heading(
                        egui::RichText::new(format!("{:.1}%", completion_rate))
                            .color(theme::COLOR_SUCCESS_GREEN)
                            .strong(),
                    );
                });

                // 5. Overall Score (OVR score pill)
                theme::hero_stat_frame(Some(theme::COLOR_ACCENT_CYAN)).show(ui, |ui| {
                    ui.set_width(card_w);
                    ui.label(egui::RichText::new("OVERALL SCORE").size(10.0).color(theme::COLOR_TEXT_MUTED));
                    ui.horizontal(|ui| {
                        let pill_frame = egui::Frame::none()
                            .fill(theme::COLOR_CYAN_PILL_BG)
                            .stroke(egui::Stroke::new(1.0_f32, theme::COLOR_ACCENT_CYAN))
                            .rounding(egui::Rounding::same(6.0_f32))
                            .inner_margin(egui::Margin::symmetric(8.0_f32, 2.0_f32));
                        pill_frame.show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("{} OVR", ovr_score))
                                    .size(16.0)
                                    .color(theme::COLOR_ACCENT_CYAN)
                                    .strong(),
                            );
                        });
                    });
                });
            });

            ui.add_space(10.0);

            // Filter & Search Toolbar
            ui.horizontal(|ui| {
                // Search Input
                ui.add(
                    egui::TextEdit::singleline(&mut self.db_search_query)
                        .hint_text("Search by title or AppID...")
                        .desired_width(240.0),
                );

                ui.add_space(8.0);

                // Platform Filter Pills
                let filter_pills = [
                    (PlatformFilter::All, "All"),
                    (PlatformFilter::Steam, "Steam"),
                    (PlatformFilter::Ports, "Ports"),
                    (PlatformFilter::Perfect100Only, "★ 100% Only"),
                ];

                for (p_filter, p_label) in filter_pills {
                    let is_active = self.platform_filter == p_filter;
                    let (bg, border, text_color) = if is_active {
                        if p_filter == PlatformFilter::Perfect100Only {
                            (theme::COLOR_GOLD_PILL_BG, theme::COLOR_ACCENT_GOLD, theme::COLOR_ACCENT_GOLD)
                        } else {
                            (theme::COLOR_CYAN_PILL_BG, theme::COLOR_ACCENT_CYAN, theme::COLOR_WHITE)
                        }
                    } else {
                        (theme::COLOR_CARD_BG, theme::COLOR_BORDER_SLATE, theme::COLOR_TEXT_SECONDARY)
                    };

                    let pill_btn = egui::Button::new(
                        egui::RichText::new(p_label).size(12.0).color(text_color).strong(),
                    )
                    .fill(bg)
                    .stroke(egui::Stroke::new(1.0_f32, border))
                    .rounding(egui::Rounding::same(6.0_f32));

                    if ui.add(pill_btn).clicked() {
                        self.platform_filter = p_filter;
                    }
                }

                ui.add_space(8.0);

                // Sort Dropdown
                let sort_label = match (self.db_sort_col, self.db_sort_dir) {
                    (DbSortColumn::Playtime, _) => "⏱ Sort: Playtime",
                    (DbSortColumn::Progress, _) => "🏆 Sort: Progress",
                    (DbSortColumn::Title, _) => "🔤 Sort: Alphabetical",
                    _ => "Sort Games",
                };

                egui::ComboBox::from_id_salt("library_sort_selector")
                    .selected_text(sort_label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.db_sort_col == DbSortColumn::Playtime, "⏱ Playtime")
                            .clicked()
                        {
                            self.db_sort_col = DbSortColumn::Playtime;
                            self.db_sort_dir = SortDirection::Descending;
                            sort_changed = true;
                        }
                        if ui
                            .selectable_label(self.db_sort_col == DbSortColumn::Progress, "🏆 Progress")
                            .clicked()
                        {
                            self.db_sort_col = DbSortColumn::Progress;
                            self.db_sort_dir = SortDirection::Descending;
                            sort_changed = true;
                        }
                        if ui
                            .selectable_label(self.db_sort_col == DbSortColumn::Title, "🔤 Alphabetical")
                            .clicked()
                        {
                            self.db_sort_col = DbSortColumn::Title;
                            self.db_sort_dir = SortDirection::Ascending;
                            sort_changed = true;
                        }
                    });

                // Right aligned "☁ Sync to Web Now" button
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let current_sync_mode = self
                        .db
                        .preferences
                        .sync_settings
                        .as_ref()
                        .map(|s| s.sync_mode)
                        .unwrap_or_default();

                    let sync_btn = egui::Button::new(
                        egui::RichText::new("☁ Sync to Web Now")
                            .color(theme::COLOR_ACCENT_CYAN)
                            .strong(),
                    )
                    .fill(theme::COLOR_CYAN_PILL_BG)
                    .stroke(egui::Stroke::new(1.0_f32, theme::COLOR_ACCENT_CYAN))
                    .rounding(egui::Rounding::same(6.0_f32));

                    if ui.add(sync_btn).clicked() {
                        self.trigger_cloud_sync_with_mode(current_sync_mode);
                    }
                });
            });

            if sort_changed {
                self.rebuild_cached_db_keys();
            }

            ui.add_space(8.0);

            // Table Columns layout
            let avail_w = ui.available_width().max(980.0);
            let col_platform = 95.0;
            let col_appid = 85.0;
            let col_progress_bar = 125.0;
            let col_trophy = 85.0;
            let col_playtime = 85.0;
            let col_status = 110.0;
            let col_action = 55.0;
            let fixed_sum = col_platform + col_appid + col_progress_bar + col_trophy + col_playtime + col_status + col_action;
            let col_title = (avail_w - fixed_sum - 40.0).max(220.0);

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
                if ui
                    .add_sized(
                        [col_title, 22.0],
                        egui::Button::new(
                            egui::RichText::new(format!("Game Title{}", sort_arrow(DbSortColumn::Title)))
                                .strong()
                                .color(theme::COLOR_ACCENT_CYAN),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    switch_sort = Some(DbSortColumn::Title);
                }

                if ui
                    .add_sized(
                        [col_platform, 22.0],
                        egui::Button::new(
                            egui::RichText::new(format!("Platform{}", sort_arrow(DbSortColumn::Platform)))
                                .strong()
                                .color(theme::COLOR_ACCENT_CYAN),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    switch_sort = Some(DbSortColumn::Platform);
                }

                if ui
                    .add_sized(
                        [col_appid, 22.0],
                        egui::Button::new(
                            egui::RichText::new(format!("App ID{}", sort_arrow(DbSortColumn::AppId)))
                                .strong()
                                .color(theme::COLOR_ACCENT_CYAN),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    switch_sort = Some(DbSortColumn::AppId);
                }

                if ui
                    .add_sized(
                        [col_progress_bar, 22.0],
                        egui::Button::new(
                            egui::RichText::new(format!("Progress{}", sort_arrow(DbSortColumn::Progress)))
                                .strong()
                                .color(theme::COLOR_ACCENT_CYAN),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    switch_sort = Some(DbSortColumn::Progress);
                }

                if ui
                    .add_sized(
                        [col_trophy, 22.0],
                        egui::Button::new(
                            egui::RichText::new(format!("Trophies{}", sort_arrow(DbSortColumn::Achievements)))
                                .strong()
                                .color(theme::COLOR_ACCENT_CYAN),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    switch_sort = Some(DbSortColumn::Achievements);
                }

                if ui
                    .add_sized(
                        [col_playtime, 22.0],
                        egui::Button::new(
                            egui::RichText::new(format!("Playtime{}", sort_arrow(DbSortColumn::Playtime)))
                                .strong()
                                .color(theme::COLOR_ACCENT_CYAN),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    switch_sort = Some(DbSortColumn::Playtime);
                }

                ui.add_sized(
                    [col_status, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Status")
                            .strong()
                            .color(theme::COLOR_ACCENT_CYAN),
                    ),
                );

                ui.add_sized(
                    [col_action, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Act")
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
                        DbSortColumn::Title | DbSortColumn::Platform | DbSortColumn::Source => {
                            SortDirection::Ascending
                        }
                        _ => SortDirection::Descending,
                    };
                }
                self.rebuild_cached_db_keys();
            }

            ui.separator();

            let query = self.db_search_query.trim().to_lowercase();
            let filter = self.platform_filter;

            let filtered_keys: Vec<String> = self
                .cached_db_keys
                .iter()
                .filter(|key| {
                    let game = match self.db.games.get(*key) {
                        Some(g) => g,
                        None => return false,
                    };

                    // Platform filter
                    let passes_platform = match filter {
                        PlatformFilter::All => true,
                        PlatformFilter::Steam => {
                            let p = game.platform.to_lowercase();
                            p.contains("steam") && !p.contains("port") && !p.contains("recomp")
                        }
                        PlatformFilter::Ports => {
                            let p = game.platform.to_lowercase();
                            p.contains("port") || p.contains("recomp") || game.source == "manual"
                        }
                        PlatformFilter::Perfect100Only => {
                            game.total > 0 && game.unlocked >= game.total
                        }
                    };

                    if !passes_platform {
                        return false;
                    }

                    // Query filter
                    if query.is_empty() {
                        true
                    } else {
                        game.name.to_lowercase().contains(&query)
                            || game.appid.as_deref().map(|id| id.contains(&query)).unwrap_or(false)
                            || game.platform.to_lowercase().contains(&query)
                    }
                })
                .cloned()
                .collect();

            let mut to_delete_key: Option<String> = None;
            let row_height = 28.0;
            let total_rows = filtered_keys.len();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    for idx in row_range {
                        let key = &filtered_keys[idx];
                        if let Some(game) = self.db.games.get(key) {
                            let is_perfect = game.total > 0 && game.unlocked >= game.total;

                            ui.horizontal(|ui| {
                                // 1. Title (Gold text and star if 100% perfect game)
                                let title_text = if is_perfect {
                                    egui::RichText::new(format!("★ {}", &game.name))
                                        .color(theme::COLOR_ACCENT_GOLD)
                                        .strong()
                                } else {
                                    egui::RichText::new(&game.name)
                                        .color(theme::COLOR_TEXT_PRIMARY)
                                        .strong()
                                };
                                ui.add_sized(
                                    [col_title, row_height],
                                    egui::Label::new(title_text).truncate(),
                                );

                                // 2. Platform badge
                                let (p_bg, p_border, p_color, p_label) = if game.platform.to_lowercase().contains("steam") {
                                    (theme::COLOR_CYAN_PILL_BG, theme::COLOR_ACCENT_CYAN, theme::COLOR_ACCENT_CYAN, "Steam")
                                } else if game.platform.to_lowercase().contains("port")
                                    || game.platform.to_lowercase().contains("bbport")
                                    || game.source == "manual"
                                {
                                    (
                                        egui::Color32::from_rgba_premultiplied(168, 85, 247, 30),
                                        egui::Color32::from_rgb(168, 85, 247),
                                        egui::Color32::from_rgb(192, 132, 252),
                                        "PC Port",
                                    )
                                } else {
                                    (
                                        theme::COLOR_CARD_BG,
                                        theme::COLOR_BORDER_SLATE,
                                        theme::COLOR_TEXT_SECONDARY,
                                        game.platform.as_str(),
                                    )
                                };

                                ui.allocate_ui_with_layout(
                                    egui::vec2(col_platform, row_height),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        let frame = egui::Frame::none()
                                            .fill(p_bg)
                                            .stroke(egui::Stroke::new(1.0_f32, p_border))
                                            .rounding(egui::Rounding::same(4.0_f32))
                                            .inner_margin(egui::Margin::symmetric(6.0_f32, 2.0_f32));
                                        frame.show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(p_label)
                                                    .size(10.5)
                                                    .color(p_color)
                                                    .strong(),
                                            );
                                        });
                                    },
                                );

                                // 3. AppID
                                ui.add_sized(
                                    [col_appid, row_height],
                                    egui::Label::new(
                                        egui::RichText::new(game.appid.as_deref().unwrap_or("—"))
                                            .color(theme::COLOR_TEXT_MUTED)
                                            .monospace(),
                                    )
                                    .truncate(),
                                );

                                // 4. Progress bar (16px high sleek progress bar)
                                ui.allocate_ui_with_layout(
                                    egui::vec2(col_progress_bar, row_height),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        let bar_size = egui::vec2(col_progress_bar - 10.0, 16.0);
                                        let (bar_rect, _) = ui.allocate_exact_size(bar_size, egui::Sense::hover());
                                        let pct_clamped = (game.pct as f32 / 100.0).clamp(0.0, 1.0);
                                        let fill_w = bar_rect.width() * pct_clamped;
                                        let fill_rect = egui::Rect::from_min_size(
                                            bar_rect.min,
                                            egui::vec2(fill_w, bar_rect.height()),
                                        );
                                        let bar_color = if is_perfect {
                                            theme::COLOR_ACCENT_GOLD
                                        } else {
                                            theme::COLOR_ACCENT_CYAN
                                        };

                                        ui.painter().rect(
                                            bar_rect,
                                            egui::Rounding::same(4.0_f32),
                                            theme::COLOR_BG_OBSIDIAN,
                                            egui::Stroke::new(1.0_f32, theme::COLOR_BORDER_SLATE),
                                        );
                                        if fill_w > 0.0 {
                                            ui.painter().rect(
                                                fill_rect,
                                                egui::Rounding::same(4.0_f32),
                                                bar_color,
                                                egui::Stroke::NONE,
                                            );
                                        }
                                        let text_pos = bar_rect.center();
                                        ui.painter().text(
                                            text_pos,
                                            egui::Align2::CENTER_CENTER,
                                            format!("{}%", game.pct),
                                            egui::FontId::monospace(10.0),
                                            theme::COLOR_WHITE,
                                        );
                                    },
                                );

                                // 5. Trophy count
                                let trophy_text = if is_perfect {
                                    egui::RichText::new(format!("{}/{} 🏆", game.unlocked, game.total))
                                        .color(theme::COLOR_ACCENT_GOLD)
                                        .strong()
                                } else {
                                    egui::RichText::new(format!("{}/{}", game.unlocked, game.total))
                                        .color(theme::COLOR_TEXT_PRIMARY)
                                };
                                ui.add_sized([col_trophy, row_height], egui::Label::new(trophy_text));

                                // 6. Playtime (STRICT CONSTRAINT: Authenticity enforced, read-only display)
                                let playtime_text = format!(
                                    "{}h {}m",
                                    game.playtime_minutes / 60,
                                    game.playtime_minutes % 60
                                );
                                ui.add_sized(
                                    [col_playtime, row_height],
                                    egui::Label::new(
                                        egui::RichText::new(playtime_text)
                                            .color(theme::COLOR_TEXT_PRIMARY)
                                            .monospace(),
                                    ),
                                );

                                // 7. Status badge
                                ui.allocate_ui_with_layout(
                                    egui::vec2(col_status, row_height),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        let (s_bg, s_border, s_color, s_label) = if is_perfect {
                                            (
                                                theme::COLOR_GOLD_PILL_BG,
                                                theme::COLOR_ACCENT_GOLD,
                                                theme::COLOR_ACCENT_GOLD,
                                                "★ PERFECT",
                                            )
                                        } else if game.unlocked > 0 {
                                            (
                                                theme::COLOR_CYAN_PILL_BG,
                                                theme::COLOR_ACCENT_CYAN,
                                                theme::COLOR_ACCENT_CYAN,
                                                "IN PROGRESS",
                                            )
                                        } else {
                                            (
                                                theme::COLOR_BG_OBSIDIAN,
                                                theme::COLOR_BORDER_SLATE,
                                                theme::COLOR_TEXT_MUTED,
                                                "UNPLAYED",
                                            )
                                        };

                                        let frame = egui::Frame::none()
                                            .fill(s_bg)
                                            .stroke(egui::Stroke::new(1.0_f32, s_border))
                                            .rounding(egui::Rounding::same(4.0_f32))
                                            .inner_margin(egui::Margin::symmetric(6.0_f32, 2.0_f32));
                                        frame.show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(s_label)
                                                    .size(9.5)
                                                    .color(s_color)
                                                    .strong(),
                                            );
                                        });
                                    },
                                );

                                // 8. Action (Delete)
                                if ui
                                    .add_sized([col_action, 22.0], egui::Button::new("🗑"))
                                    .on_hover_text("Delete game entry from local database")
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
                self.refresh_data();
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
                    // STRICT CONSTRAINT: Playtime cannot be edited manually here!
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
                        ui.label(
                            egui::RichText::new(format!("{}", ts))
                                .monospace()
                                .color(theme::COLOR_TEXT_SECONDARY),
                        );
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
                    let color = if *is_err {
                        theme::COLOR_ERROR_RED
                    } else {
                        theme::COLOR_SUCCESS_GREEN
                    };
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
                    egui::RichText::new("🏷 Player Identity & Subtitle")
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
                ui.label("Profile Subtitle:");
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
