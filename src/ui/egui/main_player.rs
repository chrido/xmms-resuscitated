//! egui main player panel/window.

use crate::app::command::{AudioCommand, PanelCommand, PlayerCommand, PlaylistCommand, UiCommand};
use crate::app::view_model::{
    balance_to_position, main_player_view_model, position_to_balance, position_to_volume,
    volume_to_position, MainPlayerViewModel,
};
use crate::app_log_info;
use crate::config::TimerMode;
use crate::player::PlayerState;
use crate::render::{
    MainPushButton, MainSlider, MainToggleButton, MainWindowRenderState, VisualizationRenderState,
    MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
};
use crate::skin::layout::{
    main_push_button_rect, main_slider_layout, main_toggle_button_rect, SkinRect,
};
use crate::skin::widget::{NumberDisplay, PlayStatusValue};

use super::app::EguiFrontendState;
use super::render_cache::{CachedMainStaticTexture, CachedMainTexture};
use super::skin_texture::{
    pixel_snapped_rect, render_main_player_dynamic_color_image_buffered,
    render_main_player_static_color_image_buffered, upload_color_image,
};
#[cfg(test)]
use super::skin_texture::{
    render_main_player_color_image, render_main_player_dynamic_color_image,
    render_main_player_static_color_image,
};
use super::ui_state::MainPressed;

const MARQUEE_REPAINT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(84);

pub fn main_player_title(view_model: &MainPlayerViewModel) -> &str {
    &view_model.title
}

