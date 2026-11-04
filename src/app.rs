//! GUI Application built using eframe / egui.

use crate::data::{AchievementsData, GameItem};
use crate::generator::{
    generate_passport_svg, render_svg_to_pixmap, render_svg_to_png, PassportOptions, PassportTheme,
};
use crate::storage::{
    database_to_achievements_data, get_database_path, get_database_path_for_user,
    load_or_create_database, load_or_create_database_for_user, save_database,
    sync_library, LibraryDatabase, ManualGameSetting,
};
use crate::steam::{
    detect_steam_users, find_active_steam_id, find_steam_dir, parse_non_steam_shortcuts,
    NonSteamShortcut, SteamUserAccount,
};
use eframe::egui;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum AppTab {
    Canvas,
    CustomGames,
    DatabaseManager,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PassportMode {
    Overall,
    SingleGame,
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

    // Feedback & Preview
    status_notification: Option<(String, bool)>, // (message, is_error)
    preview_texture: Option<egui::TextureHandle>,
    last_rendered_svg: Option<String>,
}

impl PassportApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Dark cyberpunk visuals
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(egui::Color32::from_rgb(240, 246, 252));
        visuals.panel_fill = egui::Color32::from_rgb(5, 15, 26);
        visuals.window_fill = egui::Color32::from_rgb(7, 24, 41);
        cc.egui_ctx.set_visuals(visuals);

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
            active_tab: AppTab::Canvas,

            detected_users,
            current_steam_id: initial_steam_id,
            custom_steam_id_input: String::new(),
            show_custom_id_popup: false,

            mode: PassportMode::Overall,
            selected_game_name: "Bloodborne (PC Recomp)".to_string(),
            game_search_query: String::new(),
            username,
            title_line,
            theme,
            output_path: default_output,

            new_game_name: "Bloodborne (PC Recomp)".to_string(),
            new_game_dir: r"C:\Games\Decomps\Bloodborne\user".to_string(),
            new_game_appid: "2485153835".to_string(),
            new_game_parser: "bbport".to_string(),
            detected_shortcuts: Vec::new(),

            db_search_query: String::new(),
            cached_db_keys: Vec::new(),

            status_notification: None,
            preview_texture: None,
            last_rendered_svg: None,
        };

        app.rebuild_cached_db_keys();
        app.refresh_data_and_preview(&cc.egui_ctx);
        app
    }

    fn rebuild_cached_db_keys(&mut self) {
        let mut keys: Vec<String> = self.db.games.keys().cloned().collect();
        keys.sort_by(|a, b| {
            let ga = &self.db.games[a];
            let gb = &self.db.games[b];
            gb.pct.cmp(&ga.pct).then_with(|| ga.name.cmp(&gb.name))
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
        self.generate_and_render_preview(ctx);
        if let Some(svg) = &self.last_rendered_svg {
            match render_svg_to_png(svg, 1200, 850) {
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
                    self.status_notification = Some((format!("PNG Render Error: {}", e), true));
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
}

impl eframe::App for PassportApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // TOP PANEL: Header, Navigation Tabs, and Quick Sync
        egui::TopBottomPanel::top("header_panel").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("🏆 TrophyVault")
                        .color(egui::Color32::from_rgb(0, 240, 255))
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .size(12.0)
                        .color(egui::Color32::from_rgb(100, 160, 220))
                        .monospace(),
                );

                ui.add_space(15.0);

                // Profile Switcher Dropdown
                let current_label = self.current_account_display_name();
                let mut switched_profile: Option<Option<String>> = None;
                let mut open_custom_popup = false;

                egui::ComboBox::from_id_salt("header_profile_switcher")
                    .selected_text(
                        egui::RichText::new(format!("👤 {}", current_label))
                            .color(egui::Color32::from_rgb(255, 215, 0))
                            .size(13.0)
                            .strong(),
                    )
                    .show_ui(ui, |ui| {
                        ui.label(egui::RichText::new("Steam Accounts").size(11.0).weak());
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

                if let Some(target_id) = switched_profile {
                    self.switch_user_profile(target_id, ctx);
                }
                if open_custom_popup {
                    self.show_custom_id_popup = true;
                }

                ui.add_space(15.0);

                // Navigation Tabs
                if ui
                    .selectable_label(self.active_tab == AppTab::Canvas, "📇 Passport Canvas")
                    .clicked()
                {
                    self.active_tab = AppTab::Canvas;
                }
                if ui
                    .selectable_label(
                        self.active_tab == AppTab::CustomGames,
                        "⚙ Custom Ports & Decomps",
                    )
                    .clicked()
                {
                    self.active_tab = AppTab::CustomGames;
                }
                if ui
                    .selectable_label(
                        self.active_tab == AppTab::DatabaseManager,
                        "📚 Library Database",
                    )
                    .clicked()
                {
                    self.active_tab = AppTab::DatabaseManager;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🔄 Sync Library").clicked() {
                        self.run_sync(ctx);
                    }
                    if let Some(data) = &self.data {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} Games • {} Unlocks • {} OVR",
                                data.stats.total_configs,
                                data.stats.unlocked_achievements,
                                data.stats.ovr_score
                            ))
                            .color(egui::Color32::from_rgb(0, 255, 136)),
                        );
                    }
                });
            });

            // Notification message bar
            if let Some((msg, is_err)) = &self.status_notification {
                ui.add_space(4.0);
                let color = if *is_err {
                    egui::Color32::from_rgb(255, 100, 100)
                } else {
                    egui::Color32::from_rgb(100, 255, 180)
                };
                ui.label(egui::RichText::new(msg).color(color).size(13.0));
            }

            ui.add_space(6.0);
        });

        if self.show_custom_id_popup {
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
                                if let Some(s32) = crate::steam::steam64_to_steam32(s64) {
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

        // Render active tab view
        match self.active_tab {
            AppTab::Canvas => self.show_canvas_tab(ctx),
            AppTab::CustomGames => self.show_custom_games_tab(ctx),
            AppTab::DatabaseManager => self.show_database_tab(ctx),
        }
    }
}

