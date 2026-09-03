use super::Context;

use super::core::{
    blit_skin_rect, blit_surface_rect, render_horizontal_slider, render_sprite_spec, render_text,
    render_text_offset, set_rgb, surface_from_xpm, RenderError, SliderRenderSpec,
};
use crate::audio_model::{SpectrumData, SPECTRUM_BANDS};
use crate::skin::layout::{
    main_push_button_spec, main_slider_layout, main_toggle_button_spec, MainPushButton, MainSlider,
    MainToggleButton, SkinRect, MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
};
use crate::skin::widget::{
    NumberDisplay, PlayStatusValue, VisAnalyzerMode, VisAnalyzerStyle, VisMode, VisScopeMode,
    VisVuMode,
};
use crate::skin::{DefaultSkin, SkinPixmapKind};

pub const MAIN_TITLE_TEXT_WIDTH: i32 = 153;

pub fn render_main_titlebar(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
) -> Result<bool, RenderError> {
    let Some(titlebar) = skin.get(SkinPixmapKind::Titlebar) else {
        return Ok(false);
    };
    let titlebar = surface_from_xpm(titlebar)?;
    let ysrc = match (shaded, focused) {
        (true, true) => 29,
        (true, false) => 42,
        (false, true) => 0,
        (false, false) => 15,
    };

    blit_surface_rect(
        cr,
        &titlebar,
        SkinRect::new(27, ysrc, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT),
        (0, 0),
    )
}

pub fn render_main_player(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
) -> Result<bool, RenderError> {
    let mut rendered = false;
    if !shaded {
        if let Some(main) = skin.get(SkinPixmapKind::Main) {
            let main = surface_from_xpm(main)?;
            rendered |= blit_surface_rect(
                cr,
                &main,
                SkinRect::new(0, 0, MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT),
                (0, 0),
            )?;
        }
    }

    rendered |= render_main_titlebar(cr, skin, focused, shaded)?;
    Ok(rendered)
}

