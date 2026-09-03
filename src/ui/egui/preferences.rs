//! egui preferences dialog/window.

use std::path::PathBuf;
use std::sync::Arc;

use crate::app::command::UiCommand;
use crate::app::preferences_model::{
    clamped_scale_factor, mirror_live_preferences_fields, set_scale_factor, title_format_preview,
};
use crate::config::{Config, TimerMode};
use crate::skin::widget::{
    VisAnalyzerMode, VisAnalyzerStyle, VisFalloffSpeed, VisMode, VisScopeMode, VisVuMode,
};
use crate::skin::SkinEntry;

#[cfg(target_os = "android")]
use super::android_runtime::AndroidLayoutSnapshot;
use super::app::EguiFrontendState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesPage {
    #[default]
    AudioIoPlugins,
    VisualizationPlugins,
    Options,
    Fonts,
    Title,
    Skins,
    Playlists,
}

#[derive(Debug, Clone)]
enum AndroidSkinAction {
    Default,
    Import,
    Select(PathBuf),
    Delete(PathBuf),
}

#[derive(Debug, Clone)]
pub struct PreferencesViewportState {
    pub open: bool,
    pub selected_page: PreferencesPage,
    pub android_show_categories: bool,
    pub config: Config,
    pub changed: bool,
    pub close_requested: bool,
    pub skin_browser_requested: bool,
    pub playlist_manager_requested: bool,
    android_skin_action: Option<AndroidSkinAction>,
    android_swipe_start: Option<egui::Pos2>,
    android_swipe_last: Option<egui::Pos2>,
}

impl PreferencesViewportState {
    pub fn new(config: &Config, selected_page: PreferencesPage, open: bool) -> Self {
        Self {
            open,
            selected_page,
            android_show_categories: true,
            config: config.clone(),
            changed: false,
            close_requested: false,
            skin_browser_requested: false,
            playlist_manager_requested: false,
            android_skin_action: None,
            android_swipe_start: None,
            android_swipe_last: None,
        }
    }
}

fn android_skins_page_visible(state: &PreferencesViewportState) -> bool {
    !state.android_show_categories && state.selected_page == PreferencesPage::Skins
}

pub fn show_preferences(ctx: &egui::Context, app: &mut EguiFrontendState) {
    apply_pending_viewport_state(app);
    sync_viewport_state_from_app(app);

    #[cfg(target_os = "android")]
    {
        let state = Arc::clone(&app.ui.preferences_viewport);
        let skins_page_visible = {
            let state = state.lock().expect("preferences viewport state poisoned");
            android_skins_page_visible(&state)
        };
        if skins_page_visible {
            app.ensure_runtime_skins();
        }
        let skin_entries = app.skin_entries.clone();
        let mut state = state.lock().expect("preferences viewport state poisoned");
        let before = state.config.clone();
        show_android_preferences(ctx, &mut state, &skin_entries, app.android.ready_layout());
        let changed = state.config != before;
        if changed {
            state.changed = true;
        }
        drop(state);
        if changed {
            ctx.request_repaint();
        }
        return;
    }

    #[cfg(not(target_os = "android"))]
    {
        let state = Arc::clone(&app.ui.preferences_viewport);
        let builder = egui::ViewportBuilder::default()
            .with_title("Preferences")
            .with_inner_size(egui::vec2(560.0, 460.0))
            .with_min_inner_size(egui::vec2(420.0, 300.0))
            .with_resizable(true)
            .with_decorations(true);

        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("xmms-egui-preferences"),
            builder,
            move |ctx, class| {
                let mut state = state.lock().expect("preferences viewport state poisoned");
                if ctx.input(|input| input.viewport().close_requested()) {
                    state.open = false;
                    state.close_requested = true;
                    return;
                }

                let before = state.config.clone();
                match class {
                    egui::ViewportClass::EmbeddedWindow | egui::ViewportClass::Root => {
                        show_preferences_embedded(ctx, &mut state);
                    }
                    egui::ViewportClass::Deferred | egui::ViewportClass::Immediate => {
                        egui::CentralPanel::default()
                            .show(ctx, |ui| show_preferences_contents(ui, &mut state));
                    }
                }
                if state.config != before {
                    state.changed = true;
                }
            },
        );
    }
}

