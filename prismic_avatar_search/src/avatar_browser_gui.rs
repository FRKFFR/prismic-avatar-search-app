use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    thread,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use eframe::egui::{self, Align, ColorImage, Frame, Image, Layout, Margin, Vec2};
use flume::{Receiver, Sender};
use image::ImageReader;
use open::that as open_url;
use reqwest::blocking::Client;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const API_BASE: &str = "https://api.vrchat.cloud/api/1";
const CONFIG_FILE: &str = "config.json";
const DATABASE_PATH: &str = "cache/avatars.db";
const MAX_PENDING: usize = 8;
const MAX_IMAGES_PER_FRAME: usize = 2;
const MAX_TEXTURE_CACHE: usize = 256;
const PAGE_SIZE: usize = 60;
const USER_AGENT: &str = "VRChatAvatarBrowser/0.1";
const CARD_WIDTH: f32 = 240.0;
const CARD_GAP: f32 = 12.0;
const CARD_IMAGE_WIDTH: f32 = 140.0;
const CARD_IMAGE_HEIGHT: f32 = 105.0; // 4:3 ratio (140 * 3/4 = 105)
const WORKER_COUNT: usize = 4;

// Search state for the advanced search panel
#[derive(Debug, Clone)]
pub struct SearchState {
    // Name search
    pub name_search_method: NameSearchMethod,
    pub name_include_desc_tags: bool,
    pub name_input: String,

    // Author search
    pub author_search_method: NameSearchMethod,
    pub author_input: String,

    // Platform filters
    pub filter_pc: bool,
    pub filter_android: bool,
    pub filter_ios: bool,

    // Performance filters
    pub pc_performance: PerformanceRating,
    pub quest_performance: PerformanceRating,
    pub ios_performance: PerformanceRating,

    // Style filters (max 2)
    pub selected_styles: Vec<StyleFilter>,

    // Content warnings
    pub warning_nudity: bool,
    pub warning_violence: bool,
    pub warning_gore: bool,

    // Result settings
    pub result_limit: ResultLimit,
    pub search_performance: SearchPerformance,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            name_search_method: NameSearchMethod::MatchPhrase,
            name_include_desc_tags: false,
            name_input: String::new(),
            author_search_method: NameSearchMethod::MatchPhrase,
            author_input: String::new(),
            filter_pc: true,
            filter_android: true,
            filter_ios: true,
            pc_performance: PerformanceRating::Any,
            quest_performance: PerformanceRating::Any,
            ios_performance: PerformanceRating::Any,
            selected_styles: Vec::new(),
            warning_nudity: false,
            warning_violence: false,
            warning_gore: false,
            result_limit: ResultLimit::Fifteen,
            search_performance: SearchPerformance::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameSearchMethod {
    MatchPhrase,
    ExactPhrase,
    StartsWith,
    EndsWith,
    MatchAllWords,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformanceRating {
    Any,
    Excellent,
    Good,
    Medium,
    Poor,
    VeryPoor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleFilter {
    Humanoid,
    Anime,
    Cartoon,
    Fantasy,
    Robot,
    Animal,
    Creature,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultLimit {
    Fifteen,
    FortyFive,
    SeventyFive,
    OneTwenty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchPerformance {
    Slow,
    Medium,
    Fast,
}

pub fn search_ui(ui: &mut egui::Ui, state: &mut SearchState) {
    ui.vertical_centered(|ui| {
        ui.heading("Advanced Search Dashboard");
        ui.add_space(10.0);
    });

    ui.columns(3, |columns| {
        // Left Column: Search by Name
        search_by_name_column(&mut columns[0], state);

        // Center Column: Global Filters
        global_filters_column(&mut columns[1], state);

        // Right Column: Search by Author
        search_by_author_column(&mut columns[2], state);
    });

    // Bottom: Search Button
    ui.add_space(15.0);
    if ui.add_sized(egui::Vec2::new(ui.available_width(), 50.0), egui::Button::new(
        egui::RichText::new("SEARCH").size(24.0).strong()
    ).fill(egui::Color32::from_gray(40))).clicked() {
        // Trigger search
    }
}

fn search_by_name_column(ui: &mut egui::Ui, state: &mut SearchState) {
    ui.vertical(|ui| {
        ui.heading("Name");
        ui.add_space(5.0);

        // Name Search Method ComboBox
        ui.horizontal(|ui| {
            ui.label("Method:");
            egui::ComboBox::from_id_source("name_search_method")
                .selected_text(format!("{:?}", state.name_search_method))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.name_search_method, NameSearchMethod::MatchPhrase, "Match Phrase");
                    ui.selectable_value(&mut state.name_search_method, NameSearchMethod::ExactPhrase, "Exact Phrase");
                    ui.selectable_value(&mut state.name_search_method, NameSearchMethod::StartsWith, "Starts With");
                    ui.selectable_value(&mut state.name_search_method, NameSearchMethod::EndsWith, "Ends With");
                    ui.selectable_value(&mut state.name_search_method, NameSearchMethod::MatchAllWords, "Match All Words");
                });
        });

        // Include Description & Tags checkbox
        ui.checkbox(&mut state.name_include_desc_tags, "Include Description & Tags");
        ui.add_space(10.0);

        // Input Box with > prefix and X button
        ui.horizontal(|ui| {
            ui.label(">");
            let text_edit = egui::TextEdit::singleline(&mut state.name_input)
                .hint_text("Enter avatar name...")
                .desired_width(ui.available_width() - 30.0);
            ui.add(text_edit);
            if ui.button("X").clicked() {
                state.name_input.clear();
            }
        });

        ui.add_space(15.0);

        // Virtual Keyboard
        virtual_keyboard(ui, &mut state.name_input);
    });
}

fn global_filters_column(ui: &mut egui::Ui, state: &mut SearchState) {
    ui.vertical(|ui| {
        ui.heading("Global Filters");
        ui.add_space(5.0);

        // Platform Compatibility
        ui.label("Platform Compatibility:");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.filter_pc, true, "PC");
            ui.selectable_value(&mut state.filter_android, true, "Android");
            ui.selectable_value(&mut state.filter_ios, true, "iOS");
        });

        ui.add_space(10.0);

        // Minimum Performance
        ui.label("Minimum Performance:");
        ui.horizontal(|ui| {
            performance_combo_box(ui, "PC", &mut state.pc_performance, egui::Color32::from_rgb(100, 149, 237));
            performance_combo_box(ui, "Quest", &mut state.quest_performance, egui::Color32::from_rgb(50, 205, 50));
            performance_combo_box(ui, "iOS", &mut state.ios_performance, egui::Color32::from_gray(128));
        });

        ui.add_space(10.0);

        // Style Filter (Max 2)
        ui.label("Style Filter (Max 2):");
        ui.add_space(5.0);
        style_filter_grid(ui, state);

        ui.add_space(10.0);

        // Content Warnings
        ui.label("Content Warnings:");
        ui.horizontal(|ui| {
            content_warning_button(ui, "Nudity", &mut state.warning_nudity);
            content_warning_button(ui, "Violence", &mut state.warning_violence);
            content_warning_button(ui, "Gore", &mut state.warning_gore);
        });

        ui.add_space(10.0);

        // Performance Bottom Row
        ui.horizontal(|ui| {
            ui.label("Result Limit:");
            egui::ComboBox::from_id_source("result_limit")
                .selected_text(format!("{:?}", state.result_limit))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.result_limit, ResultLimit::Fifteen, "15");
                    ui.selectable_value(&mut state.result_limit, ResultLimit::FortyFive, "45");
                    ui.selectable_value(&mut state.result_limit, ResultLimit::SeventyFive, "75");
                    ui.selectable_value(&mut state.result_limit, ResultLimit::OneTwenty, "120");
                });
        });

        ui.horizontal(|ui| {
            ui.label("Search Performance:");
            egui::ComboBox::from_id_source("search_performance")
                .selected_text(format!("{:?}", state.search_performance))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.search_performance, SearchPerformance::Slow, "Slow");
                    ui.selectable_value(&mut state.search_performance, SearchPerformance::Medium, "Medium");
                    ui.selectable_value(&mut state.search_performance, SearchPerformance::Fast, "Fast");
                });
        });
    });
}