pub fn render_transport_buttons(
    cr: &Context,
    skin: &DefaultSkin,
    pressed: Option<MainPushButton>,
) -> Result<bool, RenderError> {
    let mut rendered = false;
    let mut x = 0;
    for button in [
        MainPushButton::Previous,
        MainPushButton::Play,
        MainPushButton::Pause,
        MainPushButton::Stop,
        MainPushButton::Next,
    ] {
        let mut spec = main_push_button_spec(button, pressed == Some(button));
        spec.dest = SkinRect::new(x, 0, spec.source.width, spec.source.height);
        rendered |= render_sprite_spec(cr, skin, spec)?;
        x += spec.source.width;
    }
    Ok(rendered)
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualizationRenderState {
    pub mode: VisMode,
    pub analyzer_style: VisAnalyzerStyle,
    pub analyzer_mode: VisAnalyzerMode,
    pub scope_mode: VisScopeMode,
    pub peaks_enabled: bool,
    pub vu_mode: VisVuMode,
    pub data: SpectrumData,
    pub peak: SpectrumData,
    pub milkdrop_energy: f32,
    pub milkdrop_phase: f32,
}

impl Default for VisualizationRenderState {
    fn default() -> Self {
        Self {
            mode: VisMode::Analyzer,
            analyzer_style: VisAnalyzerStyle::Bars,
            analyzer_mode: VisAnalyzerMode::Normal,
            scope_mode: VisScopeMode::Line,
            peaks_enabled: true,
            vu_mode: VisVuMode::Segmented,
            data: [0.0; SPECTRUM_BANDS],
            peak: [0.0; SPECTRUM_BANDS],
            milkdrop_energy: 0.0,
            milkdrop_phase: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MainWindowRenderState {
    pub focused: bool,
    pub shaded: bool,
    pub title: String,
    pub title_offset_px: i32,
    pub bitrate_text: String,
    pub frequency_text: String,
    pub time_digits: [i32; 5],
    pub shaded_time_min: String,
    pub shaded_time_sec: String,
    pub volume_position: i32,
    pub balance_position: i32,
    pub position_position: i32,
    pub shaded_position_position: i32,
    pub shaded_position_visible: bool,
    pub shuffle_selected: bool,
    pub repeat_selected: bool,
    pub equalizer_selected: bool,
    pub playlist_selected: bool,
    pub pressed_push: Option<MainPushButton>,
    pub pressed_toggle: Option<MainToggleButton>,
    pub pressed_slider: Option<MainSlider>,
    pub play_status: PlayStatusValue,
    pub channels: i32,
    pub visualization: VisualizationRenderState,
}

impl Default for MainWindowRenderState {
    fn default() -> Self {
        Self {
            focused: true,
            shaded: false,
            title: "XMMS Renascene".to_string(),
            title_offset_px: 0,
            bitrate_text: String::new(),
            frequency_text: String::new(),
            time_digits: [NumberDisplay::BLANK; 5],
            shaded_time_min: "   ".to_string(),
            shaded_time_sec: "  ".to_string(),
            volume_position: 51,
            balance_position: 12,
            position_position: 0,
            shaded_position_position: 1,
            shaded_position_visible: false,
            shuffle_selected: false,
            repeat_selected: false,
            equalizer_selected: false,
            playlist_selected: false,
            pressed_push: None,
            pressed_toggle: None,
            pressed_slider: None,
            play_status: PlayStatusValue::Stopped,
            channels: 0,
            visualization: VisualizationRenderState::default(),
        }
    }
}

pub fn render_main_player_reset(cr: &Context, skin: &DefaultSkin) -> Result<bool, RenderError> {
    render_main_player_state(cr, skin, &MainWindowRenderState::default())
}

pub fn render_main_player_state(
    cr: &Context,
    skin: &DefaultSkin,
    state: &MainWindowRenderState,
) -> Result<bool, RenderError> {
    let mut rendered = render_main_player(cr, skin, state.focused, state.shaded)?;
    rendered |= render_main_player_dynamic_state(cr, skin, state)?;
    Ok(rendered)
}

pub fn render_main_player_dynamic_state(
    cr: &Context,
    skin: &DefaultSkin,
    state: &MainWindowRenderState,
) -> Result<bool, RenderError> {
    let mut rendered = false;
    for button in [
        MainPushButton::Menu,
        MainPushButton::Minimize,
        MainPushButton::Shade,
        MainPushButton::Close,
    ] {
        rendered |= render_sprite_spec(
            cr,
            skin,
            main_push_button_spec(button, state.pressed_push == Some(button)),
        )?;
    }

    if state.shaded {
        render_windowshade_visualization(cr, skin, 79, 5, &state.visualization)?;
        render_text(cr, skin, &state.shaded_time_min, 130, 4, 15)?;
        render_text(cr, skin, &state.shaded_time_sec, 147, 4, 10)?;
        if state.shaded_position_visible {
            render_shaded_position_slider(cr, skin, state)?;
        }
        return Ok(rendered);
    }

    for button in [
        MainPushButton::Previous,
        MainPushButton::Play,
        MainPushButton::Pause,
        MainPushButton::Stop,
        MainPushButton::Next,
        MainPushButton::Eject,
    ] {
        rendered |= render_sprite_spec(
            cr,
            skin,
            main_push_button_spec(button, state.pressed_push == Some(button)),
        )?;
    }

    for toggle in [
        MainToggleButton::Shuffle,
        MainToggleButton::Repeat,
        MainToggleButton::Equalizer,
        MainToggleButton::Playlist,
    ] {
        let selected = match toggle {
            MainToggleButton::Shuffle => state.shuffle_selected,
            MainToggleButton::Repeat => state.repeat_selected,
            MainToggleButton::Equalizer => state.equalizer_selected,
            MainToggleButton::Playlist => state.playlist_selected,
        };
        rendered |= render_sprite_spec(
            cr,
            skin,
            main_toggle_button_spec(toggle, selected, state.pressed_toggle == Some(toggle)),
        )?;
    }

    render_text_offset(
        cr,
        skin,
        &state.title,
        111,
        27,
        MAIN_TITLE_TEXT_WIDTH,
        state.title_offset_px,
    )?;
    render_text(cr, skin, &state.bitrate_text, 111, 43, 15)?;
    render_text(cr, skin, &state.frequency_text, 156, 43, 10)?;

    let volume_slider = main_slider_layout(MainSlider::Volume, false);
    rendered |= render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::Volume,
            x: volume_slider.rect.x,
            y: volume_slider.rect.y,
            width: volume_slider.rect.width,
            height: volume_slider.rect.height,
            position: state
                .volume_position
                .clamp(volume_slider.min, volume_slider.max),
            knob_source_x: if state.pressed_slider == Some(MainSlider::Volume) {
                0
            } else {
                15
            },
            knob_source_y: 422,
            knob_width: volume_slider.knob_size.width,
            knob_height: volume_slider.knob_size.height,
            frame_height: volume_slider.frame_height,
            frame_offset: volume_slider.frame_offset,
            frame: ((state
                .volume_position
                .clamp(volume_slider.min, volume_slider.max) as f64
                / volume_slider.max as f64)
                * 27.0) as i32,
        },
    )?;
    let balance_slider = main_slider_layout(MainSlider::Balance, false);
    rendered |= render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::Balance,
            x: balance_slider.rect.x,
            y: balance_slider.rect.y,
            width: balance_slider.rect.width,
            height: balance_slider.rect.height,
            position: state
                .balance_position
                .clamp(balance_slider.min, balance_slider.max),
            knob_source_x: if state.pressed_slider == Some(MainSlider::Balance) {
                0
            } else {
                15
            },
            knob_source_y: 422,
            knob_width: balance_slider.knob_size.width,
            knob_height: balance_slider.knob_size.height,
            frame_height: balance_slider.frame_height,
            frame_offset: balance_slider.frame_offset,
            frame: ((state
                .balance_position
                .clamp(balance_slider.min, balance_slider.max) as f64
                / balance_slider.max as f64)
                * 27.0) as i32,
        },
    )?;
    let position_slider = main_slider_layout(MainSlider::Position, false);
    rendered |= render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::PosBar,
            x: position_slider.rect.x,
            y: position_slider.rect.y,
            width: position_slider.rect.width,
            height: position_slider.rect.height,
            position: state
                .position_position
                .clamp(position_slider.min, position_slider.max),
            knob_source_x: if state.pressed_slider == Some(MainSlider::Position) {
                278
            } else {
                248
            },
            knob_source_y: 0,
            knob_width: position_slider.knob_size.width,
            knob_height: position_slider.knob_size.height,
            frame_height: position_slider.frame_height,
            frame_offset: position_slider.frame_offset,
            frame: 0,
        },
    )?;

    for (value, x) in state.time_digits.iter().zip([36, 48, 60, 78, 90]) {
        rendered |= render_number(cr, skin, *value, x, 26)?;
    }

    render_visualization(cr, skin, 24, 43, 76, &state.visualization)?;
    if state.shaded {
        render_windowshade_visualization(cr, skin, 79, 5, &state.visualization)?;
    }
    rendered |= render_mono_stereo(cr, skin, state.channels, 212, 41)?;
    let status_y = match state.play_status {
        PlayStatusValue::Playing => 0,
        PlayStatusValue::Paused => 9,
        PlayStatusValue::Stopped => 18,
    };
    rendered |= blit_skin_rect(
        cr,
        skin,
        SkinPixmapKind::PlayPause,
        SkinRect::new(0, status_y, 11, 9),
        (24, 28),
    )?;

    Ok(rendered)
}