fn sync_viewport_state_from_app(app: &mut EguiFrontendState) {
    let mut state = app
        .ui
        .preferences_viewport
        .lock()
        .expect("preferences viewport state poisoned");
    if !state.open && app.preferences_open() {
        let config = app.controller().state().persistence_snapshot().config;
        *state = PreferencesViewportState::new(&config, app.ui.selected_preferences_page, true);
    } else if !app.preferences_open() {
        state.open = false;
    } else {
        // While Preferences stays open, panel visibility/detach/shade and
        // volume/balance can change from the main window, menus, shortcuts, or
        // by closing a window. Mirror those live so the checkboxes/sliders don't
        // show a stale value (the snapshot is only re-seeded when opening).
        let live = app.controller().state().persistence_snapshot().config;
        mirror_live_preferences_fields(&mut state.config, &live);
        #[cfg(target_os = "android")]
        {
            state.config.skin = app.controller().state().config.skin.clone();
        }
    }
}

fn apply_pending_viewport_state(app: &mut EguiFrontendState) {
    let mut save_config = false;
    let mut queue_render = false;
    let mut open_skin_browser = false;
    #[cfg(target_os = "android")]
    let mut open_playlist_manager = false;
    let mut next_open = app.preferences_open();
    let next_page;
    let mut next_config = None;
    #[cfg(target_os = "android")]
    let android_skin_action;

    {
        let mut state = app
            .ui
            .preferences_viewport
            .lock()
            .expect("preferences viewport state poisoned");
        next_page = state.selected_page;
        if state.close_requested {
            next_open = false;
            state.close_requested = false;
        }
        if state.changed {
            next_config = Some(state.config.clone());
            save_config = true;
            queue_render = true;
            state.changed = false;
        }
        if state.skin_browser_requested {
            open_skin_browser = true;
            state.skin_browser_requested = false;
        }
        #[cfg(target_os = "android")]
        {
            if state.playlist_manager_requested {
                open_playlist_manager = true;
                state.playlist_manager_requested = false;
            }
            android_skin_action = state.android_skin_action.take();
        }
    }

    if next_open != app.preferences_open() {
        app.dispatch(UiCommand::SetPreferencesVisible(next_open));
    }
    app.ui.selected_preferences_page = next_page;
    if let Some(config) = next_config {
        app.apply_preferences_config(config);
    }
    if open_skin_browser {
        app.dispatch(UiCommand::SetSkinBrowserVisible(true));
    }
    #[cfg(target_os = "android")]
    if open_playlist_manager {
        app.android.playlist_manager.open();
    }
    #[cfg(target_os = "android")]
    if let Some(action) = android_skin_action {
        match action {
            AndroidSkinAction::Default => app.select_default_skin(),
            AndroidSkinAction::Import => app.import_skin(),
            AndroidSkinAction::Select(path) => app.select_skin_path(path),
            AndroidSkinAction::Delete(path) => app.delete_skin_path(path),
        }
    }
    if save_config {
        app.apply_effect(crate::app::effect::AppEffect::SaveConfig);
    }
    if queue_render {
        app.apply_effect(crate::app::effect::AppEffect::QueueRender(
            crate::app::effect::RenderTarget::All,
        ));
    }
}

#[cfg(not(target_os = "android"))]
fn show_preferences_embedded(ctx: &egui::Context, state: &mut PreferencesViewportState) {
    let mut open = state.open;
    egui::Window::new("Preferences")
        .open(&mut open)
        .resizable(true)
        .default_width(520.0)
        .constrain(false)
        .show(ctx, |ui| show_preferences_contents(ui, state));
    state.open = open;
    if !open {
        state.close_requested = true;
    }
}

