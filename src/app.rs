//! GUI Application built using eframe / egui.

use crate::data::{AchievementsData, GameItem};
use crate::generator::{
    generate_passport_svg, render_svg_to_png, PassportOptions, PassportTheme,
};
use crate::storage::{
    database_to_achievements_data, get_database_path, load_or_create_database, save_database,
    sync_library, LibraryDatabase, ManualGameSetting,
};
use crate::steam::{find_active_steam_id, find_steam_dir, parse_non_steam_shortcuts, NonSteamShortcut};
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

    // Database Manager search
    db_search_query: String,

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

        let mut db = load_or_create_database();
        // Sync database on startup
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

        let default_output = dirs::picture_dir()
            .or_else(dirs::desktop_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("achievements_passport.png");

        let mut app = Self {
            db,
            data: None,
            active_tab: AppTab::Canvas,

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

            status_notification: None,
            preview_texture: None,
            last_rendered_svg: None,
        };

        app.refresh_data_and_preview(&cc.egui_ctx);
        app
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
            // Render 1200x850 PNG for preview
            match render_svg_to_png(&svg, 1200, 850) {
                Ok(png_bytes) => {
                    if let Ok(image) = image::load_from_memory(&png_bytes) {
                        let size = [image.width() as _, image.height() as _];
                        let image_buffer = image.to_rgba8();
                        let pixels = image_buffer.as_flat_samples();
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                            size,
                            pixels.as_slice(),
                        );
                        self.preview_texture = Some(ctx.load_texture(
                            "passport_preview",
                            color_image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
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

    fn detect_steam_shortcuts(&mut self) {
        if let Some(s_dir) = find_steam_dir() {
            if let Some(s_id) = find_active_steam_id(&s_dir) {
                self.detected_shortcuts = parse_non_steam_shortcuts(&s_dir, &s_id);
                if self.detected_shortcuts.is_empty() {
                    self.status_notification = Some((
                        "No Non-Steam shortcuts found in Steam config".to_string(),
                        false,
                    ));
                } else {
                    self.status_notification = Some((
                        format!("Found {} Non-Steam shortcut(s)", self.detected_shortcuts.len()),
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
                    egui::RichText::new("🎮 Achievements Passport")
                        .color(egui::Color32::from_rgb(0, 240, 255))
                        .strong(),
                );

                ui.add_space(20.0);

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
                                self.output_path = PathBuf::from(path_str);
                            }
                            if ui.button("Browse…").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_file_name("achievements_passport.png")
                                    .save_file()
                                {
                                    self.output_path = path;
                                }
                            }
                        });

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized([150.0, 36.0], egui::Button::new("📸 Export PNG (1200×850)"))
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

                    ui.add_space(20.0);
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
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);

                ui.heading(
                    egui::RichText::new("📚 Persistent Library Database")
                        .color(egui::Color32::from_rgb(0, 240, 255))
                        .strong(),
                );

                let db_path = get_database_path();
                ui.label(format!("Database Path: {}", db_path.display()));

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("📂 Open Database Folder").clicked() {
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

                ui.add_space(10.0);

                // Search Filter
                ui.horizontal(|ui| {
                    ui.label("Filter Games:");
                    ui.text_edit_singleline(&mut self.db_search_query);
                });

                ui.add_space(10.0);

                // Table of Games
                let query = self.db_search_query.to_lowercase();
                let mut game_keys: Vec<String> = self.db.games.keys().cloned().collect();
                game_keys.sort_by(|a, b| {
                    let ga = &self.db.games[a];
                    let gb = &self.db.games[b];
                    gb.pct.cmp(&ga.pct).then_with(|| ga.name.cmp(&gb.name))
                });

                let mut to_delete_key: Option<String> = None;

                egui::Grid::new("db_games_grid")
                    .striped(true)
                    .min_col_width(80.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Game Title").strong());
                        ui.label(egui::RichText::new("Platform").strong());
                        ui.label(egui::RichText::new("App ID").strong());
                        ui.label(egui::RichText::new("Achievements").strong());
                        ui.label(egui::RichText::new("Progress").strong());
                        ui.label(egui::RichText::new("Playtime").strong());
                        ui.label(egui::RichText::new("Source").strong());
                        ui.label(egui::RichText::new("Actions").strong());
                        ui.end_row();

                        for key in game_keys {
                            let game = &self.db.games[&key];
                            if !query.is_empty()
                                && !game.name.to_lowercase().contains(&query)
                                && !game.platform.to_lowercase().contains(&query)
                            {
                                continue;
                            }

                            ui.label(&game.name);
                            ui.label(&game.platform);
                            ui.label(game.appid.as_deref().unwrap_or("—"));
                            ui.label(format!("{}/{}", game.unlocked, game.total));
                            ui.label(format!("{}%", game.pct));
                            ui.label(format!("{}h {}m", game.playtime_minutes / 60, game.playtime_minutes % 60));
                            ui.label(&game.source);

                            if ui.button("🗑 Delete").clicked() {
                                to_delete_key = Some(key.clone());
                            }
                            ui.end_row();
                        }
                    });

                if let Some(key) = to_delete_key {
                    self.db.games.remove(&key);
                    let _ = save_database(&self.db);
                    self.refresh_data_and_preview(ctx);
                }
            });
        });
    }
}