fn render_shaded_position_slider(
    cr: &Context,
    skin: &DefaultSkin,
    state: &MainWindowRenderState,
) -> Result<bool, RenderError> {
    let layout = main_slider_layout(MainSlider::Position, true);
    let position = state.shaded_position_position.clamp(layout.min, layout.max);
    let knob_source_x = match position {
        1..=5 => 17,
        6..=8 => 20,
        _ => 23,
    };
    blit_skin_rect(
        cr,
        skin,
        SkinPixmapKind::Titlebar,
        SkinRect::new(
            knob_source_x,
            36,
            layout.knob_size.width,
            layout.knob_size.height,
        ),
        (layout.rect.x + position, layout.rect.y),
    )
}

fn render_number(
    cr: &Context,
    skin: &DefaultSkin,
    value: i32,
    xdest: i32,
    ydest: i32,
) -> Result<bool, RenderError> {
    let digit = if (0..=NumberDisplay::DASH).contains(&value) {
        value
    } else {
        NumberDisplay::DASH
    };
    blit_skin_rect(
        cr,
        skin,
        SkinPixmapKind::Numbers,
        SkinRect::new(
            digit * NumberDisplay::WIDTH,
            0,
            NumberDisplay::WIDTH,
            NumberDisplay::HEIGHT,
        ),
        (xdest, ydest),
    )
}