fn search_by_author_column(ui: &mut egui::Ui, state: &mut SearchState) {
    ui.vertical(|ui| {
        ui.heading("Author");
        ui.add_space(5.0);

        // Author Search Method ComboBox
        ui.horizontal(|ui| {
            ui.label("Method:");
            egui::ComboBox::from_id_source("author_search_method")
                .selected_text(format!("{:?}", state.author_search_method))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.author_search_method, NameSearchMethod::MatchPhrase, "Match Phrase");
                    ui.selectable_value(&mut state.author_search_method, NameSearchMethod::ExactPhrase, "Exact Phrase");
                    ui.selectable_value(&mut state.author_search_method, NameSearchMethod::StartsWith, "Starts With");
                    ui.selectable_value(&mut state.author_search_method, NameSearchMethod::EndsWith, "Ends With");
                    ui.selectable_value(&mut state.author_search_method, NameSearchMethod::MatchAllWords, "Match All Words");
                });
        });

        // Avatar Creator FAQ hyperlink
        ui.hyperlink_to("Avatar Creator FAQ", "https://docs.vrchat.com/docs/avatar-creation-faq");

        ui.add_space(10.0);

        // Input Box with > prefix and X button
        ui.horizontal(|ui| {
            ui.label(">");
            let text_edit = egui::TextEdit::singleline(&mut state.author_input)
                .hint_text("Enter author name...")
                .desired_width(ui.available_width() - 30.0);
            ui.add(text_edit);
            if ui.button("X").clicked() {
                state.author_input.clear();
            }
        });

        ui.add_space(15.0);

        // Virtual Keyboard
        virtual_keyboard(ui, &mut state.author_input);
    });
}

fn virtual_keyboard(ui: &mut egui::Ui, input: &mut String) {
    ui.vertical(|ui| {
        // Number row
        ui.horizontal(|ui| {
            for key in &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"] {
                if ui.button(*key).clicked() {
                    input.push_str(key);
                }
            }
        });

        // QWERTY row
        ui.horizontal(|ui| {
            for key in &["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"] {
                if ui.button(*key).clicked() {
                    input.push_str(key);
                }
            }
        });

        // ASDF row
        ui.horizontal(|ui| {
            for key in &["A", "S", "D", "F", "G", "H", "J", "K", "L"] {
                if ui.button(*key).clicked() {
                    input.push_str(key);
                }
            }
        });

        // ZXCV row
        ui.horizontal(|ui| {
            for key in &["Z", "X", "C", "V", "B", "N", "M"] {
                if ui.button(*key).clicked() {
                    input.push_str(key);
                }
            }
        });

        // Bottom row with special keys
        ui.horizontal(|ui| {
            if ui.button("Tab").clicked() {
                input.push(' ');
            }
            if ui.button("Shift").clicked() {
                // Toggle shift mode (not implemented for simplicity)
            }
            if ui.button("Space").clicked() {
                input.push(' ');
            }
            if ui.button("Enter").clicked() {
                // Trigger search (not implemented)
            }
        });
    });
}

fn performance_combo_box(ui: &mut egui::Ui, label: &str, rating: &mut PerformanceRating, color: egui::Color32) {
    egui::Frame::none()
        .stroke(egui::Stroke::new(1.0, color))
        .show(ui, |ui| {
            ui.add_space(2.0);
            egui::ComboBox::from_id_source(format!("{}_performance", label))
                .selected_text(format!("{:?}", rating))
                .show_ui(ui, |ui| {
                    ui.selectable_value(rating, PerformanceRating::Any, "Any");
                    ui.selectable_value(rating, PerformanceRating::Excellent, "Excellent ★");
                    ui.selectable_value(rating, PerformanceRating::Good, "Good");
                    ui.selectable_value(rating, PerformanceRating::Medium, "Medium");
                    ui.selectable_value(rating, PerformanceRating::Poor, "Poor");
                    ui.selectable_value(rating, PerformanceRating::VeryPoor, "Very Poor !");
                });
            ui.add_space(2.0);
        });
}

fn style_filter_grid(ui: &mut egui::Ui, state: &mut SearchState) {
    let styles = [
        StyleFilter::Humanoid,
        StyleFilter::Anime,
        StyleFilter::Cartoon,
        StyleFilter::Fantasy,
        StyleFilter::Robot,
        StyleFilter::Animal,
        StyleFilter::Creature,
        StyleFilter::Other,
    ];

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            for i in 0..4 {
                let style = styles[i];
                let is_selected = state.selected_styles.contains(&style);
                let button_text = format!("{:?}", style);
                if ui.button(button_text).clicked() {
                    if is_selected {
                        state.selected_styles.retain(|s| *s != style);
                    } else if state.selected_styles.len() < 2 {
                        state.selected_styles.push(style);
                    }
                }
            }
        });

        ui.vertical(|ui| {
            for i in 4..8 {
                let style = styles[i];
                let is_selected = state.selected_styles.contains(&style);
                let button_text = format!("{:?}", style);
                if ui.button(button_text).clicked() {
                    if is_selected {
                        state.selected_styles.retain(|s| *s != style);
                    } else if state.selected_styles.len() < 2 {
                        state.selected_styles.push(style);
                    }
                }
            }
        });
    });
}

fn content_warning_button(ui: &mut egui::Ui, label: &str, selected: &mut bool) {
    let color = if *selected {
        egui::Color32::from_rgb(200, 50, 50)
    } else {
        egui::Color32::from_rgb(100, 30, 30)
    };
    let button = egui::Button::new(label).fill(color);
    if ui.add(button).clicked() {
        *selected = !*selected;
    }
}

// Compact Search Dashboard
pub fn draw_compact_search_dashboard(ui: &mut egui::Ui, app: &mut AvatarBrowserApp) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let old_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);

        ui.columns(3, |columns| {
            // Column 1: Name Search
            compact_name_column(&mut columns[0], app);

            // Column 2: Global Filters
            compact_filters_column(&mut columns[1], app);

            // Column 3: Author Search
            compact_author_column(&mut columns[2], app);
        });

        // Main Search Button
        ui.add_space(4.0);
        if ui.add_sized(
            egui::vec2(ui.available_width(), 32.0),
            egui::Button::new(egui::RichText::new("SEARCH").size(18.0).strong())
                .fill(egui::Color32::from_gray(40))
        ).clicked() {
            app.page = 0;
            app.apply_filters();
        }

        ui.spacing_mut().item_spacing = old_spacing;
    });
}

fn compact_name_column(ui: &mut egui::Ui, app: &mut AvatarBrowserApp) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("Name").size(14.0).strong());
        ui.horizontal(|ui| {
            ui.label("Method:");
            egui::ComboBox::from_id_source("compact_name_method")
                .selected_text("Match Phrase")
                .show_ui(ui, |ui| {
                    ui.label("Match Phrase");
                    ui.label("Exact Phrase");
                    ui.label("Starts With");
                    ui.label("Ends With");
                    ui.label("Match All Words");
                });
        });

        ui.add_space(2.0);

        // Search input with prefix and clear button
        ui.horizontal(|ui| {
            ui.label(">");
            let text_edit = egui::TextEdit::singleline(&mut app.search_name)
                .hint_text("Name...")
                .desired_width(ui.available_width() - 20.0);
            ui.add(text_edit);
            if ui.button("X").clicked() {
                app.search_name.clear();
            }
        });

        ui.add_space(2.0);

        // Compact virtual keyboard
        compact_virtual_keyboard(ui, &mut app.search_name);
    });
}