#[cfg(target_os = "android")]
fn show_android_preferences(
    ctx: &egui::Context,
    state: &mut PreferencesViewportState,
    skin_entries: &[SkinEntry],
    layout: Option<AndroidLayoutSnapshot>,
) {
    update_android_swipe_start(ctx, state);
    let pixels_per_point = ctx.pixels_per_point().max(f32::EPSILON);
    let Some(layout) = layout else {
        return;
    };
    let screen = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(
            layout.width as f32 / pixels_per_point,
            layout.height as f32 / pixels_per_point,
        ),
    );
    let back_requested = ctx.input(|input| input.key_pressed(egui::Key::Escape))
        || take_android_back_swipe(ctx, state, screen);
    if back_requested {
        navigate_android_preferences_back(state);
        ctx.request_repaint();
    }
    if !state.open {
        return;
    }

    let insets = layout.insets;
    let left_inset = insets.left as f32 / pixels_per_point;
    let top_inset = insets.top as f32 / pixels_per_point;
    let right_inset = insets.right as f32 / pixels_per_point;
    let bottom_inset = insets.bottom as f32 / pixels_per_point;
    let horizontal_margin = 16.0;
    let vertical_margin = 12.0;
    let content_width =
        (screen.width() - left_inset - right_inset - horizontal_margin * 2.0).max(1.0);
    let content_height =
        (screen.height() - top_inset - bottom_inset - vertical_margin * 2.0).max(1.0);
    let fill = egui::Color32::from_gray(46);

    egui::Area::new(egui::Id::new("xmms-android-preferences"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            ui.painter().rect_filled(ui.max_rect(), 0.0, fill);
            ui.add_space(top_inset + vertical_margin);
            ui.horizontal(|ui| {
                ui.add_space(left_inset + horizontal_margin);
                ui.vertical(|ui| {
                    ui.set_width(content_width);
                    ui.set_min_height(content_height);
                    apply_android_preferences_style(ui);
                    show_android_preferences_header(ui, state);
                    ui.separator();
                    if state.android_show_categories {
                        show_android_preferences_categories(ui, state);
                    } else {
                        show_android_preferences_page(ui, state, skin_entries);
                    }
                });
            });
        });
}

#[cfg(target_os = "android")]
pub(crate) fn apply_android_preferences_style(ui: &mut egui::Ui) {
    let style = ui.style_mut();
    style.visuals.panel_fill = egui::Color32::from_gray(46);
    style.visuals.window_fill = egui::Color32::from_gray(46);
    style.visuals.extreme_bg_color = egui::Color32::from_gray(28);
    style.visuals.override_text_color = Some(egui::Color32::from_gray(245));
    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_gray(68);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(68);
    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_gray(92);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(92);
    style.visuals.widgets.active.weak_bg_fill = egui::Color32::from_gray(112);
    style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(112);
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.interact_size.y = 48.0;
    style.spacing.icon_width = 28.0;
    style.spacing.icon_width_inner = 16.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(18.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(18.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
}

#[cfg(target_os = "android")]
fn show_android_preferences_header(ui: &mut egui::Ui, state: &mut PreferencesViewportState) {
    ui.horizontal(|ui| {
        let back_label = if state.android_show_categories {
            "Close"
        } else {
            "Back"
        };
        if ui
            .add_sized([88.0, 48.0], egui::Button::new(back_label))
            .clicked()
        {
            navigate_android_preferences_back(state);
        }
        ui.heading(if state.android_show_categories {
            "Settings"
        } else {
            state.selected_page.android_label()
        });
    });
}

#[cfg(target_os = "android")]
fn show_android_preferences_categories(ui: &mut egui::Ui, state: &mut PreferencesViewportState) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            for page in PreferencesPage::ANDROID_ALL {
                if ui
                    .add_sized(
                        [ui.available_width(), 56.0],
                        egui::Button::new(page.android_label()),
                    )
                    .clicked()
                {
                    state.selected_page = page;
                    if page == PreferencesPage::Playlists {
                        state.open = false;
                        state.close_requested = true;
                        state.playlist_manager_requested = true;
                    } else {
                        state.android_show_categories = false;
                    }
                }
            }
        });
}

#[cfg(target_os = "android")]
fn show_android_preferences_page(
    ui: &mut egui::Ui,
    state: &mut PreferencesViewportState,
    skin_entries: &[SkinEntry],
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if state.selected_page == PreferencesPage::Skins {
                show_android_skins_page(ui, state, skin_entries);
            } else {
                show_selected_preferences_page(ui, state);
            }
            if android_page_shows_reset(state.selected_page) {
                ui.separator();
                if ui
                    .add_sized(
                        [ui.available_width(), 48.0],
                        egui::Button::new("Reset to Defaults"),
                    )
                    .clicked()
                {
                    state.config = Config::default();
                    state.changed = true;
                }
            }
        });
}

#[cfg(any(target_os = "android", test))]
fn android_page_shows_reset(page: PreferencesPage) -> bool {
    !matches!(
        page,
        PreferencesPage::Fonts | PreferencesPage::Skins | PreferencesPage::Playlists
    )
}

#[cfg(any(target_os = "android", test))]
fn navigate_android_preferences_back(state: &mut PreferencesViewportState) {
    if state.android_show_categories {
        state.open = false;
        state.close_requested = true;
    } else {
        state.android_show_categories = true;
    }
}