fn render_visualization_reset(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
) -> Result<(), RenderError> {
    let colors = skin.vis_colors();
    let bg = colors[0];
    cr.save()?;
    cr.rectangle(f64::from(xdest), f64::from(ydest), f64::from(width), 16.0);
    cr.clip();
    cr.set_source_rgb(
        f64::from(bg[0]) / 255.0,
        f64::from(bg[1]) / 255.0,
        f64::from(bg[2]) / 255.0,
    );
    cr.paint()?;
    let dot = colors[1];
    cr.set_source_rgb(
        f64::from(dot[0]) / 255.0,
        f64::from(dot[1]) / 255.0,
        f64::from(dot[2]) / 255.0,
    );
    for y in (1..16).step_by(2) {
        for x in (0..width.min(76)).step_by(2) {
            cr.rectangle(f64::from(xdest + x), f64::from(ydest + y), 1.0, 1.0);
        }
    }
    cr.fill()?;
    cr.restore()?;
    Ok(())
}

pub fn render_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    render_visualization_reset(cr, skin, xdest, ydest, width)?;
    if state.mode == VisMode::Off {
        return Ok(());
    }

    let mut levels = [0; SPECTRUM_BANDS];
    for (index, value) in state.data.iter().enumerate() {
        levels[index] = visualization_level(*value);
    }

    match state.mode {
        VisMode::Scope => render_scope_visualization(cr, skin, xdest, ydest, width, &levels, state),
        VisMode::Milkdrop => render_milkdrop_visualization(cr, skin, xdest, ydest, width, state),
        VisMode::Analyzer => {
            render_analyzer_visualization(cr, skin, xdest, ydest, width, &levels, state)
        }
        VisMode::Off => Ok(()),
    }
}

pub fn render_windowshade_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    let colors = skin.vis_colors();
    cr.save()?;
    cr.rectangle(f64::from(xdest), f64::from(ydest), 38.0, 5.0);
    cr.clip();
    // Like XMMS, initialize every pixel in the 38x5 meter to viscolor[0]
    // before drawing the active pixels on top.
    set_vis_color(cr, colors, 0);
    cr.rectangle(f64::from(xdest), f64::from(ydest), 38.0, 5.0);
    cr.fill()?;

    if state.mode == VisMode::Off {
        cr.restore()?;
        return Ok(());
    }

    if state.mode == VisMode::Scope {
        const SCOPE_COLORS: [usize; 5] = [20, 19, 18, 19, 20];
        for sx in 0..38 {
            let h = (visualization_level(state.data[(sx * 2) as usize]) / 3).clamp(0, 4);
            set_vis_color(cr, colors, SCOPE_COLORS[h as usize]);
            cr.rectangle(f64::from(xdest + sx), f64::from(ydest + 4 - h), 1.0, 1.0);
            cr.fill()?;
        }
        cr.restore()?;
        return Ok(());
    }

    // This is the logical-pixel layout from XMMS's non-scaled renderer. The
    // completed surface is scaled by the frontend, so applying XMMS's separate
    // double-size rasterization here would shift an extra segment into view.
    const SEGMENT_COLORS: [usize; 7] = [17, 17, 17, 12, 12, 12, 2];
    for row in 0..2 {
        let level = (state.data[row].clamp(0.0, 1.0) * 37.0 + 0.5).clamp(0.0, 37.0) as i32;
        if state.vu_mode == VisVuMode::Smooth {
            for sx in 0..level.min(38) {
                set_vis_color(cr, colors, 17 - ((sx * 15) / 37) as usize);
                cr.rectangle(
                    f64::from(xdest + sx),
                    f64::from(ydest + row as i32 * 3),
                    1.0,
                    1.0,
                );
                cr.rectangle(
                    f64::from(xdest + sx),
                    f64::from(ydest + row as i32 * 3 + 1),
                    1.0,
                    1.0,
                );
                cr.fill()?;
            }
        } else {
            let segments = ((level * 7) / 37).clamp(0, 7);
            for sx in 0..segments {
                set_vis_color(cr, colors, SEGMENT_COLORS[sx as usize]);
                cr.rectangle(
                    f64::from(xdest + sx * 5),
                    f64::from(ydest + row as i32 * 3),
                    3.0,
                    2.0,
                );
                cr.fill()?;
            }
        }
    }

    cr.restore()?;
    Ok(())
}