fn compact_filters_column(ui: &mut egui::Ui, app: &mut AvatarBrowserApp) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("Global Filters").size(14.0).strong());

        // Platform toggles
        ui.horizontal(|ui| {
            ui.checkbox(&mut app.filter_pc, "PC");
            ui.checkbox(&mut app.filter_quest, "Quest");
            ui.checkbox(&mut app.filter_ios, "iOS");
        });

        ui.add_space(2.0);

        // Performance ratings
        ui.label("Min Performance:");
        ui.horizontal(|ui| {
            compact_perf_combo(ui, "PC");
            compact_perf_combo(ui, "Quest");
            compact_perf_combo(ui, "iOS");
        });

        ui.add_space(2.0);

        // Style filters
        ui.label("Styles (Max 2):");
        let styles = ["Human", "Realistic", "Anime", "Cartoon", "Furry", "Animal", "Objects", "Fantasy", "Sci-Fi", "Pop Culture", "Fashion"];
        ui.horizontal(|ui| {
            for i in 0..4 {
                let style = styles[i];
                let is_selected = app.selected_styles.contains(style);
                let is_disabled = !is_selected && app.selected_styles.len() >= 2;
                let selected = is_selected;
                if ui.add_enabled(!is_disabled, egui::Button::new(style)).clicked() {
                    if selected {
                        app.selected_styles.remove(style);
                    } else {
                        app.selected_styles.insert(style.to_string());
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            for i in 4..8 {
                let style = styles[i];
                let is_selected = app.selected_styles.contains(style);
                let is_disabled = !is_selected && app.selected_styles.len() >= 2;
                let selected = is_selected;
                if ui.add_enabled(!is_disabled, egui::Button::new(style)).clicked() {
                    if selected {
                        app.selected_styles.remove(style);
                    } else {
                        app.selected_styles.insert(style.to_string());
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            for i in 8..11 {
                let style = styles[i];
                let is_selected = app.selected_styles.contains(style);
                let is_disabled = !is_selected && app.selected_styles.len() >= 2;
                let selected = is_selected;
                if ui.add_enabled(!is_disabled, egui::Button::new(style)).clicked() {
                    if selected {
                        app.selected_styles.remove(style);
                    } else {
                        app.selected_styles.insert(style.to_string());
                    }
                }
            }
        });

        ui.add_space(2.0);

        // Content warnings
        ui.label("Warnings:");
        ui.horizontal(|ui| {
            let warnings = ["Sexually Suggestive", "Adult Language", "Graphic Violence", "Excessive Gore", "Extreme Horror"];
            for warning in warnings {
                let is_selected = app.content_warnings.contains(warning);
                let color = if is_selected {
                    egui::Color32::from_rgb(200, 50, 50)
                } else {
                    egui::Color32::from_rgb(100, 30, 30)
                };
                if ui.button(egui::RichText::new(warning).color(color)).clicked() {
                    if is_selected {
                        app.content_warnings.remove(warning);
                    } else {
                        app.content_warnings.insert(warning.to_string());
                    }
                }
            }
        });

        ui.add_space(2.0);

        // Result settings
        ui.horizontal(|ui| {
            ui.label("Limit:");
            egui::ComboBox::from_id_source("compact_result_limit")
                .selected_text(format!("{}", app.result_limit))
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut app.result_limit, 15, "15").clicked() {}
                    if ui.selectable_value(&mut app.result_limit, 45, "45").clicked() {}
                    if ui.selectable_value(&mut app.result_limit, 75, "75").clicked() {}
                    if ui.selectable_value(&mut app.result_limit, 120, "120").clicked() {}
                });
        });
        ui.horizontal(|ui| {
            ui.label("Perf:");
            egui::ComboBox::from_id_source("compact_search_perf")
                .selected_text(app.search_performance.clone())
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut app.search_performance, "Slow".to_string(), "Slow").clicked() {}
                    if ui.selectable_value(&mut app.search_performance, "Medium".to_string(), "Medium").clicked() {}
                    if ui.selectable_value(&mut app.search_performance, "Fast".to_string(), "Fast").clicked() {}
                });
        });
    });
}

fn compact_author_column(ui: &mut egui::Ui, app: &mut AvatarBrowserApp) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("Author").size(14.0).strong());
        ui.horizontal(|ui| {
            ui.label("Method:");
            egui::ComboBox::from_id_source("compact_author_method")
                .selected_text("Match Phrase")
                .show_ui(ui, |ui| {
                    ui.label("Match Phrase");
                    ui.label("Exact Phrase");
                    ui.label("Starts With");
                    ui.label("Ends With");
                    ui.label("Match All Words");
                });
        });
        ui.hyperlink_to("FAQ", "https://docs.vrchat.com/docs/avatar-creation-faq");

        ui.add_space(2.0);

        // Search input with prefix and clear button
        ui.horizontal(|ui| {
            ui.label(">");
            let text_edit = egui::TextEdit::singleline(&mut app.search_author)
                .hint_text("Author...")
                .desired_width(ui.available_width() - 20.0);
            ui.add(text_edit);
            if ui.button("X").clicked() {
                app.search_author.clear();
            }
        });

        ui.add_space(2.0);

        // Compact virtual keyboard
        compact_virtual_keyboard(ui, &mut app.search_author);
    });
}

fn compact_virtual_keyboard(ui: &mut egui::Ui, input: &mut String) {
    let old_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing = egui::vec2(2.0, 1.0);

    ui.vertical(|ui| {
        // Number row
        ui.horizontal(|ui| {
            for key in &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"] {
                if ui.add_sized([20.0, 20.0], egui::Button::new(*key)).clicked() {
                    input.push_str(key);
                }
            }
        });

        // QWERTY row
        ui.horizontal(|ui| {
            for key in &["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P"] {
                if ui.add_sized([20.0, 20.0], egui::Button::new(*key)).clicked() {
                    input.push_str(key);
                }
            }
        });

        // ASDF row
        ui.horizontal(|ui| {
            for key in &["A", "S", "D", "F", "G", "H", "J", "K", "L"] {
                if ui.add_sized([20.0, 20.0], egui::Button::new(*key)).clicked() {
                    input.push_str(key);
                }
            }
        });

        // ZXCV row
        ui.horizontal(|ui| {
            for key in &["Z", "X", "C", "V", "B", "N", "M"] {
                if ui.add_sized([20.0, 20.0], egui::Button::new(*key)).clicked() {
                    input.push_str(key);
                }
            }
        });

        // Special keys
        ui.horizontal(|ui| {
            if ui.add_sized([30.0, 20.0], egui::Button::new("Space")).clicked() {
                input.push(' ');
            }
            if ui.add_sized([30.0, 20.0], egui::Button::new("Del")).clicked() {
                if !input.is_empty() {
                    input.pop();
                }
            }
        });
    });

    ui.spacing_mut().item_spacing = old_spacing;
}

fn compact_perf_combo(ui: &mut egui::Ui, label: &str) {
    let color = match label {
        "PC" => egui::Color32::from_rgb(100, 149, 237),
        "Quest" => egui::Color32::from_gray(128),
        "iOS" => egui::Color32::from_rgb(50, 205, 50),
        _ => egui::Color32::GRAY,
    };
    egui::Frame::none()
        .stroke(egui::Stroke::new(1.0, color))
        .show(ui, |ui| {
            egui::ComboBox::from_id_source(format!("compact_perf_{}", label))
                .selected_text("Any")
                .show_ui(ui, |ui| {
                    ui.label("Any");
                    ui.label("Excellent");
                    ui.label("Good");
                    ui.label("Medium");
                    ui.label("Poor");
                    ui.label("Very Poor");
                });
        });
}

#[derive(Debug, Clone, Default)]
struct AvatarEntry {
    name: String,
    author: String,
    description: String,
    avatar_id: String,
    tags: Vec<String>,
    quest: bool,
    ios: bool,
    platform: u8,
    impostor_platform: u8,
    pc_rating: u8,
    quest_rating: u8,
    ios_rating: u8,
    content_warnings: u8,
    style_filter: u16,
    marketplace: bool,
}