#[cfg(not(target_os = "android"))]
fn show_preferences_contents(ui: &mut egui::Ui, state: &mut PreferencesViewportState) {
    ui.horizontal_wrapped(|ui| {
        for page in [
            PreferencesPage::AudioIoPlugins,
            PreferencesPage::VisualizationPlugins,
            PreferencesPage::Options,
            PreferencesPage::Fonts,
            PreferencesPage::Title,
        ] {
            if ui
                .selectable_label(state.selected_page == page, page.label())
                .clicked()
            {
                state.selected_page = page;
            }
        }
    });
    ui.separator();
    show_selected_preferences_page(ui, state);
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Reset to Defaults").clicked() {
            state.config = Config::default();
            state.changed = true;
        }
        if ui.button("Close").clicked() {
            state.open = false;
            state.close_requested = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}

impl PreferencesPage {
    const ALL: [Self; 5] = [
        Self::AudioIoPlugins,
        Self::VisualizationPlugins,
        Self::Options,
        Self::Fonts,
        Self::Title,
    ];

    #[cfg(any(target_os = "android", test))]
    const ANDROID_ALL: [Self; 6] = [
        Self::VisualizationPlugins,
        Self::Options,
        Self::Fonts,
        Self::Title,
        Self::Skins,
        Self::Playlists,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::AudioIoPlugins => "Audio I/O Plugins",
            Self::VisualizationPlugins => "Visualization Plugins",
            Self::Options => "Options",
            Self::Fonts => "Fonts",
            Self::Title => "Title",
            Self::Skins => "Skins",
            Self::Playlists => "Playlists",
        }
    }

    #[cfg(target_os = "android")]
    fn android_label(self) -> &'static str {
        match self {
            Self::AudioIoPlugins => "Audio",
            Self::VisualizationPlugins => "Visualization",
            Self::Options => "Player",
            Self::Fonts => "Fonts",
            Self::Title => "Track titles",
            Self::Skins => "Skins",
            Self::Playlists => "Playlists",
        }
    }
}

fn show_selected_preferences_page(ui: &mut egui::Ui, state: &mut PreferencesViewportState) {
    match state.selected_page {
        PreferencesPage::AudioIoPlugins => show_audio_page(ui, &mut state.config),
        PreferencesPage::VisualizationPlugins => show_visualization_page(ui, &mut state.config),
        PreferencesPage::Options => show_options_page(ui, &mut state.config),
        PreferencesPage::Fonts => show_fonts_page(ui, state),
        PreferencesPage::Title => show_title_page(ui, &mut state.config),
        PreferencesPage::Skins | PreferencesPage::Playlists => {}
    }
}

#[cfg(target_os = "android")]
fn show_android_skins_page(
    ui: &mut egui::Ui,
    state: &mut PreferencesViewportState,
    skin_entries: &[SkinEntry],
) {
    ui.heading("Skins");
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                [ui.available_width() * 0.48, 48.0],
                egui::Button::new("Default"),
            )
            .clicked()
        {
            state.android_skin_action = Some(AndroidSkinAction::Default);
        }
        if ui
            .add_sized(
                [ui.available_width(), 48.0],
                egui::Button::new("Add skin..."),
            )
            .clicked()
        {
            state.android_skin_action = Some(AndroidSkinAction::Import);
        }
    });
    ui.separator();
    for entry in skin_entries {
        let path = entry.path.to_string_lossy();
        let selected = state.config.skin.as_deref() == Some(path.as_ref());
        ui.horizontal(|ui| {
            let can_delete = super::app::is_user_imported_skin_path(&entry.path);
            let delete_width = if can_delete { 88.0 } else { 0.0 };
            let select_width =
                (ui.available_width() - delete_width - ui.spacing().item_spacing.x).max(48.0);
            if ui
                .add_sized(
                    [select_width, 52.0],
                    egui::Button::selectable(selected, &entry.name),
                )
                .clicked()
            {
                state.android_skin_action = Some(AndroidSkinAction::Select(entry.path.clone()));
            }
            if can_delete
                && ui
                    .add_sized([delete_width, 52.0], egui::Button::new("Delete"))
                    .clicked()
            {
                state.android_skin_action = Some(AndroidSkinAction::Delete(entry.path.clone()));
            }
        });
    }
    if skin_entries.is_empty() {
        ui.label("No additional skins found. Use Add skin to choose a WSZ file.");
    }
}