pub fn show_main_player(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = main_player_view_model(app.controller().state());
    let now = std::time::Instant::now();
    let marquee_elapsed = now.saturating_duration_since(app.last_title_marquee_tick);
    app.last_title_marquee_tick = now;
    app.title_marquee.update(
        &view_model.title,
        crate::render::MAIN_TITLE_TEXT_WIDTH,
        view_model.player_state,
        !view_model.shaded,
        marquee_elapsed,
    );
    if app
        .title_marquee
        .is_scrolling(view_model.player_state, !view_model.shaded)
    {
        ui.ctx().request_repaint_after(MARQUEE_REPAINT_INTERVAL);
    }
    let config = &app.controller().state().config;
    let mut render_state = main_render_state(
        &view_model,
        current_position_ms(app),
        current_duration_ms(app),
        config.equalizer_visible,
        config.playlist_visible,
        app.main_pressed,
        config.timer_mode,
        app.visualization_render_state(),
    );
    render_state.title_offset_px = app.title_marquee.offset_px();
    let needs_static_update = app.render_cache.main_static.as_ref().is_none_or(|cached| {
        cached.generation != app.render_cache.generation
            || cached.focused != render_state.focused
            || cached.shaded != render_state.shaded
    });
    if needs_static_update {
        let Ok(image) = render_main_player_static_color_image_buffered(
            &app.active_skin,
            render_state.focused,
            render_state.shaded,
            &mut app.render_cache.main_static_staging,
        ) else {
            ui.label("failed to render skinned main player");
            return;
        };
        if let Some(cached) = &mut app.render_cache.main_static {
            cached.texture.set(image, egui::TextureOptions::NEAREST);
            cached.generation = app.render_cache.generation;
            cached.focused = render_state.focused;
            cached.shaded = render_state.shaded;
        } else {
            app.render_cache.main_static = Some(CachedMainStaticTexture {
                generation: app.render_cache.generation,
                focused: render_state.focused,
                shaded: render_state.shaded,
                texture: upload_color_image(ui.ctx(), "xmms-main-player-static", image),
            });
        }
    }
    let needs_texture_update = app.render_cache.main.as_ref().is_none_or(|cached| {
        cached.generation != app.render_cache.generation || cached.state != render_state
    });
    if needs_texture_update {
        let Ok(image) = render_main_player_dynamic_color_image_buffered(
            &app.active_skin,
            &render_state,
            &mut app.render_cache.main_staging,
        ) else {
            ui.label("failed to render skinned main player");
            return;
        };
        if let Some(cached) = &mut app.render_cache.main {
            cached.texture.set(image, egui::TextureOptions::NEAREST);
            cached.generation = app.render_cache.generation;
            cached.state = render_state.clone();
        } else {
            app.render_cache.main = Some(CachedMainTexture {
                generation: app.render_cache.generation,
                state: render_state.clone(),
                texture: upload_color_image(ui.ctx(), "xmms-main-player-dynamic", image),
            });
        }
    }
    let static_texture_id = app
        .render_cache
        .main_static
        .as_ref()
        .expect("main static texture initialized")
        .texture
        .id();
    let dynamic_texture_id = app
        .render_cache
        .main
        .as_ref()
        .expect("main texture initialized")
        .texture
        .id();
    let base_height = if view_model.shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        MAIN_WINDOW_HEIGHT
    };
    let size = egui::vec2(
        MAIN_WINDOW_WIDTH as f32 * app.scale_factor,
        base_height as f32 * app.scale_factor,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().image(
        static_texture_id,
        pixel_snapped_rect(ui.ctx(), rect),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    ui.painter().image(
        dynamic_texture_id,
        pixel_snapped_rect(ui.ctx(), rect),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    add_main_titlebar_drag_region(ui, app, rect, &view_model);
    add_main_hit_regions(ui, app, rect, &view_model);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

pub(crate) fn main_render_state(
    view_model: &MainPlayerViewModel,
    playback_position_ms: i64,
    duration_ms: Option<i64>,
    equalizer_visible: bool,
    playlist_visible: bool,
    pressed: MainPressed,
    timer_mode: TimerMode,
    visualization: VisualizationRenderState,
) -> MainWindowRenderState {
    let (pressed_push, pressed_toggle, pressed_slider) = pressed.render_parts();
    let (shaded_time_min, shaded_time_sec) = shaded_time_parts(
        view_model.player_state,
        playback_position_ms,
        duration_ms,
        timer_mode,
    );
    MainWindowRenderState {
        title: if view_model.title.is_empty() {
            "XMMS Renascene".to_string()
        } else {
            view_model.title.clone()
        },
        shaded: view_model.shaded,
        bitrate_text: blank_zero(&view_model.bitrate_text),
        frequency_text: blank_zero(&view_model.frequency_text),
        channels: view_model.channels_text.parse().unwrap_or(0),
        volume_position: volume_to_position(view_model.volume),
        balance_position: balance_to_position(view_model.balance),
        position_position: position_slider_position(playback_position_ms, duration_ms),
        shaded_position_position: shaded_position_slider_position(
            playback_position_ms,
            duration_ms,
        ),
        shaded_position_visible: duration_ms.is_some_and(|duration| duration > 0)
            && view_model.player_state != PlayerState::Stopped,
        shuffle_selected: view_model.shuffle,
        repeat_selected: view_model.repeat,
        equalizer_selected: equalizer_visible,
        playlist_selected: playlist_visible,
        play_status: match view_model.player_state {
            PlayerState::Playing => PlayStatusValue::Playing,
            PlayerState::Paused => PlayStatusValue::Paused,
            PlayerState::Stopped => PlayStatusValue::Stopped,
        },
        pressed_push,
        pressed_toggle,
        pressed_slider,
        time_digits: time_digits(
            view_model.player_state,
            playback_position_ms,
            duration_ms,
            timer_mode,
        ),
        shaded_time_min,
        shaded_time_sec,
        visualization,
        ..MainWindowRenderState::default()
    }
}

fn display_time_ms(position_ms: i64, duration_ms: Option<i64>, timer_mode: TimerMode) -> i64 {
    let elapsed = position_ms.max(0);
    if let (TimerMode::Remaining, Some(duration)) =
        (timer_mode, duration_ms.filter(|duration| *duration > 0))
    {
        return (duration - elapsed).max(0);
    }
    elapsed
}

fn display_seconds(position_ms: i64, duration_ms: Option<i64>, timer_mode: TimerMode) -> i64 {
    let mut seconds = display_time_ms(position_ms, duration_ms, timer_mode) / 1000;
    if seconds > i64::from(99 * 60) {
        seconds /= 60;
    }
    seconds.max(0)
}

fn showing_remaining(duration_ms: Option<i64>, timer_mode: TimerMode) -> bool {
    timer_mode == TimerMode::Remaining && duration_ms.is_some_and(|duration| duration > 0)
}

fn time_digits(
    player_state: PlayerState,
    position_ms: i64,
    duration_ms: Option<i64>,
    timer_mode: TimerMode,
) -> [i32; 5] {
    if player_state == PlayerState::Stopped {
        return [NumberDisplay::BLANK; 5];
    }
    let seconds = display_seconds(position_ms, duration_ms, timer_mode);
    let minutes = seconds / 60;
    [
        if showing_remaining(duration_ms, timer_mode) {
            NumberDisplay::DASH
        } else {
            NumberDisplay::BLANK
        },
        ((minutes / 10) % 10) as i32,
        (minutes % 10) as i32,
        ((seconds % 60) / 10) as i32,
        (seconds % 10) as i32,
    ]
}

fn shaded_time_parts(
    player_state: PlayerState,
    position_ms: i64,
    duration_ms: Option<i64>,
    timer_mode: TimerMode,
) -> (String, String) {
    if player_state == PlayerState::Stopped {
        return ("   ".to_string(), "  ".to_string());
    }
    let seconds = display_seconds(position_ms, duration_ms, timer_mode);
    let prefix = if showing_remaining(duration_ms, timer_mode) {
        '-'
    } else {
        ' '
    };
    (
        format!("{prefix}{:02}", seconds / 60),
        format!("{:02}", seconds % 60),
    )
}

fn current_position_ms(app: &EguiFrontendState) -> i64 {
    app.controller().state().config.playback_position_ms.max(0)
}

fn current_duration_ms(app: &EguiFrontendState) -> Option<i64> {
    let state = app.controller().state();
    state.player.duration_ms().or_else(|| {
        state
            .playlist
            .position()
            .and_then(|index| state.playlist.entries().get(index))
            .map(|entry| entry.length_ms)
    })
}

fn position_slider_position(position_ms: i64, duration_ms: Option<i64>) -> i32 {
    let Some(duration_ms) = duration_ms.filter(|duration| *duration > 0) else {
        return 0;
    };
    let layout = main_slider_layout(MainSlider::Position, false);
    ((position_ms.clamp(0, duration_ms) * i64::from(layout.max)) / duration_ms) as i32
}

fn shaded_position_slider_position(position_ms: i64, duration_ms: Option<i64>) -> i32 {
    let Some(duration_ms) = duration_ms.filter(|duration| *duration > 0) else {
        return 1;
    };
    (((position_ms.clamp(0, duration_ms) * 12) / duration_ms) as i32 + 1).clamp(1, 13)
}

fn blank_zero(text: &str) -> String {
    if text == "0" {
        String::new()
    } else {
        text.to_string()
    }
}

fn add_main_titlebar_drag_region(
    ui: &mut egui::Ui,
    app: &EguiFrontendState,
    base_rect: egui::Rect,
    view_model: &MainPlayerViewModel,
) {
    let titlebar = scale_skin_rect(
        base_rect,
        SkinRect::new(0, 0, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT),
        app.scale_factor,
    );
    let response = ui.interact(
        titlebar,
        ui.id().with("main-titlebar-drag"),
        egui::Sense::click_and_drag(),
    );
    if response.drag_started() {
        let Some(pointer) = response.interact_pointer_pos() else {
            return;
        };
        let x = ((pointer.x - base_rect.left()) / app.scale_factor).floor() as i32;
        let y = ((pointer.y - base_rect.top()) / app.scale_factor).floor() as i32;
        if main_titlebar_drag_excluded(x, y, view_model.shaded) {
            return;
        }
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

fn main_titlebar_drag_excluded(x: i32, y: i32, shaded: bool) -> bool {
    main_push_buttons(shaded)
        .iter()
        .any(|button| main_push_hit_rect(*button, shaded).contains(x, y))
        || main_sliders(shaded)
            .iter()
            .any(|slider| main_slider_layout(*slider, shaded).rect.contains(x, y))
}

fn main_push_hit_rect(button: MainPushButton, shaded: bool) -> SkinRect {
    main_push_hit_rect_for(button, shaded, cfg!(target_os = "android"))
}

fn main_push_hit_rect_for(button: MainPushButton, shaded: bool, touch_friendly: bool) -> SkinRect {
    if touch_friendly && button == MainPushButton::Menu {
        SkinRect::new(0, 0, 36, 36)
    } else {
        main_push_button_rect(button, shaded)
    }
}

fn add_main_hit_regions(
    ui: &mut egui::Ui,
    app: &mut EguiFrontendState,
    base_rect: egui::Rect,
    view_model: &MainPlayerViewModel,
) {
    app.main_pressed = MainPressed::None;

    for &button in main_push_buttons(view_model.shaded) {
        let rect = scale_skin_rect(
            base_rect,
            main_push_hit_rect(button, view_model.shaded),
            app.scale_factor,
        );
        let response = ui.interact(
            rect,
            ui.id().with(("main-push", button as u8)),
            egui::Sense::click(),
        );
        if response.is_pointer_button_down_on() {
            app.main_pressed = MainPressed::Push(button);
            ui.ctx().request_repaint();
        }
        if response.clicked() {
            app.close_player_menus();
            dispatch_push(ui.ctx(), app, button);
        }
    }

    if !view_model.shaded {
        for toggle in [
            MainToggleButton::Shuffle,
            MainToggleButton::Repeat,
            MainToggleButton::Equalizer,
            MainToggleButton::Playlist,
        ] {
            let rect =
                scale_skin_rect(base_rect, main_toggle_button_rect(toggle), app.scale_factor);
            let response = ui.interact(
                rect,
                ui.id().with(("main-toggle", toggle as u8)),
                egui::Sense::click(),
            );
            if response.is_pointer_button_down_on() {
                app.main_pressed = MainPressed::Toggle(toggle);
                ui.ctx().request_repaint();
            }
            if response.clicked() {
                app.close_player_menus();
                dispatch_toggle(app, toggle);
            }
        }
    }

    for &slider in main_sliders(view_model.shaded) {
        let layout = main_slider_layout(slider, view_model.shaded);
        let rect = scale_skin_rect(base_rect, layout.rect, app.scale_factor);
        let response = ui.interact(
            rect,
            ui.id().with(("main-slider", slider as u8)),
            egui::Sense::click_and_drag(),
        );
        if response.is_pointer_button_down_on() || response.dragged() {
            app.main_pressed = MainPressed::Slider(slider);
            ui.ctx().request_repaint();
        }
        if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
            let pointer = response.interact_pointer_pos().unwrap();
            let normalized = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let position =
                layout.min + ((layout.max - layout.min) as f32 * normalized).round() as i32;
            let position_changed = response.clicked()
                || slider_drag_position_changed(
                    &mut app.main_slider_drag_position,
                    slider,
                    position,
                );
            if position_changed {
                app.close_player_menus();
                dispatch_slider(app, slider, position, view_model.shaded);
            }
        }
        if response.drag_stopped() {
            app.main_slider_drag_position = None;
        }
    }
}

fn main_push_buttons(shaded: bool) -> &'static [MainPushButton] {
    if shaded {
        &[
            MainPushButton::Menu,
            MainPushButton::Minimize,
            MainPushButton::Shade,
            MainPushButton::Close,
            MainPushButton::Previous,
            MainPushButton::Play,
            MainPushButton::Pause,
            MainPushButton::Stop,
            MainPushButton::Next,
            MainPushButton::Eject,
        ]
    } else {
        &[
            MainPushButton::Menu,
            MainPushButton::Minimize,
            MainPushButton::Shade,
            MainPushButton::Close,
            MainPushButton::Previous,
            MainPushButton::Play,
            MainPushButton::Pause,
            MainPushButton::Stop,
            MainPushButton::Next,
            MainPushButton::Eject,
        ]
    }
}

fn main_sliders(shaded: bool) -> &'static [MainSlider] {
    if shaded {
        &[MainSlider::Position]
    } else {
        &[
            MainSlider::Volume,
            MainSlider::Balance,
            MainSlider::Position,
        ]
    }
}

fn scale_skin_rect(base: egui::Rect, rect: SkinRect, scale: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(
            base.left() + rect.x as f32 * scale,
            base.top() + rect.y as f32 * scale,
        ),
        egui::vec2(rect.width as f32 * scale, rect.height as f32 * scale),
    )
}

fn dispatch_push(ctx: &egui::Context, app: &mut EguiFrontendState, button: MainPushButton) {
    let button_name = format!("{button:?}");
    app_log_info!(player, "button activated", button_name);
    match button {
        MainPushButton::Previous => app.dispatch(PlayerCommand::PreviousTrack),
        MainPushButton::Play => app.dispatch(PlayerCommand::Play),
        MainPushButton::Pause => app.dispatch(PlayerCommand::Pause),
        MainPushButton::Stop => app.dispatch(PlayerCommand::Halt),
        MainPushButton::Next => app.dispatch(PlayerCommand::NextTrack),
        MainPushButton::Eject => app.apply_effect(crate::app::effect::AppEffect::OpenFileDialog(
            crate::app::effect::FileDialogRequest::AddAudioFiles,
        )),
        MainPushButton::Shade => app.dispatch(PanelCommand::ToggleMainShade),
        MainPushButton::Menu => {
            #[cfg(target_os = "android")]
            app.dispatch(UiCommand::SetPreferencesVisible(true));
            #[cfg(not(target_os = "android"))]
            app.dispatch(UiCommand::SetMainMenuVisible(true));
        }
        MainPushButton::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
        MainPushButton::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
    }
}

fn dispatch_toggle(app: &mut EguiFrontendState, toggle: MainToggleButton) {
    let toggle_name = format!("{toggle:?}");
    app_log_info!(player, "toggle activated", toggle_name);
    match toggle {
        MainToggleButton::Shuffle => app.dispatch(PlaylistCommand::ToggleShuffle),
        MainToggleButton::Repeat => app.dispatch(PlaylistCommand::ToggleRepeat),
        MainToggleButton::Equalizer => app.dispatch(PanelCommand::ToggleEqualizerVisibility),
        MainToggleButton::Playlist => app.dispatch(PanelCommand::TogglePlaylistVisibility),
    }
}

fn slider_drag_position_changed(
    previous: &mut Option<(MainSlider, i32)>,
    slider: MainSlider,
    position: i32,
) -> bool {
    let current = (slider, position);
    if *previous == Some(current) {
        return false;
    }
    *previous = Some(current);
    true
}

fn dispatch_slider(app: &mut EguiFrontendState, slider: MainSlider, position: i32, shaded: bool) {
    let slider_name = format!("{slider:?}");
    app_log_info!(player, "slider changed", slider_name, position);
    match slider {
        MainSlider::Volume => app.dispatch(AudioCommand::SetVolume(position_to_volume(position))),
        MainSlider::Balance => {
            app.dispatch(AudioCommand::SetBalance(position_to_balance(position)))
        }
        MainSlider::Position => {
            if let Some(position_ms) = position_to_seek_ms(app, position, shaded) {
                app.dispatch(PlayerCommand::SeekToMs(position_ms));
            }
        }
    }
}

fn position_to_seek_ms(app: &EguiFrontendState, position: i32, shaded: bool) -> Option<i64> {
    let duration_ms = current_duration_ms(app)?;
    if duration_ms <= 0 {
        return None;
    }
    let position_ms = if shaded {
        (duration_ms * i64::from((position - 1).clamp(0, 12))) / 12
    } else {
        let layout = main_slider_layout(MainSlider::Position, false);
        (duration_ms * i64::from(position.clamp(layout.min, layout.max))) / i64::from(layout.max)
    };
    Some(position_ms.clamp(0, duration_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::AppCommand;

    #[test]
    fn main_player_commands_use_hierarchical_domains() {
        assert_eq!(
            AppCommand::from(PlayerCommand::Play),
            AppCommand::Player(PlayerCommand::Play)
        );
        assert_eq!(
            AppCommand::from(AudioCommand::SetVolume(50)),
            AppCommand::Audio(AudioCommand::SetVolume(50))
        );
        assert_eq!(
            AppCommand::from(PanelCommand::TogglePlaylistVisibility),
            AppCommand::Panel(PanelCommand::TogglePlaylistVisibility)
        );
    }

    #[test]
    fn android_menu_uses_larger_touch_target() {
        assert_eq!(
            main_push_hit_rect_for(MainPushButton::Menu, false, true),
            SkinRect::new(0, 0, 36, 36)
        );
        assert_eq!(
            main_push_hit_rect_for(MainPushButton::Menu, false, false),
            main_push_button_rect(MainPushButton::Menu, false)
        );
        assert_eq!(
            main_push_hit_rect_for(MainPushButton::Play, false, true),
            main_push_button_rect(MainPushButton::Play, false)
        );
    }

    #[test]
    fn main_render_state_uses_skin_slider_positions() {
        let state = main_render_state(
            &MainPlayerViewModel {
                title: String::new(),
                player_state: PlayerState::Stopped,
                volume: 100,
                balance: 0,
                shuffle: false,
                repeat: false,
                shaded: false,
                bitrate_text: "0".to_string(),
                frequency_text: "0".to_string(),
                channels_text: "0".to_string(),
            },
            60_000,
            Some(120_000),
            true,
            true,
            MainPressed::Slider(MainSlider::Volume),
            TimerMode::Elapsed,
            VisualizationRenderState::default(),
        );

        assert_eq!(state.volume_position, 51);
        assert_eq!(state.balance_position, 12);
        assert_eq!(state.position_position, 109);
        assert_eq!(state.title, "XMMS Renascene");
        assert!(state.bitrate_text.is_empty());
        assert!(state.equalizer_selected);
        assert!(state.playlist_selected);
        assert_eq!(state.pressed_push, None);
        assert_eq!(state.pressed_toggle, None);
        assert_eq!(state.pressed_slider, Some(MainSlider::Volume));
    }

    #[test]
    fn main_render_state_keeps_visualization_data() {
        let mut visualization = VisualizationRenderState::default();
        visualization.data[7] = 0.85;
        visualization.peak[7] = 0.95;

        let state = main_render_state(
            &MainPlayerViewModel {
                title: "Song".to_string(),
                player_state: PlayerState::Playing,
                volume: 100,
                balance: 0,
                shuffle: false,
                repeat: false,
                shaded: false,
                bitrate_text: "128".to_string(),
                frequency_text: "44".to_string(),
                channels_text: "2".to_string(),
            },
            0,
            None,
            false,
            false,
            MainPressed::None,
            TimerMode::Elapsed,
            visualization.clone(),
        );

        assert_eq!(state.visualization, visualization);
    }

    #[test]
    fn main_render_state_formats_elapsed_and_remaining_time() {
        let view_model = MainPlayerViewModel {
            title: "Song".to_string(),
            player_state: PlayerState::Playing,
            volume: 100,
            balance: 0,
            shuffle: false,
            repeat: false,
            shaded: false,
            bitrate_text: "128".to_string(),
            frequency_text: "44".to_string(),
            channels_text: "2".to_string(),
        };
        let state = main_render_state(
            &view_model,
            65_000,
            Some(130_000),
            false,
            false,
            MainPressed::None,
            TimerMode::Elapsed,
            VisualizationRenderState::default(),
        );
        assert_eq!(state.time_digits, [NumberDisplay::BLANK, 0, 1, 0, 5]);
        assert_eq!(
            (state.shaded_time_min, state.shaded_time_sec),
            (" 01".to_string(), "05".to_string())
        );

        let state = main_render_state(
            &view_model,
            65_000,
            Some(130_000),
            false,
            false,
            MainPressed::None,
            TimerMode::Remaining,
            VisualizationRenderState::default(),
        );
        assert_eq!(state.time_digits, [NumberDisplay::DASH, 0, 1, 0, 5]);
        assert_eq!(
            (state.shaded_time_min, state.shaded_time_sec),
            ("-01".to_string(), "05".to_string())
        );
    }

    #[test]
    fn main_render_state_blanks_time_when_stopped() {
        let state = main_render_state(
            &MainPlayerViewModel {
                title: "Song".to_string(),
                player_state: PlayerState::Stopped,
                volume: 100,
                balance: 0,
                shuffle: false,
                repeat: false,
                shaded: false,
                bitrate_text: "128".to_string(),
                frequency_text: "44".to_string(),
                channels_text: "2".to_string(),
            },
            65_000,
            Some(130_000),
            false,
            false,
            MainPressed::None,
            TimerMode::Elapsed,
            VisualizationRenderState::default(),
        );
        assert_eq!(state.time_digits, [NumberDisplay::BLANK; 5]);
        assert_eq!(
            (state.shaded_time_min, state.shaded_time_sec),
            ("   ".to_string(), "  ".to_string())
        );
    }

    #[test]
    fn rendered_main_player_includes_visualizer_pixels() {
        let skin = crate::skin::DefaultSkin::load_bundled().unwrap();
        let blank_state = MainWindowRenderState::default();
        let mut active_state = blank_state.clone();
        active_state.visualization.data[5] = 1.0;
        active_state.visualization.peak[5] = 1.0;

        let blank = render_main_player_color_image(&skin, &blank_state).unwrap();
        let active = render_main_player_color_image(&skin, &active_state).unwrap();
        let width = MAIN_WINDOW_WIDTH as usize;
        let changed_visualizer_pixels = (43usize..59)
            .flat_map(|y| (24usize..100).map(move |x| (x, y)))
            .filter(|(x, y)| blank.pixels[y * width + x] != active.pixels[y * width + x])
            .count();

        assert!(changed_visualizer_pixels > 0);
    }

    #[test]
    fn visualizer_updates_leave_static_main_layer_unchanged() {
        let skin = crate::skin::DefaultSkin::load_bundled().unwrap();
        let blank_state = MainWindowRenderState::default();
        let mut active_state = blank_state.clone();
        active_state.visualization.data[5] = 1.0;

        let static_before =
            render_main_player_static_color_image(&skin, blank_state.focused, blank_state.shaded)
                .unwrap();
        let static_after =
            render_main_player_static_color_image(&skin, active_state.focused, active_state.shaded)
                .unwrap();
        let dynamic_before = render_main_player_dynamic_color_image(&skin, &blank_state).unwrap();
        let dynamic_after = render_main_player_dynamic_color_image(&skin, &active_state).unwrap();

        assert_eq!(static_before, static_after);
        assert_ne!(dynamic_before, dynamic_after);
    }

    #[test]
    fn position_slider_maps_to_current_track_duration() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        app.controller_mut().state_mut().playlist.add_timed_uri(
            "file:///tmp/song.ogg",
            "Song",
            219_000,
        );
        app.controller_mut().state_mut().playlist.set_position(0);

        assert_eq!(position_to_seek_ms(&app, 109, false), Some(109_000));
        assert_eq!(position_to_seek_ms(&app, 7, true), Some(109_500));
    }

    #[test]
    fn held_slider_position_is_dispatched_only_once_until_it_changes() {
        let mut previous = None;

        assert!(slider_drag_position_changed(
            &mut previous,
            MainSlider::Position,
            24,
        ));
        assert!(!slider_drag_position_changed(
            &mut previous,
            MainSlider::Position,
            24,
        ));
        assert!(slider_drag_position_changed(
            &mut previous,
            MainSlider::Position,
            25,
        ));
    }

    #[test]
    fn skinned_main_controls_dispatch_to_app_state() {
        let mut app =
            EguiFrontendState::new(crate::app::preview::PreviewOptions::default()).unwrap();
        let ctx = egui::Context::default();

        dispatch_push(&ctx, &mut app, MainPushButton::Shade);
        assert!(app.controller().state().config.main_shaded);

        dispatch_toggle(&mut app, MainToggleButton::Playlist);
        assert!(app.controller().state().config.playlist_visible);

        dispatch_toggle(&mut app, MainToggleButton::Shuffle);
        assert!(app.controller().state().playlist.shuffle());

        dispatch_slider(&mut app, MainSlider::Volume, 0, false);
        assert_eq!(app.controller().state().player.volume(), 0);

        dispatch_slider(&mut app, MainSlider::Balance, 24, false);
        assert_eq!(app.controller().state().player.balance(), 100);
    }
}