fn fetch_current_avatar_info(
    client: &Client,
    auth_cookie: &str,
) -> Result<(String, String, Option<Vec<u8>>)> {
    let user_resp = client
        .get(format!("{API_BASE}/auth/user"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .header("User-Agent", USER_AGENT)
        .send()
        .context("GET auth/user")?;
    if !user_resp.status().is_success() {
        return Err(anyhow!("Failed to fetch user profile: {}", user_resp.status()));
    }
    let user_json: serde_json::Value = user_resp.json()?;
    let current_id = user_json
        .get("currentAvatar")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("User profile missing current avatar"))?
        .to_string();

    let avatar_resp = client
        .get(format!("{API_BASE}/avatars/{current_id}"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .header("User-Agent", USER_AGENT)
        .send()
        .with_context(|| format!("GET avatar {}", current_id))?;
    if !avatar_resp.status().is_success() {
        return Err(anyhow!(
            "Failed to fetch current avatar data: {}",
            avatar_resp.status()
        ));
    }
    let avatar_json: serde_json::Value = avatar_resp.json()?;
    let name = avatar_json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let image_url = avatar_json
        .get("imageUrl")
        .and_then(|v| v.as_str())
        .or_else(|| avatar_json.get("thumbnailImageUrl").and_then(|v| v.as_str()));
    let image = if let Some(url) = image_url {
        Some(
            client
                .get(url)
                .header("Cookie", format!("auth={}", auth_cookie))
                .header("User-Agent", USER_AGENT)
                .send()
                .with_context(|| format!("GET image {}", url))?
                .bytes()?
                .to_vec(),
        )
    } else {
        None
    };

    Ok((current_id, name, image))
}

fn select_avatar_remote(client: &Client, auth_cookie: &str, avatar_id: &str) -> Result<()> {
    let resp = client
        .put(format!("{API_BASE}/avatars/{avatar_id}/select"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .header("User-Agent", USER_AGENT)
        .send()
        .with_context(|| format!("PUT select avatar {}", avatar_id))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(anyhow!("Select avatar failed: {}", resp.status()))
    }
}

impl AvatarBrowserApp {
    fn update_query_terms(&mut self) {
        self.search_name_lower = self.search_name.to_lowercase();
        self.search_author_lower = self.search_author.to_lowercase();
        self.search_description_lower = self.search_description.to_lowercase();
    }

    fn build_query_params(&self) -> Vec<String> {
        let mut params = Vec::new();
        
        if !self.search_name_lower.is_empty() {
            params.push(format!("%{}%", self.search_name_lower));
        }
        if !self.search_author_lower.is_empty() {
            params.push(format!("%{}%", self.search_author_lower));
        }
        if !self.search_description_lower.is_empty() {
            params.push(format!("%{}%", self.search_description_lower));
        }
        
        params
    }

    fn db_connection(&self) -> Result<Connection> {
        let conn = Connection::open(DATABASE_PATH)
            .with_context(|| "Failed to open database. Run the downloader to generate it.")?;
        // Optimize for read-heavy search workload
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA temp_store = MEMORY;"
        )?;
        Ok(conn)
    }

    fn reset_stream(&mut self) {
        self.stream_offset = 0;
        self.page = 0;
        self.total_avatars = 0;
        self.avatars.clear();
        self.filtered.clear();
        
        // Count matching avatars for proper pagination using SQLite
        if let Ok(conn) = self.db_connection() {
            let query = self.build_sql_count_query();
            let params = self.build_query_params();
            let query_params: Vec<&dyn rusqlite::ToSql> = params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            if let Ok(count) = conn.query_row(&query, query_params.as_slice(), |row| row.get::<_, i64>(0)) {
                self.total_avatars = count as usize;
            }
        }
        
        self.log(&format!("Found {} matching avatars out of total database", self.total_avatars));
    }

    fn build_sql_count_query(&self) -> String {
        let mut query = "SELECT COUNT(*) FROM avatars WHERE 1=1".to_string();
        
        if self.filter_pc {
            query.push_str(" AND (platform & 1) != 0");
        }
        if self.filter_quest {
            query.push_str(" AND (platform & 2) != 0");
        }
        if self.filter_ios {
            query.push_str(" AND (platform & 4) != 0");
        }
        if !self.search_name_lower.is_empty() {
            query.push_str(" AND name LIKE ?");
        }
        if !self.search_author_lower.is_empty() {
            query.push_str(" AND author LIKE ?");
        }
        if !self.search_description_lower.is_empty() {
            query.push_str(" AND description LIKE ?");
        }
        
        query
    }

    fn build_sql_query_with_pagination(&self) -> String {
        let mut query = "SELECT * FROM avatars WHERE 1=1".to_string();
        
        if self.filter_pc {
            query.push_str(" AND (platform & 1) != 0");
        }
        if self.filter_quest {
            query.push_str(" AND (platform & 2) != 0");
        }
        if self.filter_ios {
            query.push_str(" AND (platform & 4) != 0");
        }
        if !self.search_name_lower.is_empty() {
            query.push_str(" AND name LIKE ?");
        }
        if !self.search_author_lower.is_empty() {
            query.push_str(" AND author LIKE ?");
        }
        if !self.search_description_lower.is_empty() {
            query.push_str(" AND description LIKE ?");
        }
        
        query.push_str(" LIMIT ? OFFSET ?");
        query
    }

    fn load_page(&mut self, reset: bool) -> Result<()> {
        if reset {
            self.reset_stream();
            self.filtered.clear();
            self.details.clear();
            self.pending.clear();
            self.texture_order.clear();
        }

        // Always clear avatars for true pagination
        self.avatars.clear();
        self.filtered.clear();

        let loaded = self.load_next_page()?;
        self.filtered = (0..self.avatars.len()).collect();

        let total_pages = (self.total_avatars + PAGE_SIZE - 1) / PAGE_SIZE;
        if loaded == 0 {
            self.status = "No avatars match your filters.".into();
        } else {
            self.status = format!(
                "Page {} of {} ({} avatars)",
                self.page + 1,
                total_pages,
                self.avatars.len()
            );
        }

        Ok(())
    }

    fn load_next_page(&mut self) -> Result<usize> {
        let conn = self.db_connection()?;
        
        let query = self.build_sql_query_with_pagination();
        let offset = self.page * PAGE_SIZE;
        let mut params = self.build_query_params();
        params.push(PAGE_SIZE.to_string());
        params.push(offset.to_string());
        let query_params: Vec<&dyn rusqlite::ToSql> = params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        
        let mut stmt = conn.prepare(&query)?;
        let avatar_iter = stmt.query_map(query_params.as_slice(), |row| {
            let tags_json: String = row.get(5)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            
            Ok(AvatarEntry {
                name: row.get(1)?,
                author: row.get(2)?,
                description: row.get(3)?,
                avatar_id: row.get(4)?,
                tags,
                quest: row.get::<_, i32>(6)? != 0,
                ios: row.get::<_, i32>(7)? != 0,
                platform: row.get::<_, i64>(8)? as u8,
                impostor_platform: row.get::<_, i64>(9)? as u8,
                pc_rating: row.get::<_, i64>(10)? as u8,
                quest_rating: row.get::<_, i64>(11)? as u8,
                ios_rating: row.get::<_, i64>(12)? as u8,
                content_warnings: row.get::<_, i64>(13)? as u8,
                style_filter: row.get::<_, i64>(14)? as u16,
                marketplace: row.get::<_, i32>(15)? != 0,
            })
        })?;
        
        let mut loaded = 0usize;
        for avatar in avatar_iter {
            let avatar = avatar?;
            self.avatars.push(avatar);
            loaded += 1;
        }

        self.log(&format!("Loaded {} avatars for page {}", loaded, self.page + 1));
        Ok(loaded)
    }
}

fn default_avatar_id() -> String {
    "unknown".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    auth_cookie: String,
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    remember_me: bool,
}

struct AvatarDetail {
    image: Option<egui::TextureHandle>,
    status: Option<String>,
}

#[derive(Debug)]
enum WorkerRequest {
    FetchAvatar { avatar_id: String },
    FetchCurrentAvatar,
    SelectAvatar { avatar_id: String },
}

#[derive(Debug)]
enum WorkerResponse {
    AvatarImage { 
        avatar_id: String, 
        image: Option<Vec<u8>>,
        status: Option<String>, // "Loaded", "Unavailable", "Deleted", "No Image", "Decode Failed"
    },
    CurrentAvatar {
        avatar_id: String,
        name: String,
        image: Option<Vec<u8>>,
    },
    SelectResult {
        avatar_id: String,
        success: bool,
        message: String,
    },
    Error(String),
}

fn load_config() -> Config {
    PathBuf::from(CONFIG_FILE)
        .exists()
        .then(|| fs::read_to_string(CONFIG_FILE).ok())
        .flatten()
        .and_then(|text| serde_json::from_str::<Config>(&text).ok())
        .unwrap_or_default()
}

fn spawn_worker(auth_cookie: String) -> (Sender<WorkerRequest>, Receiver<WorkerResponse>) {
    let (tx, rx) = flume::unbounded();
    let (rtx, rrx) = flume::unbounded();

    for _ in 0..WORKER_COUNT {
        let rx = rx.clone();
        let rtx = rtx.clone();
        let auth_cookie = auth_cookie.clone();
        thread::spawn(move || {
            let client = match Client::builder()
                .cookie_store(true)
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(30))
                .build()
            {
                Ok(client) => client,
                Err(err) => {
                    let _ = rtx.send(WorkerResponse::Error(format!(
                        "HTTP client init failed: {}",
                        err
                    )));
                    return;
                }
            };

            while let Ok(message) = rx.recv() {
                match message {
                    WorkerRequest::FetchAvatar { avatar_id } => {
                        let result = fetch_avatar_image(&client, &auth_cookie, &avatar_id)
                            .map(|(bytes, status)| WorkerResponse::AvatarImage {
                                avatar_id,
                                image: bytes,
                                status,
                            })
                            .unwrap_or_else(|err| WorkerResponse::Error(err.to_string()));
                        let _ = rtx.send(result);
                    }
                    WorkerRequest::FetchCurrentAvatar => {
                        let response = fetch_current_avatar_info(&client, &auth_cookie)
                            .map(|(avatar_id, name, image)| WorkerResponse::CurrentAvatar {
                                avatar_id,
                                name,
                                image,
                            })
                            .unwrap_or_else(|err| WorkerResponse::Error(err.to_string()));
                        let _ = rtx.send(response);
                    }
                    WorkerRequest::SelectAvatar { avatar_id } => {
                        let outcome = select_avatar_remote(&client, &auth_cookie, &avatar_id)
                            .map(|_| WorkerResponse::SelectResult {
                                avatar_id: avatar_id.clone(),
                                success: true,
                                message: "Avatar equipped".into(),
                            })
                            .unwrap_or_else(|err| WorkerResponse::SelectResult {
                                avatar_id: avatar_id.clone(),
                                success: false,
                                message: err.to_string(),
                            });
                        let _ = rtx.send(outcome);
                        if let Ok((id, name, image)) = fetch_current_avatar_info(&client, &auth_cookie)
                        {
                            let _ = rtx.send(WorkerResponse::CurrentAvatar {
                                avatar_id: id,
                                name,
                                image,
                            });
                        }
                    }
                }
            }
        });
    }

    (tx, rrx)
}

fn fetch_avatar_image(
    client: &Client,
    auth_cookie: &str,
    avatar_id: &str,
) -> Result<(Option<Vec<u8>>, Option<String>)> {
    let response = client
        .get(format!("{API_BASE}/avatars/{avatar_id}"))
        .header("Cookie", format!("auth={}", auth_cookie))
        .send()
        .with_context(|| format!("GET avatar {}", avatar_id))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok((None, Some("Deleted".to_string()))); // Avatar deleted/banned/private
    }

    if !response.status().is_success() {
        return Err(anyhow!("API returned status {}", response.status()));
    }

    let json: serde_json::Value = response.json()?;
    let image_url = json
        .get("imageUrl")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("thumbnailImageUrl").and_then(|v| v.as_str()));
    
    if image_url.is_none() {
        return Ok((None, Some("No Image".to_string()))); // Avatar exists but has no image (old avatar)
    }

    let bytes = client
        .get(image_url.unwrap())
        .header("Cookie", format!("auth={}", auth_cookie))
        .send()
        .with_context(|| format!("GET image {}", image_url.unwrap()))?
        .bytes()?;

    Ok((Some(bytes.to_vec()), Some("Loaded".to_string())))
}