fn show_audio_page(ui: &mut egui::Ui, config: &mut Config) {
    #[cfg(target_os = "android")]
    {
        let _ = config;
        ui.heading("Audio");
        ui.label("Output: Android system media routing");
        ui.label("Headphones, Bluetooth, and speaker selection are managed by Android.");
        return;
    }
    #[cfg(not(target_os = "android"))]
    {
        ui.heading("Audio I/O Plugins");
        ui.label("Output plugin: GStreamer");
        ui.horizontal(|ui| {
            ui.label("Output device:");
            let mut device = config
                .output_device
                .clone()
                .unwrap_or_else(|| "System default".to_string());
            egui::ComboBox::from_id_salt("egui-output-device")
                .selected_text(&device)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut device,
                        "System default".to_string(),
                        "System default",
                    );
                });
            config.output_device = (device != "System default").then_some(device);
        });
        ui.add_enabled(false, egui::Button::new("Configure"))
            .on_disabled_hover_text(
                "GStreamer output configuration is handled by the system for egui.",
            );
        ui.separator();
        ui.label("Input plugins are handled by GStreamer and playlist import helpers.");
    }
}

fn show_visualization_page(ui: &mut egui::Ui, config: &mut Config) {
    ui.heading("Visualization Plugins");
    #[cfg(target_os = "android")]
    {
        android_combo(
            ui,
            "Visualization mode",
            &mut config.vis_mode,
            &[
                (VisMode::Analyzer, "Analyzer"),
                (VisMode::Scope, "Scope"),
                (VisMode::Off, "Off"),
            ],
        );
        android_combo(
            ui,
            "Analyzer mode",
            &mut config.vis_analyzer_mode,
            &[
                (VisAnalyzerMode::Normal, "Normal"),
                (VisAnalyzerMode::Fire, "Fire"),
                (VisAnalyzerMode::VerticalLines, "Vertical lines"),
            ],
        );
        android_combo(
            ui,
            "Analyzer style",
            &mut config.vis_analyzer_style,
            &[
                (VisAnalyzerStyle::Bars, "Bars"),
                (VisAnalyzerStyle::Lines, "Lines"),
            ],
        );
        android_combo(
            ui,
            "Scope mode",
            &mut config.vis_scope_mode,
            &[
                (VisScopeMode::Dot, "Dot"),
                (VisScopeMode::Line, "Line"),
                (VisScopeMode::Solid, "Solid"),
            ],
        );
        ui.checkbox(&mut config.vis_peaks_enabled, "Peaks");
        android_combo(
            ui,
            "Analyzer falloff",
            &mut config.vis_analyzer_falloff,
            &falloff_options(),
        );
        android_combo(
            ui,
            "Peaks falloff",
            &mut config.vis_peaks_falloff,
            &falloff_options(),
        );
        android_combo(
            ui,
            "Window Shade Level Meter Style",
            &mut config.vis_vu_mode,
            &[
                (VisVuMode::Segmented, "Segmented"),
                (VisVuMode::Smooth, "Smooth"),
            ],
        );
        android_combo(
            ui,
            "Refresh rate",
            &mut config.vis_refresh_divisor,
            &[
                (1, "Full (~50 fps)"),
                (2, "Half (~25 fps)"),
                (4, "Quarter (~13 fps)"),
                (8, "Eighth (~6 fps)"),
            ],
        );
        return;
    }
    #[cfg(not(target_os = "android"))]
    {
        combo(
            ui,
            "Visualization mode",
            &mut config.vis_mode,
            &[
                (VisMode::Analyzer, "Analyzer"),
                (VisMode::Scope, "Scope"),
                (VisMode::Off, "Off"),
            ],
        );
        combo(
            ui,
            "Analyzer mode",
            &mut config.vis_analyzer_mode,
            &[
                (VisAnalyzerMode::Normal, "Normal"),
                (VisAnalyzerMode::Fire, "Fire"),
                (VisAnalyzerMode::VerticalLines, "Vertical lines"),
            ],
        );
        combo(
            ui,
            "Analyzer style",
            &mut config.vis_analyzer_style,
            &[
                (VisAnalyzerStyle::Bars, "Bars"),
                (VisAnalyzerStyle::Lines, "Lines"),
            ],
        );
        combo(
            ui,
            "Scope mode",
            &mut config.vis_scope_mode,
            &[
                (VisScopeMode::Dot, "Dot"),
                (VisScopeMode::Line, "Line"),
                (VisScopeMode::Solid, "Solid"),
            ],
        );
        ui.checkbox(&mut config.vis_peaks_enabled, "Peaks");
        combo(
            ui,
            "Analyzer falloff",
            &mut config.vis_analyzer_falloff,
            &falloff_options(),
        );
        combo(
            ui,
            "Peaks falloff",
            &mut config.vis_peaks_falloff,
            &falloff_options(),
        );
        combo(
            ui,
            "Window Shade Level Meter Style",
            &mut config.vis_vu_mode,
            &[
                (VisVuMode::Segmented, "Segmented"),
                (VisVuMode::Smooth, "Smooth"),
            ],
        );
        combo(
            ui,
            "Refresh rate",
            &mut config.vis_refresh_divisor,
            &[
                (1, "Full (~50 fps)"),
                (2, "Half (~25 fps)"),
                (4, "Quarter (~13 fps)"),
                (8, "Eighth (~6 fps)"),
            ],
        );
    }
}