impl PassportApp {
    fn show_canvas_tab(&mut self, ctx: &egui::Context) {
        let mut selection_changed = false;

        // LEFT SIDEBAR: Controls & Customization
        egui::SidePanel::left("control_panel")
            .resizable(true)
            .default_width(380.0)
            .min_width(320.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(8.0);

                    // --- PASSPORT MODE ---
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new("🎯 Passport Mode")
                                .strong()
                                .color(egui::Color32::from_rgb(0, 240, 255)),
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
                            ui.add_space(8.0);
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
                                egui::ComboBox::from_label("Selected Game")
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
                            }
                        }
                    });

                    ui.add_space(10.0);

                    // --- VISUAL THEME ---
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new("🎨 Visual Theme")
                                .strong()
                                .color(egui::Color32::from_rgb(0, 240, 255)),
                        );
                        ui.add_space(4.0);

                        let mut chosen_theme = None;
                        egui::ComboBox::from_label("Theme Preset")
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

                    ui.add_space(10.0);

                    // --- PLAYER PROFILE DETAILS ---
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new("👤 Player Tag & Subtitle")
                                .strong()
                                .color(egui::Color32::from_rgb(0, 240, 255)),
                        );
                        ui.add_space(4.0);

                        ui.label("Player Tag / Username:");
                        if ui.text_edit_singleline(&mut self.username).changed() {
                            self.db.preferences.username = self.username.clone();
                            let _ = save_database(&self.db);
                            selection_changed = true;
                        }

                        ui.add_space(4.0);
                        ui.label("Passport Subtitle:");
                        if ui.text_edit_singleline(&mut self.title_line).changed() {
                            self.db.preferences.title_line = self.title_line.clone();
                            let _ = save_database(&self.db);
                            selection_changed = true;
                        }

                        ui.horizontal(|ui| {
                            if ui.button("Hunter of Yharnam").clicked() {
                                self.title_line = "Hunter of Yharnam   •   Trophy Hunter".to_string();
                                self.db.preferences.title_line = self.title_line.clone();
                                let _ = save_database(&self.db);
                                selection_changed = true;
                            }
                            if ui.button("Clear Subtitle").clicked() {
                                self.title_line.clear();
                                self.db.preferences.title_line = self.title_line.clone();
                                let _ = save_database(&self.db);
                                selection_changed = true;
                            }
                        });
                    });

                    ui.add_space(10.0);

                    // --- EXPORT & SAVE ---
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new("💾 Export Passport")
                                .strong()
                                .color(egui::Color32::from_rgb(0, 240, 255)),
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

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized(
                                    [150.0, 36.0],
                                    egui::Button::new("📸 Export PNG (1200×850)"),
                                )
                                .clicked()
                            {
                                self.save_png(ctx);
                            }
                            if ui
                                .add_sized([140.0, 36.0], egui::Button::new("🎨 Export Vector SVG"))
                                .clicked()
                            {
                                self.save_svg();
                            }
                        });
                    });

                    ui.add_space(8.0);

                    // GitHub Link (Bottom-Left)
                    ui.horizontal(|ui| {
                        ui.hyperlink_to(
                            egui::RichText::new("GitHub Repository ↗")
                                .color(egui::Color32::from_rgb(88, 166, 255)),
                            "https://github.com/MohandL3G/TrophyVault",
                        );
                    });

                    ui.add_space(10.0);
                });
            });

        if selection_changed {
            self.generate_and_render_preview(ctx);
        }

        // CENTRAL AREA: Passport Live Preview
        egui::CentralPanel::default().show(ctx, |ui| {
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

                ui.vertical_centered(|ui| {
                    ui.image((texture.id(), egui::vec2(target_width, target_height)));
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Rendering passport preview...");
                });
            }
        });
    }

    fn show_custom_games_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);

                ui.heading(
                    egui::RichText::new("⚙ Custom Ports, Decomps & Emulators")
                        .color(egui::Color32::from_rgb(0, 240, 255))
                        .strong(),
                );
                ui.label(
                    "Add manual games such as Bloodborne PC Port (bbport/ShadPS4). Achievements and trophies are read from your save folder, and playtime is linked from Steam.",
                );

                ui.add_space(12.0);

                // FORM: Add / Update Manual Game
                ui.group(|ui| {
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

                    // If detected shortcuts exist, show clickable choices
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

                        // Check if already exists in manual_games
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

                ui.add_space(16.0);

                // LIST: Configured Manual Games
                ui.heading("Configured Custom Games");
                ui.add_space(4.0);

                let manual_games_clone = self.db.manual_games.clone();
                let mut rescan_needed = false;
                let mut to_remove = None;

                for (idx, cfg) in manual_games_clone.iter().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.heading(
                                egui::RichText::new(&cfg.name)
                                    .color(egui::Color32::from_rgb(0, 240, 255)),
                            );
                            ui.label(format!("(ID: {}, Parser: {})", cfg.id, cfg.parser_type));
                        });
                        ui.label(format!("📁 Folder: {}", cfg.user_dir));
                        if let Some(appid) = &cfg.non_steam_appid {
                            ui.label(format!("🆔 Non-Steam AppID: {}", appid));
                        }

                        // Display current stats if present in db.games
                        if let Some(game) = self.db.games.get(&cfg.id) {
                            ui.label(
                                egui::RichText::new(format!(
                                    "🏆 Trophies: {} / {} ({}%) • Playtime: {} hrs",
                                    game.unlocked,
                                    game.total,
                                    game.pct,
                                    game.playtime_minutes / 60
                                ))
                                .color(egui::Color32::from_rgb(0, 255, 136)),
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

    fn show_database_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);

            ui.heading(
                egui::RichText::new("📚 Persistent Library Database")
                    .color(egui::Color32::from_rgb(0, 240, 255))
                    .strong(),
            );

            let db_path = get_database_path_for_user(self.current_steam_id.as_deref());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Active Profile: {}",
                        self.current_account_display_name()
                    ))
                    .color(egui::Color32::from_rgb(255, 215, 0))
                    .strong(),
                );
                ui.label("•");
                ui.label(format!("Path: {}", db_path.display()));
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("📂 Open Profile Folder").clicked() {
                    if let Some(parent) = db_path.parent() {
                        let _ = open::that(parent);
                    }
                }
                if ui.button("🔄 Sync Library Now").clicked() {
                    self.run_sync(ctx);
                }
            });

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "🛡 Retention Guarantee: Games in this database will NEVER be deleted automatically, even if you delete the game folder or remove the LuaTools script. Only manual deletion will remove an entry.",
                )
                .color(egui::Color32::from_rgb(140, 200, 255))
                .italics(),
            );

            ui.add_space(8.0);

            // Search Filter
            ui.horizontal(|ui| {
                ui.label("Filter Games:");
                ui.text_edit_singleline(&mut self.db_search_query);
            });

            ui.add_space(8.0);

            // Table of Games (Optimized with cached keys and virtual row windowing, scaling dynamically to full width and height)
            let query = self.db_search_query.to_lowercase();
            let filtered_keys: Vec<&String> = self
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
                .collect();

            let mut to_delete_key: Option<String> = None;
            let row_height = 24.0;
            let total_rows = filtered_keys.len();

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

            // Header Row (Matches Body Column Sizes Exactly)
            ui.horizontal(|ui| {
                ui.add_sized(
                    [col_title, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Game Title")
                            .strong()
                            .color(egui::Color32::from_rgb(0, 240, 255)),
                    ),
                );
                ui.add_sized(
                    [col_platform, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Platform")
                            .strong()
                            .color(egui::Color32::from_rgb(0, 240, 255)),
                    ),
                );
                ui.add_sized(
                    [col_appid, 22.0],
                    egui::Label::new(
                        egui::RichText::new("App ID")
                            .strong()
                            .color(egui::Color32::from_rgb(0, 240, 255)),
                    ),
                );
                ui.add_sized(
                    [col_ach, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Achievements")
                            .strong()
                            .color(egui::Color32::from_rgb(0, 240, 255)),
                    ),
                );
                ui.add_sized(
                    [col_pct, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Progress")
                            .strong()
                            .color(egui::Color32::from_rgb(0, 240, 255)),
                    ),
                );
                ui.add_sized(
                    [col_playtime, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Playtime")
                            .strong()
                            .color(egui::Color32::from_rgb(0, 240, 255)),
                    ),
                );
                ui.add_sized(
                    [col_source, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Source")
                            .strong()
                            .color(egui::Color32::from_rgb(0, 240, 255)),
                    ),
                );
                ui.add_sized(
                    [col_action, 22.0],
                    egui::Label::new(
                        egui::RichText::new("Actions")
                            .strong()
                            .color(egui::Color32::from_rgb(0, 240, 255)),
                    ),
                );
            });
            ui.separator();

            // Virtualized Scroll Area Expanding to Full Remaining Height
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, total_rows, |ui, row_range| {
                    for idx in row_range {
                        let key = filtered_keys[idx];
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
}