pub struct AvatarBrowserApp {
    avatars: Vec<AvatarEntry>,
    filtered: Vec<usize>,
    search_name: String,
    search_author: String,
    search_description: String,
    search_name_lower: String,
    search_author_lower: String,
    search_description_lower: String,
    filter_pc: bool,
    filter_quest: bool,
    filter_ios: bool,
    details: HashMap<String, AvatarDetail>,
    pending: HashSet<String>,
    sender: Option<Sender<WorkerRequest>>,
    receiver: Option<Receiver<WorkerResponse>>,
    status: String,
    last_error: Option<String>,
    texture_order: VecDeque<String>,
    stream_offset: u64,
    page: usize,
    total_avatars: usize,
    current_avatar_id: Option<String>,
    current_avatar_name: Option<String>,
    current_avatar_image: Option<egui::TextureHandle>,
    selection_message: Option<String>,
    show_debug: bool,
    debug_log: Vec<String>,
    show_enlarged: bool,
    enlarged_avatar: Option<AvatarEntry>,
    enlarged_image: Option<egui::TextureHandle>,
    search_state: SearchState,
    show_current_avatar: bool,
    selected_styles: std::collections::HashSet<String>,
    content_warnings: std::collections::HashSet<String>,
    result_limit: u32,
    search_performance: String,
}

impl AvatarBrowserApp {
    // Sanitize text to remove characters that might render as squares
    fn sanitize_text(text: &str) -> String {
        text.chars()
            .filter(|c| {
                // Keep ASCII and common Unicode ranges
                // This filters out emoji and other problematic characters
                let code = *c as u32;
                (code >= 0x20 && code <= 0x7E) || // ASCII printable
                (code >= 0xA0 && code <= 0xFF) || // Latin-1 supplement
                (code >= 0x100 && code <= 0x24F) || // Latin extended
                (code >= 0x300 && code <= 0x36F) || // Combining diacritics
                (code >= 0x1F00 && code <= 0x1FFF) // Greek and Coptic
            })
            .collect()
    }

    pub fn new() -> Self {
        let config = load_config();

        let (sender, receiver) = if config.auth_cookie.is_empty() {
            (None, None)
        } else {
            let (tx, rx) = spawn_worker(config.auth_cookie);
            (Some(tx), Some(rx))
        };

        let mut app = Self {
            avatars: Vec::new(),
            filtered: Vec::new(),
            search_name: String::new(),
            search_author: String::new(),
            search_description: String::new(),
            search_name_lower: String::new(),
            search_author_lower: String::new(),
            search_description_lower: String::new(),
            filter_pc: false,
            filter_quest: false,
            filter_ios: false,
            details: HashMap::new(),
            pending: HashSet::new(),
            sender,
            receiver,
            status: "Loading...".into(),
            last_error: None,
            texture_order: VecDeque::new(),
            stream_offset: 0,
            page: 0,
            total_avatars: 0,
            current_avatar_id: None,
            current_avatar_name: None,
            current_avatar_image: None,
            selection_message: None,
            show_debug: false,
            debug_log: Vec::new(),
            show_enlarged: false,
            enlarged_avatar: None,
            enlarged_image: None,
            search_state: SearchState::default(),
            show_current_avatar: true,
            selected_styles: std::collections::HashSet::new(),
            content_warnings: std::collections::HashSet::new(),
            result_limit: 15,
            search_performance: "Medium".to_string(),
        };

        app.log("Avatar Browser initialized");

        if let Err(err) = app.load_page(true) {
            app.log(&format!("Initialization error: {}", err));
            app.last_error = Some(err.to_string());
        }

        if app.sender.is_some() {
            app.request_current_avatar();
        } else {
            app.status = "Login required to view images and wear avatars.".into();
        }

        app
    }