fn show_options_page(ui: &mut egui::Ui, config: &mut Config) {
    ui.heading("Options");
    #[cfg(target_os = "android")]
    {
        ui.heading("Playback");
        android_slider(ui, "Volume", egui::Slider::new(&mut config.volume, 0..=100));
        android_slider(
            ui,
            "Balance",
            egui::Slider::new(&mut config.balance, -100..=100),
        );
        ui.checkbox(&mut config.repeat, "Repeat");
        ui.checkbox(&mut config.shuffle, "Shuffle");
        ui.checkbox(&mut config.no_playlist_advance, "No playlist advance");
        ui.checkbox(&mut config.pause_between_songs, "Pause between songs");
        if config.pause_between_songs {
            android_slider(
                ui,
                "Pause duration in seconds",
                egui::Slider::new(&mut config.pause_between_songs_time, 0..=30),
            );
        }
        ui.checkbox(&mut config.stop_with_fadeout, "Stop with fadeout");
        let mut remaining = config.timer_mode == TimerMode::Remaining;
        if ui.checkbox(&mut remaining, "Show time remaining").changed() {
            config.timer_mode = if remaining {
                TimerMode::Remaining
            } else {
                TimerMode::Elapsed
            };
        }

        ui.separator();
        ui.heading("Layout");
        let mut scale_factor = clamped_scale_factor(config.scale_factor);
        android_slider(
            ui,
            "Player zoom",
            egui::Slider::new(&mut scale_factor, 1.0..=5.0).suffix("x"),
        );
        set_scale_factor(config, scale_factor);
        ui.checkbox(&mut config.playlist_visible, "Show playlist");
        ui.checkbox(&mut config.equalizer_visible, "Show equalizer");
        return;
    }
    #[cfg(not(target_os = "android"))]
    {
        ui.add(egui::Slider::new(&mut config.volume, 0..=100).text("Volume"));
        ui.add(egui::Slider::new(&mut config.balance, -100..=100).text("Balance"));
        let mut scale_factor = clamped_scale_factor(config.scale_factor);
        ui.add(
            egui::Slider::new(&mut scale_factor, 1.0..=5.0)
                .text("Zoom level")
                .suffix("x"),
        );
        set_scale_factor(config, scale_factor);
        ui.add(
            egui::Slider::new(&mut config.pause_between_songs_time, 0..=30)
                .text("Pause between songs seconds"),
        );
        ui.add(
            egui::Slider::new(&mut config.mouse_wheel_change, 1..=25)
                .text("Mouse wheel volume step"),
        );
        ui.checkbox(&mut config.repeat, "Repeat");
        ui.checkbox(&mut config.shuffle, "Shuffle");
        ui.checkbox(&mut config.no_playlist_advance, "No playlist advance");
        ui.checkbox(&mut config.pause_between_songs, "Pause between songs");
        ui.checkbox(&mut config.stop_with_fadeout, "Stop with fadeout");
        let mut remaining = config.timer_mode == TimerMode::Remaining;
        if ui.checkbox(&mut remaining, "Time remaining").changed() {
            config.timer_mode = if remaining {
                TimerMode::Remaining
            } else {
                TimerMode::Elapsed
            };
        }
        ui.checkbox(&mut config.playlist_visible, "Show playlist");
        ui.checkbox(&mut config.equalizer_visible, "Show equalizer");
        ui.checkbox(&mut config.playlist_detached, "Detach playlist");
        ui.checkbox(&mut config.equalizer_detached, "Detach equalizer");
        ui.checkbox(&mut config.convert_twenty, "Convert %20 to space");
        ui.checkbox(
            &mut config.convert_underscore,
            "Convert underscore to space",
        );
        ui.checkbox(&mut config.show_numbers_in_pl, "Show numbers in playlist");
        ui.checkbox(
            &mut config.vim_playlist_navigation,
            "Vim-style playlist navigation",
        );
    }
}