fn render_scope_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
    levels: &[i32; SPECTRUM_BANDS],
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    const SCOPE_COLORS: [usize; 13] = [21, 21, 20, 20, 19, 19, 18, 19, 19, 20, 20, 21, 21];
    let colors = skin.vis_colors();
    for x in 0..(SPECTRUM_BANDS as i32).min(width) {
        let h = levels[x as usize].clamp(0, 15);
        match state.scope_mode {
            VisScopeMode::Dot => draw_vis_pixel(
                cr,
                colors,
                SkinRect::new(xdest, ydest, width, 16),
                (x, 15 - h),
                SCOPE_COLORS[h.clamp(0, 12) as usize],
            )?,
            VisScopeMode::Line => {
                let y1 = 15 - h;
                let y2 = if x < 74 && x + 1 < width {
                    15 - levels[(x + 1) as usize].clamp(0, 15)
                } else {
                    y1
                };
                for y in y1.min(y2)..=y1.max(y2) {
                    draw_vis_pixel(
                        cr,
                        colors,
                        SkinRect::new(xdest, ydest, width, 16),
                        (x, y),
                        SCOPE_COLORS[(y - 3).clamp(0, 12) as usize],
                    )?;
                }
            }
            VisScopeMode::Solid => {
                let y1 = 15 - h;
                let color = SCOPE_COLORS[h.clamp(0, 12) as usize];
                for y in y1.min(9)..=y1.max(9) {
                    draw_vis_pixel(
                        cr,
                        colors,
                        SkinRect::new(xdest, ydest, width, 16),
                        (x, y),
                        color,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn render_analyzer_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
    levels: &[i32; SPECTRUM_BANDS],
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    let colors = skin.vis_colors();
    for x in 0..(SPECTRUM_BANDS as i32).min(width) {
        let h = if state.analyzer_style == VisAnalyzerStyle::Bars {
            if x % 4 == 3 {
                continue;
            }
            levels[(x >> 2) as usize]
        } else {
            levels[x as usize]
        }
        .clamp(0, 15);

        if h > 0 {
            for y in 16 - h..16 {
                draw_vis_pixel(
                    cr,
                    colors,
                    SkinRect::new(xdest, ydest, width, 16),
                    (x, y),
                    analyzer_color(state.analyzer_mode, y, h),
                )?;
            }
        }

        if state.peaks_enabled {
            let peak_index = if state.analyzer_style == VisAnalyzerStyle::Bars {
                (x >> 2) as usize
            } else {
                x as usize
            };
            let peak_y = 16 - visualization_level(state.peak[peak_index]).clamp(0, 15);
            if (0..16).contains(&peak_y) {
                draw_vis_pixel(
                    cr,
                    colors,
                    SkinRect::new(xdest, ydest, width, 16),
                    (x, peak_y),
                    23,
                )?;
            }
        }
    }
    Ok(())
}

fn render_milkdrop_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    let colors = skin.vis_colors();
    let draw_width = width.min(76);
    let cx = (width - 1) as f32 * 0.5;
    let cy = 7.5_f32;
    let phase = state.milkdrop_phase;
    let energy = state.milkdrop_energy.clamp(0.0, 1.0);
    let rot = phase * (0.35 + energy * 0.25);
    let (s, c) = rot.sin_cos();

    for y in 0..16 {
        for x in 0..draw_width {
            let nx = (x as f32 - cx) / cx.max(1.0);
            let ny = (y as f32 - cy) / 8.0;
            let rx = nx * c - ny * s;
            let ry = nx * s + ny * c;
            let r = (rx * rx + ry * ry).sqrt();
            let angle = ry.atan2(rx);
            let warp =
                (angle * 5.0 + phase * 1.7).sin() * 0.18 + (r * 12.0 - phase * 2.4).sin() * 0.16;
            let tunnel = ((r + warp) * 18.0 - phase * 3.0).sin();
            let plasma = ((rx - ry) * 7.0 + phase).sin() + ((rx + ry) * 5.0 - phase * 1.3).cos();
            let color =
                (3.0 + (tunnel + plasma + 4.0 + energy * 2.0) * 2.6).clamp(2.0, 22.0) as usize;
            draw_vis_pixel(
                cr,
                colors,
                SkinRect::new(xdest, ydest, width, 16),
                (x, y),
                color,
            )?;
        }
    }

    for x in 0..width.min(SPECTRUM_BANDS as i32) {
        let sample = state.data[x as usize];
        let y = (7.5 + (x as f32 * 0.19 + phase * 2.0).sin() * 3.0 - sample * 6.0).clamp(0.0, 15.0)
            as i32;
        draw_vis_pixel(
            cr,
            colors,
            SkinRect::new(xdest, ydest, width, 16),
            (x, y),
            23,
        )?;
        if y + 1 < 16 {
            draw_vis_pixel(
                cr,
                colors,
                SkinRect::new(xdest, ydest, width, 16),
                (x, y + 1),
                18,
            )?;
        }
    }

    for i in 0..28 {
        let a = phase * 0.9 + i as f32 * (std::f32::consts::TAU / 28.0);
        let radius = 2.0 + energy * 7.0 + (phase * 1.4 + i as f32 * 0.7).sin() * 1.4;
        let x = (cx + a.cos() * radius * 3.4).clamp(0.0, (width - 1) as f32) as i32;
        let y = (cy + a.sin() * radius).clamp(0.0, 15.0) as i32;
        draw_vis_pixel(
            cr,
            colors,
            SkinRect::new(xdest, ydest, width, 16),
            (x, y),
            21 + (i % 3),
        )?;
    }

    Ok(())
}

fn visualization_level(value: f32) -> i32 {
    (value * 16.0 + 0.5).clamp(0.0, 16.0) as i32
}

fn analyzer_color(mode: VisAnalyzerMode, row: i32, height: i32) -> usize {
    match mode {
        VisAnalyzerMode::Fire => (row + height - 14).clamp(0, 23) as usize,
        VisAnalyzerMode::VerticalLines => (18 - height).clamp(0, 23) as usize,
        VisAnalyzerMode::Normal => (row + 2).clamp(0, 23) as usize,
    }
}

fn draw_vis_pixel(
    cr: &Context,
    colors: &[[u8; 3]; 24],
    area: SkinRect,
    point: (i32, i32),
    color_idx: usize,
) -> Result<(), RenderError> {
    let (x, y) = point;
    if x < 0 || x >= area.width || !(0..area.height).contains(&y) {
        return Ok(());
    }
    set_vis_color(cr, colors, color_idx);
    cr.rectangle(f64::from(area.x + x), f64::from(area.y + y), 1.0, 1.0);
    cr.fill()?;
    Ok(())
}

fn set_vis_color(cr: &Context, colors: &[[u8; 3]; 24], color_idx: usize) {
    let color = colors[color_idx.min(23)];
    set_rgb(cr, color);
}

fn render_mono_stereo(
    cr: &Context,
    skin: &DefaultSkin,
    channels: i32,
    xdest: i32,
    ydest: i32,
) -> Result<bool, RenderError> {
    let (stereo_y, mono_y) = match channels {
        2 => (0, 12),
        1 => (12, 0),
        _ => (12, 12),
    };
    let mut rendered = blit_skin_rect(
        cr,
        skin,
        SkinPixmapKind::MonoStereo,
        SkinRect::new(29, mono_y, 27, 12),
        (xdest, ydest),
    )?;
    rendered |= blit_skin_rect(
        cr,
        skin,
        SkinPixmapKind::MonoStereo,
        SkinRect::new(0, stereo_y, 29, 12),
        (xdest + 27, ydest),
    )?;
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzer_colors_match_reference_modes() {
        let fire = (8..16)
            .map(|row| analyzer_color(VisAnalyzerMode::Fire, row, 8))
            .collect::<Vec<_>>();
        let normal = (8..16)
            .map(|row| analyzer_color(VisAnalyzerMode::Normal, row, 8))
            .collect::<Vec<_>>();
        let vertical = (8..16)
            .map(|row| analyzer_color(VisAnalyzerMode::VerticalLines, row, 8))
            .collect::<Vec<_>>();
        assert_eq!(fire, (2..10).collect::<Vec<_>>());
        assert_eq!(normal, (10..18).collect::<Vec<_>>());
        assert_eq!(vertical, vec![10; 8]);
    }
}