    fn log(&mut self, message: &str) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let seconds = timestamp / 1000;
        let millis = timestamp % 1000;
        let time_str = format!("{:02}:{:02}:{:02}.{:03}", 
            (seconds / 3600) % 24,
            (seconds / 60) % 60,
            seconds % 60,
            millis
        );
        self.debug_log.push(format!("[{}] {}", time_str, message));
        
        // Keep only last 1000 log entries
        if self.debug_log.len() > 1000 {
            self.debug_log.remove(0);
        }
    }

    fn apply_filters(&mut self) {
        println!("=== Active Filters ===");
        println!("Name: '{}'", self.search_name);
        println!("Author: '{}'", self.search_author);
        println!("Description: '{}'", self.search_description);
        println!("Platform filters: PC={}, Quest={}, iOS={}", self.filter_pc, self.filter_quest, self.filter_ios);
        println!("Selected Styles: {:?}", self.selected_styles);
        println!("Content Warnings: {:?}", self.content_warnings);
        println!("Result Limit: {}", self.result_limit);
        println!("Search Performance: {}", self.search_performance);
        println!("====================");

        self.log(&format!("Applying filters: name='{}', author='{}', desc='{}', PC={}, Quest={}, iOS={}",
            self.search_name, self.search_author, self.search_description,
            self.filter_pc, self.filter_quest, self.filter_ios));
        self.log(&format!("Styles: {:?}, Warnings: {:?}", self.selected_styles, self.content_warnings));
        self.update_query_terms();
        if let Err(err) = self.load_page(true) {
            self.log(&format!("Error applying filters: {}", err));
            self.last_error = Some(err.to_string());
        } else {
            self.log(&format!("Filters applied: {} results", self.filtered.len()));
        }
    }

    fn matches_query(&self, avatar: &AvatarEntry) -> bool {
        if self.filter_pc && avatar.platform & 1 == 0 {
            return false;
        }
        if self.filter_quest && avatar.platform & 2 == 0 {
            return false;
        }
        if self.filter_ios && avatar.platform & 4 == 0 {
            return false;
        }

        if !self.search_name_lower.is_empty()
            && !avatar
                .name
                .to_lowercase()
                .contains(&self.search_name_lower)
        {
            return false;
        }

        if !self.search_author_lower.is_empty()
            && !avatar
                .author
                .to_lowercase()
                .contains(&self.search_author_lower)
        {
            return false;
        }

        if !self.search_description_lower.is_empty()
            && !avatar
                .description
                .to_lowercase()
                .contains(&self.search_description_lower)
        {
            return false;
        }

        // Filter by selected styles
        if !self.selected_styles.is_empty() {
            let avatar_styles = self.decode_style_filter(avatar.style_filter);
            let has_matching_style = self.selected_styles.iter().any(|s| avatar_styles.contains(s));
            if !has_matching_style {
                return false;
            }
        }

        // Filter by content warnings (exclude avatars with selected warnings)
        if !self.content_warnings.is_empty() {
            if self.content_warnings.contains("Sexually Suggestive") && avatar.content_warnings & 1 != 0 {
                return false;
            }
            if self.content_warnings.contains("Adult Language") && avatar.content_warnings & 2 != 0 {
                return false;
            }
            if self.content_warnings.contains("Graphic Violence") && avatar.content_warnings & 4 != 0 {
                return false;
            }
            if self.content_warnings.contains("Excessive Gore") && avatar.content_warnings & 8 != 0 {
                return false;
            }
            if self.content_warnings.contains("Extreme Horror") && avatar.content_warnings & 16 != 0 {
                return false;
            }
        }

        true
    }

    fn decode_style_filter(&self, style_filter: u16) -> Vec<String> {
        let mut styles = Vec::new();
        // Decode style filter bits to style names (matching database schema)
        if style_filter & 1 != 0 { styles.push("Pop Culture".to_string()); }
        if style_filter & 2 != 0 { styles.push("Furry".to_string()); }
        if style_filter & 4 != 0 { styles.push("Sci-Fi".to_string()); }
        if style_filter & 8 != 0 { styles.push("Anime".to_string()); }
        if style_filter & 16 != 0 { styles.push("Cartoon".to_string()); }
        if style_filter & 32 != 0 { styles.push("Objects".to_string()); }
        if style_filter & 64 != 0 { styles.push("Human".to_string()); }
        if style_filter & 128 != 0 { styles.push("Realistic".to_string()); }
        if style_filter & 256 != 0 { styles.push("Animal".to_string()); }
        if style_filter & 512 != 0 { styles.push("Fantasy".to_string()); }
        if style_filter & 1024 != 0 { styles.push("Fashion".to_string()); }
        styles
    }

    fn next_page(&mut self) {
        let total_pages = (self.total_avatars + PAGE_SIZE - 1) / PAGE_SIZE;
        if self.page + 1 >= total_pages {
            self.log("Already on last page");
            return;
        }
        self.page += 1;
        self.log(&format!("Navigating to page {} of {}", self.page + 1, total_pages));
        if let Err(err) = self.load_page(false) {
            self.log(&format!("Error loading page: {}", err));
            self.last_error = Some(err.to_string());
        }
    }

    fn prev_page(&mut self) {
        if self.page == 0 {
            self.log("Already on first page");
            return;
        }
        self.page -= 1;
        self.log(&format!("Navigating to page {}", self.page + 1));
        // Load from the beginning up to the target page
        self.reset_stream();
        for _ in 0..self.page {
            let _ = self.load_next_page(); // Skip pages
        }
        // Load the target page
        if let Err(err) = self.load_page(false) {
            self.log(&format!("Error loading page: {}", err));
            self.last_error = Some(err.to_string());
        }
    }

    fn request_current_avatar(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(WorkerRequest::FetchCurrentAvatar);
        }
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        let mut needs_repaint = false;
        if let Some(receiver) = &self.receiver {
            let mut image_budget = MAX_IMAGES_PER_FRAME;
            let mut drained = Vec::new();
            while let Ok(message) = receiver.try_recv() {
                if matches!(message, WorkerResponse::AvatarImage { .. }) {
                    if image_budget == 0 {
                        // Leave further image responses for the next frame
                        break;
                    }
                    image_budget -= 1;
                }
                drained.push(message);
            }

            for message in drained {
                needs_repaint = true;
                match message {
                    WorkerResponse::AvatarImage { avatar_id, image, status } => {
                        self.pending.remove(&avatar_id);
                        if let Some(bytes) = image {
                            match texture_from_bytes(ctx, &avatar_id, &bytes) {
                                Ok(texture) => {
                                    self.log(&format!("Successfully loaded image for avatar {}", avatar_id));
                                    self.details.insert(
                                        avatar_id.clone(),
                                        AvatarDetail {
                                            image: Some(texture),
                                            status: status,
                                        },
                                    );
                                    self.texture_order.push_back(avatar_id);
                                    self.prune_texture_cache();
                                }
                                Err(err) => {
                                    self.log(&format!("Failed to decode image for avatar {}: {}", avatar_id, err));
                                    self.last_error = Some(err.to_string());
                                    // Mark as decode failed to prevent re-requesting
                                    self.details.insert(
                                        avatar_id.clone(),
                                        AvatarDetail {
                                            image: None,
                                            status: Some("Decode Failed".to_string()),
                                        },
                                    );
                                    self.texture_order.push_back(avatar_id);
                                    self.prune_texture_cache();
                                }
                            }
                        } else {
                            // Handle different types of unavailability
                            let status_msg = status.unwrap_or_else(|| "Unavailable".to_string());
                            match status_msg.as_str() {
                                "Deleted" => {
                                    self.log(&format!("Avatar {} has been deleted/banned/private", avatar_id));
                                }
                                "No Image" => {
                                    self.log(&format!("Avatar {} exists but has no image (old avatar)", avatar_id));
                                }
                                _ => {
                                    self.log(&format!("No image available for avatar {}", avatar_id));
                                }
                            }
                            self.details.insert(
                                avatar_id.clone(),
                                AvatarDetail {
                                    image: None,
                                    status: Some(status_msg),
                                },
                            );
                            self.texture_order.push_back(avatar_id);
                            self.prune_texture_cache();
                        }
                    }
                    WorkerResponse::CurrentAvatar {
                        avatar_id,
                        name,
                        image,
                    } => {
                        self.log(&format!("Current avatar updated: {} ({})", name, avatar_id));
                        self.current_avatar_id = Some(avatar_id.clone());
                        self.current_avatar_name = Some(name.clone());
                        if let Some(bytes) = image {
                            match texture_from_bytes(ctx, &avatar_id, &bytes) {
                                Ok(texture) => {
                                    self.log("Successfully loaded current avatar image");
                                    self.current_avatar_image = Some(texture);
                                }
                                Err(err) => {
                                    self.log(&format!("Failed to decode current avatar image: {}", err));
                                    self.last_error = Some(err.to_string());
                                }
                            }
                        }
                    }
                    WorkerResponse::SelectResult {
                        avatar_id,
                        success,
                        message,
                    } => {
                        self.log(&format!("Avatar selection result for {}: {} - {}", 
                            avatar_id, if success { "SUCCESS" } else { "FAILED" }, message));
                        if success {
                            self.current_avatar_id = Some(avatar_id);
                            self.request_current_avatar(); // Refresh current avatar display
                        }
                        self.selection_message = Some(message);
                    }
                    WorkerResponse::Error(err) => {
                        self.log(&format!("Worker error: {}", err));
                        self.last_error = Some(err);
                    }
                }
            }
        }

        // Also repaint if there are pending images to show loading animation
        if needs_repaint || !self.pending.is_empty() {
            ctx.request_repaint();
        }
    }

    fn request_avatar(&mut self, avatar_id: &str) {
        if self.pending.len() >= MAX_PENDING {
            return;
        }
        if self.pending.contains(avatar_id) {
            return;
        }
        // Check if image is already loaded
        if let Some(detail) = self.details.get(avatar_id) {
            if detail.image.is_some() || detail.status.as_ref().map_or(false, |s| {
                s == "Unavailable" || s == "Decode Failed" || s == "Deleted" || s == "No Image"
            }) {
                return; // Already loaded or known to be unavailable/failed/deleted/no image
            }
        }
        if let Some(sender) = &self.sender {
            if sender
                .send(WorkerRequest::FetchAvatar {
                    avatar_id: avatar_id.to_string(),
                })
                .is_ok()
            {
                self.pending.insert(avatar_id.to_string());
                self.log(&format!("Requested image for avatar {}", avatar_id));
            }
        }
    }

    fn render_avatar_card(&mut self, ui: &mut egui::Ui, avatar: &AvatarEntry) {
        let avatar_id = avatar.avatar_id.clone();
        self.request_avatar(&avatar_id);

        // Check if avatar is removed to determine frame styling
        let is_removed = self.details.get(&avatar_id)
            .and_then(|d| d.status.as_ref())
            .map_or(false, |s| s == "Deleted");

        let frame_stroke = if is_removed {
            egui::Stroke::new(3.0, egui::Color32::RED)
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_gray(60))
        };

        Frame::group(ui.style())
            .inner_margin(Margin::same(10.0))
            .stroke(frame_stroke)
            .show(ui, |ui| {
                ui.set_width(CARD_WIDTH);
                ui.vertical(|ui| {
                    ui.set_width(CARD_WIDTH - 8.0);
                    
                    // Image with platforms on the right
                    let mut image_loaded = false;
                    ui.horizontal(|ui| {
                        if let Some(detail) = self.details.get(&avatar_id) {
                            if let Some(texture) = &detail.image {
                                let img = Image::new(texture)
                                    .fit_to_exact_size(Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT))
                                    .sense(egui::Sense::click());
                                let response = ui.add(img);
                                if response.clicked() {
                                    self.show_enlarged = true;
                                    self.enlarged_avatar = Some(avatar.clone());
                                    self.enlarged_image = Some(texture.clone());
                                }
                                image_loaded = true;
                            } else {
                                // Check if this is a deleted/banned avatar
                                let is_deleted = detail.status.as_ref().map_or(false, |s| s == "Deleted");
                                
                                if is_deleted {
                                    // Show red cross for deleted/banned avatars
                                    let cross_rect = egui::Rect::from_min_size(
                                        ui.cursor().min,
                                        Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT)
                                    );
                                    
                                    // Draw background
                                    ui.painter().rect_filled(
                                        cross_rect,
                                        4.0,
                                        egui::Color32::from_gray(40)
                                    );
                                    
                                    // Draw red X
                                    let center = cross_rect.center();
                                    let size = 30.0;
                                    let stroke = egui::Stroke::new(4.0, egui::Color32::RED);
                                    
                                    // Draw X lines
                                    ui.painter().line_segment(
                                        [center - egui::Vec2::new(size, size), center + egui::Vec2::new(size, size)],
                                        stroke
                                    );
                                    ui.painter().line_segment(
                                        [center - egui::Vec2::new(size, -size), center + egui::Vec2::new(size, -size)],
                                        stroke
                                    );
                                    
                                    // Draw "(Removed)" text on the image
                                    let text_pos = egui::Pos2::new(center.x, center.y + size + 15.0);
                                    let text_color = egui::Color32::RED;
                                    ui.painter().text(
                                        text_pos,
                                        egui::Align2::CENTER_CENTER,
                                        "(Removed)",
                                        egui::FontId::default(),
                                        text_color,
                                    );
                                    
                                    ui.allocate_exact_size(Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT), egui::Sense::hover());
                                } else {
                                    ui.label(detail.status.clone().unwrap_or_else(|| "Unavailable".into()));
                                }
                                image_loaded = true;
                            }
                        } else {
                            // YouTube-style loading placeholder
                            let loading_rect = egui::Rect::from_min_size(
                                ui.cursor().min,
                                Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT)
                            );
                            ui.painter().rect_filled(
                                loading_rect,
                                4.0,
                                egui::Color32::from_gray(40)
                            );
                            
                            // Animated loading spinner using circle_stroke
                            let time = ui.ctx().animate_value_with_time(
                                egui::Id::new(format!("loading_{}", avatar_id)),
                                0.0,
                                2.0
                            );
                            let center = loading_rect.center();
                            let radius = 12.0;
                            let start_angle = time * std::f32::consts::TAU;
                            let _end_angle = start_angle + std::f32::consts::FRAC_PI_2;
                            
                            // Draw arc using circle_stroke
                            ui.painter().circle_stroke(
                                center,
                                radius,
                                egui::Stroke::new(3.0, egui::Color32::WHITE),
                            );
                            
                            ui.allocate_exact_size(Vec2::new(CARD_IMAGE_WIDTH, CARD_IMAGE_HEIGHT), egui::Sense::hover());
                        }
                        
                        // Platform tags on the right
                        ui.vertical(|ui| {
                            if avatar.platform & 1 != 0 {
                                ui.colored_label(egui::Color32::from_rgb(0, 255, 0), "PC");
                            }
                            if avatar.platform & 2 != 0 {
                                ui.colored_label(egui::Color32::from_rgb(255, 165, 0), "Quest");
                            }
                            if avatar.platform & 4 != 0 {
                                ui.colored_label(egui::Color32::from_rgb(0, 122, 255), "iOS");
                            }
                        });
                    });

                    ui.label(egui::RichText::new(&Self::sanitize_text(&avatar.name)).strong());
                    ui.label(&Self::sanitize_text(&avatar.author));
                    ui.label(
                        egui::RichText::new(&Self::sanitize_text(&avatar.description))
                            .italics()
                            .small(),
                    );

                    ui.horizontal(|ui| {
                        // Check if avatar is removed to disable buttons
                        let is_removed = self.details.get(&avatar_id)
                            .and_then(|d| d.status.as_ref())
                            .map_or(false, |s| s == "Deleted");
                        
                        ui.add_enabled(!is_removed, egui::Button::new("Open Web"))
                            .clicked()
                            .then(|| {
                                let url = format!("https://vrchat.com/home/avatar/{}", avatar_id);
                                self.log(&format!("Opening web page for avatar: {}", url));
                                let _ = open_url(url);
                            });
                        if image_loaded && ui.button("Refresh").clicked() {
                            self.log(&format!("Refreshing image for avatar {}", avatar_id));
                            self.details.remove(&avatar_id);
                            self.request_avatar(&avatar_id);
                        }
                        if self.sender.is_some() {
                            ui.add_enabled(!is_removed, egui::Button::new("Wear"))
                                .clicked()
                                .then(|| {
                                    self.log(&format!("Attempting to wear avatar: {}", avatar_id));
                                    if let Some(sender) = &self.sender {
                                        let _ = sender.send(WorkerRequest::SelectAvatar {
                                            avatar_id: avatar_id.to_string(),
                                        });
                                    }
                                    self.selection_message =
                                        Some(format!("Equipping {}...", &avatar.name));
                                });
                        }
                    });
                });
            });
    }

    fn prune_texture_cache(&mut self) {
        while self.texture_order.len() > MAX_TEXTURE_CACHE {
            if let Some(old_id) = self.texture_order.pop_front() {
                if let Some(detail) = self.details.get(&old_id) {
                    if detail.image.is_some() {
                        self.details.remove(&old_id);
                        continue;
                    }
                }
                self.details.remove(&old_id);
            }
        }
    }
}