fn playlist_font_size_from_descriptor(descriptor: &str) -> f64 {
    descriptor
        .split_whitespace()
        .filter_map(|token| token.parse::<f64>().ok())
        .find(|value| *value > 0.0)
        .unwrap_or(10.0)
}

fn playlist_font_descriptor_for_size(size: f64) -> String {
    let size = size.clamp(6.0, 24.0);
    if (size - size.round()).abs() < 0.05 {
        format!("Helvetica Bold {}", size.round() as i32)
    } else {
        format!("Helvetica Bold {:.1}", size)
    }
}

fn show_fonts_page(ui: &mut egui::Ui, state: &mut PreferencesViewportState) {
    ui.heading("Fonts");
    let mut playlist_font_size = playlist_font_size_from_descriptor(&state.config.playlist_font);
    #[cfg(target_os = "android")]
    let font_size_changed = android_slider(
        ui,
        "Playlist font size",
        egui::Slider::new(&mut playlist_font_size, 6.0..=24.0).suffix(" px"),
    );
    #[cfg(not(target_os = "android"))]
    let font_size_changed = ui
        .add(
            egui::Slider::new(&mut playlist_font_size, 6.0..=24.0)
                .text("Playlist font size")
                .suffix(" px"),
        )
        .changed();
    if font_size_changed {
        state.config.playlist_font = playlist_font_descriptor_for_size(playlist_font_size);
    }
    #[cfg(not(target_os = "android"))]
    ui.label("Main window text uses the active skin bitmap font.");
    #[cfg(not(target_os = "android"))]
    ui.label("Skin bitmap font");
    #[cfg(not(target_os = "android"))]
    if ui.button("Open Skin Browser").clicked() {
        state.skin_browser_requested = true;
    }
}

#[cfg(target_os = "android")]
fn update_android_swipe_start(ctx: &egui::Context, state: &mut PreferencesViewportState) {
    let (pressed, position) = ctx.input(|input| {
        (
            input.pointer.any_pressed(),
            input
                .pointer
                .interact_pos()
                .or_else(|| input.pointer.latest_pos()),
        )
    });
    if pressed {
        state.android_swipe_start = position;
        state.android_swipe_last = position;
    } else if state.android_swipe_start.is_some() && position.is_some() {
        state.android_swipe_last = position;
    }
}

#[cfg(target_os = "android")]
fn take_android_back_swipe(
    ctx: &egui::Context,
    state: &mut PreferencesViewportState,
    screen: egui::Rect,
) -> bool {
    if !ctx.input(|input| input.pointer.any_released()) {
        return false;
    }
    let start = state.android_swipe_start.take();
    let end = state.android_swipe_last.take();
    start
        .zip(end)
        .is_some_and(|(start, end)| is_android_back_swipe(start, end, screen))
}

#[cfg(any(target_os = "android", test))]
fn is_android_back_swipe(start: egui::Pos2, end: egui::Pos2, screen: egui::Rect) -> bool {
    let delta = end - start;
    start.x <= screen.left() + 96.0 && delta.x >= 80.0 && delta.x >= delta.y.abs() * 1.5
}

fn show_title_page(ui: &mut egui::Ui, config: &mut Config) {
    #[cfg(target_os = "android")]
    ui.heading("Track titles");
    #[cfg(not(target_os = "android"))]
    ui.heading("Title");
    ui.label("Title format");
    ui.text_edit_singleline(&mut config.title_format);
    ui.label(
        "Tokens: %p performer, %a album, %t title, %n track number, %f filename, %F full path/URI.",
    );
    ui.label(format!("Preview: {}", title_format_preview(config)));
    #[cfg(target_os = "android")]
    {
        ui.separator();
        ui.heading("Playlist text");
        ui.checkbox(&mut config.convert_twenty, "Convert %20 to space");
        ui.checkbox(
            &mut config.convert_underscore,
            "Convert underscore to space",
        );
        ui.checkbox(&mut config.show_numbers_in_pl, "Show track numbers");
    }
}

#[cfg(not(target_os = "android"))]
fn combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    let selected = options
        .iter()
        .find(|(candidate, _)| candidate == value)
        .map(|(_, label)| *label)
        .unwrap_or("Unknown");
    egui::ComboBox::from_label(label)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (candidate, label) in options {
                ui.selectable_value(value, *candidate, *label);
            }
        });
}

#[cfg(target_os = "android")]
fn android_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    let selected = options
        .iter()
        .find(|(candidate, _)| candidate == value)
        .map(|(_, label)| *label)
        .unwrap_or("Unknown");
    ui.label(label);
    let width = ui.available_width();
    ui.scope(|ui| {
        let style = ui.style_mut();
        style.spacing.interact_size.y = 56.0;
        style.spacing.button_padding.y = 14.0;
        style.spacing.icon_width = 32.0;
        style.spacing.icon_width_inner = 18.0;
        egui::ComboBox::from_id_salt(("android-preferences", label))
            .width(width)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.set_min_width(width);
                for (candidate, option_label) in options {
                    if ui
                        .add_sized(
                            [ui.available_width(), 56.0],
                            egui::Button::selectable(*value == *candidate, *option_label),
                        )
                        .clicked()
                    {
                        *value = *candidate;
                    }
                }
            });
    });
}

#[cfg(target_os = "android")]
fn android_slider(ui: &mut egui::Ui, label: &str, slider: egui::Slider<'_>) -> bool {
    ui.label(label);
    ui.add_sized([ui.available_width(), 48.0], slider).changed()
}

fn falloff_options() -> [(VisFalloffSpeed, &'static str); 5] {
    [
        (VisFalloffSpeed::Slowest, "Slowest"),
        (VisFalloffSpeed::Slow, "Slow"),
        (VisFalloffSpeed::Medium, "Medium"),
        (VisFalloffSpeed::Fast, "Fast"),
        (VisFalloffSpeed::Fastest, "Fastest"),
    ]
}

pub fn page_labels() -> Vec<&'static str> {
    PreferencesPage::ALL
        .into_iter()
        .map(PreferencesPage::label)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_pages_have_stable_labels() {
        assert_eq!(
            page_labels(),
            vec![
                "Audio I/O Plugins",
                "Visualization Plugins",
                "Options",
                "Fonts",
                "Title"
            ]
        );
    }

    #[test]
    fn android_back_returns_to_categories_before_closing() {
        let mut state =
            PreferencesViewportState::new(&Config::default(), PreferencesPage::Options, true);
        state.android_show_categories = false;

        navigate_android_preferences_back(&mut state);
        assert!(state.android_show_categories);
        assert!(state.open);
        assert!(!state.close_requested);

        navigate_android_preferences_back(&mut state);
        assert!(!state.open);
        assert!(state.close_requested);
    }

    #[test]
    fn android_preferences_exclude_audio_and_hide_font_reset() {
        assert!(!PreferencesPage::ANDROID_ALL.contains(&PreferencesPage::AudioIoPlugins));
        assert!(PreferencesPage::ANDROID_ALL.contains(&PreferencesPage::VisualizationPlugins));
        assert!(!android_page_shows_reset(PreferencesPage::Fonts));
        assert!(!android_page_shows_reset(PreferencesPage::Skins));
        assert!(android_page_shows_reset(
            PreferencesPage::VisualizationPlugins
        ));
    }

    #[test]
    fn android_skin_discovery_waits_until_the_skins_page_is_visible() {
        let mut state =
            PreferencesViewportState::new(&Config::default(), PreferencesPage::Skins, true);
        assert!(!android_skins_page_visible(&state));

        state.android_show_categories = false;
        assert!(android_skins_page_visible(&state));

        state.selected_page = PreferencesPage::Options;
        assert!(!android_skins_page_visible(&state));
    }

    #[test]
    fn android_back_swipe_requires_a_rightward_edge_gesture() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 800.0));
        assert!(is_android_back_swipe(
            egui::pos2(20.0, 300.0),
            egui::pos2(180.0, 315.0),
            screen,
        ));
        assert!(!is_android_back_swipe(
            egui::pos2(140.0, 300.0),
            egui::pos2(300.0, 300.0),
            screen,
        ));
        assert!(!is_android_back_swipe(
            egui::pos2(20.0, 300.0),
            egui::pos2(40.0, 300.0),
            screen,
        ));
    }
}