impl eframe::App for AvatarBrowserApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);

        egui::TopBottomPanel::top("filters").show(ctx, |ui| {
            ui.label(&self.status);
            ui.horizontal(|ui| {
                if ui.button("👁").clicked() {
                    self.show_current_avatar = !self.show_current_avatar;
                }
            });

            if self.show_current_avatar {
                ui.horizontal(|ui| {
                    ui.group(|ui| {
                        ui.label("Current Avatar");
                        if let Some(texture) = &self.current_avatar_image {
                            let img = Image::new(texture).fit_to_exact_size(egui::vec2(96.0, 96.0));
                            ui.add(img);
                        } else {
                            ui.label("(not loaded)");
                        }
                    });
                    ui.vertical(|ui| {
                        if let Some(name) = &self.current_avatar_name {
                            ui.strong(name);
                        } else {
                            ui.label("Unknown");
                        }
                        if let Some(id) = &self.current_avatar_id {
                            ui.label(id);
                        }
                        if ui.button("Refresh Current").clicked() {
                            self.log("Refreshing current avatar");
                            self.request_current_avatar();
                        }
                        if let Some(msg) = &self.selection_message {
                            ui.label(msg);
                        }
                    });
                });
            }

            // Compact Search Dashboard
            draw_compact_search_dashboard(ui, self);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let _prev_spacing = ui.spacing().item_spacing;
                ui.spacing_mut().item_spacing = Vec2::new(CARD_GAP, CARD_GAP);
                let filtered_copy: Vec<usize> = self.filtered.iter().copied().collect();
                let available_width = ui.available_width();
                let cols = (available_width / (CARD_WIDTH + CARD_GAP)).max(1.0) as usize;
                for row in filtered_copy.chunks(cols) {
                    ui.horizontal(|ui| {
                        for &idx in row {
                            if let Some(avatar) = self.avatars.get(idx).cloned() {
                                self.render_avatar_card(ui, &avatar);
                            }
                        }
                    });
                }
            });
        });

        // Show enlarged image modal
        if self.show_enlarged {
            let avatar_clone = self.enlarged_avatar.clone();
            let texture_clone = self.enlarged_image.clone();
            
            egui::Window::new("Enlarged Avatar")
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(600.0, 600.0))
                .show(ctx, |ui| {
                    if let Some(avatar) = avatar_clone {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new(&Self::sanitize_text(&avatar.name)).size(20.0).strong());
                            ui.label(&Self::sanitize_text(&avatar.author));
                            ui.label(&Self::sanitize_text(&avatar.description));
                            ui.add_space(10.0);
                            
                            if let Some(ref texture) = texture_clone {
                                let img = Image::new(texture)
                                    .fit_to_exact_size(egui::vec2(500.0, 500.0));
                                ui.add(img);
                            }
                            
                            ui.add_space(10.0);
                            if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                self.show_enlarged = false;
                                self.enlarged_avatar = None;
                                self.enlarged_image = None;
                            }
                        });
                    }
                });
        }

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Debug").clicked() {
                    self.show_debug = !self.show_debug;
                }
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ui.label(&self.status);
            });
        });

        // Debug console window
        if self.show_debug {
            egui::Window::new("Debug Console")
                .collapsible(true)
                .resizable(true)
                .default_size([800.0, 600.0])
                .fixed_size([800.0, 600.0])
                .show(ctx, |ui| {
                    ui.heading("Debug Console");
                    ui.separator();
                    
                    // Log output area with fixed size
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .max_height(400.0)
                        .show(ui, |ui| {
                            ui.set_min_size(egui::Vec2::new(760.0, 400.0));
                            ui.set_max_size(egui::Vec2::new(760.0, 400.0));
                            
                            // Use vertical layout to ensure proper spacing
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 2.0);
                                
                                for log_entry in &self.debug_log {
                                    ui.label(
                                        egui::RichText::new(log_entry)
                                            .monospace()
                                            .size(12.0)
                                    );
                                }
                            });
                        });
                    
                    ui.separator();
                    
                    // Control buttons
                    ui.horizontal(|ui| {
                        if ui.button("Clear Log").clicked() {
                            self.debug_log.clear();
                            self.log("Log cleared");
                        }
                        
                        if ui.button("Refresh Status").clicked() {
                            self.log(&format!("Status: {}", self.status));
                            self.log(&format!("Total avatars: {}", self.total_avatars));
                            self.log(&format!("Current page: {}", self.page + 1));
                            self.log(&format!("Filtered count: {}", self.filtered.len()));
                            self.log(&format!("Pending images: {}", self.pending.len()));
                            self.log(&format!("Texture cache size: {}", self.details.len()));
                        }
                        
                        if ui.button("Test Log").clicked() {
                            self.log("Test log message");
                        }
                    });
                    
                    ui.separator();
                    
                    // Manual command input
                    ui.horizontal(|ui| {
                        ui.label("CMD:");
                        let response = ui.text_edit_singleline(&mut String::new());
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            // For now, just echo the command
                            // You can extend this to handle actual commands
                            self.log(&format!("> Command entered (not implemented yet)"));
                        }
                    });
                });
        }
    }
}

fn texture_from_bytes(
    ctx: &egui::Context,
    key: &str,
    bytes: &[u8],
) -> Result<egui::TextureHandle> {
    let guessed = image::guess_format(bytes).unwrap_or(image::ImageFormat::Png);
    let reader = ImageReader::with_format(std::io::Cursor::new(bytes), guessed);
    let image = reader.decode().context("Failed to decode image")?;
    let size = [image.width() as usize, image.height() as usize];
    let rgba = image.to_rgba8();
    let color_image = ColorImage::from_rgba_unmultiplied(size, rgba.as_flat_samples().as_slice());
    Ok(ctx.load_texture(key.to_string(), color_image, egui::TextureOptions::LINEAR))
}

fn main() {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    
    if let Err(err) = eframe::run_native(
        "VRChat Avatar Browser",
        options,
        Box::new(|_cc| Box::new(AvatarBrowserApp::new())),
    ) {
        eprintln!("Failed to launch avatar browser: {err}");
    }
}
