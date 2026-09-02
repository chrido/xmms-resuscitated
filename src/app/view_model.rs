//! Data-only view models and pure presentation helpers consumed by UI frontends.
//!
//! View models should expose presentation-ready state without tying callers to
//! GTK or any other concrete UI toolkit.

use std::path::Path;
use std::time::Duration;

use crate::app_state::AppState;
use crate::audio_model::EqualizerBandPositions;
use crate::config::Config;
use crate::player::PlayerState;
use crate::playlist::{PlaylistEntry, PlaylistMenuKind, PlaylistRevisions};
use crate::render::{PlaylistRowRenderEntry, PlaylistRowsRenderState};
use crate::skin::layout::{playlist_menu_button_at, playlist_menu_popup_rect, PlaylistMenuButton};
use crate::skin::widget::TextBox;

const TITLE_SCROLL_SPEED_PX_PER_SECOND: f64 = 12.0;
const TITLE_SCROLL_END_PAUSE: Duration = Duration::from_millis(1_500);
const PLAYLIST_ROW_HEIGHT: i32 = 11;
const PLAYLIST_ROW_AREA_CHROME: i32 = 58;
const PLAYLIST_ROW_OVERSCAN: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct TitleMarquee {
    title: String,
    viewport_width: i32,
    overflow_px: i32,
    elapsed: Duration,
    offset_px: i32,
    advancing: bool,
}

impl Default for TitleMarquee {
    fn default() -> Self {
        Self {
            title: String::new(),
            viewport_width: 0,
            overflow_px: 0,
            elapsed: Duration::ZERO,
            offset_px: 0,
            advancing: false,
        }
    }
}

impl TitleMarquee {
    pub fn update(
        &mut self,
        title: &str,
        viewport_width: i32,
        player_state: PlayerState,
        visible: bool,
        elapsed: Duration,
    ) -> bool {
        let overflow_px = title_overflow_px(title, viewport_width);
        let layout_changed = self.title != title || self.viewport_width != viewport_width;
        let should_reset =
            layout_changed || !visible || player_state == PlayerState::Stopped || overflow_px == 0;
        let previous_offset = self.offset_px;
        let can_advance = visible && overflow_px > 0 && player_state == PlayerState::Playing;

        if should_reset {
            self.title = title.to_string();
            self.viewport_width = viewport_width;
            self.overflow_px = overflow_px;
            self.elapsed = Duration::ZERO;
            self.offset_px = 0;
        }
        if can_advance && self.advancing && !layout_changed {
            self.elapsed += elapsed;
            self.offset_px = marquee_offset(self.elapsed, self.overflow_px);
        }
        self.advancing = can_advance;

        self.offset_px != previous_offset
    }

    pub fn offset_px(&self) -> i32 {
        self.offset_px
    }

    pub fn is_scrolling(&self, player_state: PlayerState, visible: bool) -> bool {
        visible && player_state == PlayerState::Playing && self.overflow_px > 0
    }
}

pub fn title_overflow_px(title: &str, viewport_width: i32) -> i32 {
    let glyphs = i32::try_from(title.chars().count()).unwrap_or(i32::MAX);
    glyphs
        .saturating_mul(TextBox::CHAR_WIDTH)
        .saturating_sub(viewport_width.max(0))
        .max(0)
}

fn marquee_offset(elapsed: Duration, overflow_px: i32) -> i32 {
    if overflow_px <= 0 {
        return 0;
    }

    let pause = TITLE_SCROLL_END_PAUSE.as_secs_f64();
    let travel = f64::from(overflow_px) / TITLE_SCROLL_SPEED_PX_PER_SECOND;
    let cycle = 2.0 * (pause + travel);
    let phase = elapsed.as_secs_f64() % cycle;
    let offset = if phase < pause {
        0.0
    } else if phase < pause + travel {
        (phase - pause) * TITLE_SCROLL_SPEED_PX_PER_SECOND
    } else if phase < 2.0 * pause + travel {
        f64::from(overflow_px)
    } else {
        f64::from(overflow_px) - (phase - (2.0 * pause + travel)) * TITLE_SCROLL_SPEED_PX_PER_SECOND
    };
    offset.round().clamp(0.0, f64::from(overflow_px)) as i32
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainPlayerViewModel {
    pub title: String,
    pub player_state: PlayerState,
    pub volume: i32,
    pub balance: i32,
    pub shuffle: bool,
    pub repeat: bool,
    pub shaded: bool,
    pub bitrate_text: String,
    pub frequency_text: String,
    pub channels_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRowViewModel {
    pub index: usize,
    pub title: String,
    pub duration_text: Option<String>,
    pub selected: bool,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistViewModel {
    pub rows: Vec<PlaylistRowViewModel>,
    pub current_index: Option<usize>,
    pub visible: bool,
    pub shaded: bool,
    pub detached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistProjectionKey {
    revisions: PlaylistRevisions,
    title_format: String,
    convert_underscore: bool,
    convert_twenty: bool,
}

impl PlaylistProjectionKey {
    pub fn from_state(state: &AppState) -> Self {
        Self {
            revisions: state.playlist.revisions(),
            title_format: state.config.title_format.clone(),
            convert_underscore: state.config.convert_underscore,
            convert_twenty: state.config.convert_twenty,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistProjection {
    pub rows: Vec<PlaylistRowViewModel>,
    pub current_index: Option<usize>,
    pub footer_info: String,
}

pub fn playlist_projection(state: &AppState) -> PlaylistProjection {
    let view_model = playlist_view_model(state);
    PlaylistProjection {
        rows: view_model.rows,
        current_index: view_model.current_index,
        footer_info: playlist_footer_info(state),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqualizerViewModel {
    pub active: bool,
    pub auto: bool,
    pub preamp_position: i32,
    pub band_positions: EqualizerBandPositions,
    pub visible: bool,
    pub shaded: bool,
    pub detached: bool,
}

pub fn main_player_view_model(state: &AppState) -> MainPlayerViewModel {
    let title = formatted_current_title(state);
    MainPlayerViewModel {
        title,
        player_state: state.player.state(),
        volume: state.player.volume(),
        balance: state.player.balance(),
        shuffle: state.playlist.shuffle(),
        repeat: state.playlist.repeat(),
        shaded: state.config.main_shaded,
        bitrate_text: state.player.bitrate().to_string(),
        frequency_text: state.player.frequency().to_string(),
        channels_text: state.player.channels().to_string(),
    }
}

pub fn formatted_playlist_entry_title(state: &AppState, entry: &PlaylistEntry) -> String {
    format_title_for_preferences(
        &state.config.title_format,
        &entry.filename,
        &entry.title,
        &state.config,
    )
}

pub fn formatted_current_title(state: &AppState) -> String {
    let Some(position) = state.playlist.position() else {
        return "XMMS Renascene".to_string();
    };
    let Some(entry) = state.playlist.entries().get(position) else {
        return "XMMS Renascene".to_string();
    };
    formatted_playlist_entry_title(state, entry)
}

pub fn playlist_view_model(state: &AppState) -> PlaylistViewModel {
    crate::perf_span!("playlist_view_model_build");
    let current_index = state.playlist.position();
    let rows = state
        .playlist
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| PlaylistRowViewModel {
            index,
            title: formatted_playlist_entry_title(state, entry),
            duration_text: (entry.length_ms >= 0).then(|| format_duration(entry.length_ms)),
            selected: entry.selected,
            current: Some(index) == current_index,
        })
        .collect();
    PlaylistViewModel {
        rows,
        current_index,
        visible: state.config.playlist_visible,
        shaded: state.config.playlist_shaded,
        detached: state.config.playlist_detached,
    }
}

pub fn equalizer_view_model(state: &AppState) -> EqualizerViewModel {
    EqualizerViewModel {
        active: state.config.equalizer_active,
        auto: state.config.equalizer_auto,
        preamp_position: state.config.equalizer_preamp_pos,
        band_positions: state.config.equalizer_band_pos,
        visible: state.config.equalizer_visible,
        shaded: state.config.equalizer_shaded,
        detached: state.config.equalizer_detached,
    }
}

pub fn playlist_menu_at(x: i32, y: i32, width: i32, height: i32) -> Option<PlaylistMenuKind> {
    playlist_menu_button_at(x, y, width, height).map(playlist_menu_from_button)
}

pub fn playlist_menu_from_button(button: PlaylistMenuButton) -> PlaylistMenuKind {
    match button {
        PlaylistMenuButton::Add => PlaylistMenuKind::Add,
        PlaylistMenuButton::Remove => PlaylistMenuKind::Remove,
        PlaylistMenuButton::Select => PlaylistMenuKind::Select,
        PlaylistMenuButton::Misc => PlaylistMenuKind::Misc,
        PlaylistMenuButton::List => PlaylistMenuKind::List,
    }
}

pub fn playlist_menu_button_from_kind(menu: PlaylistMenuKind) -> PlaylistMenuButton {
    match menu {
        PlaylistMenuKind::Add => PlaylistMenuButton::Add,
        PlaylistMenuKind::Remove => PlaylistMenuButton::Remove,
        PlaylistMenuKind::Select => PlaylistMenuButton::Select,
        PlaylistMenuKind::Misc => PlaylistMenuButton::Misc,
        PlaylistMenuKind::List => PlaylistMenuButton::List,
    }
}

pub fn playlist_menu_rect(menu: PlaylistMenuKind, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let rect = playlist_menu_popup_rect(playlist_menu_button_from_kind(menu), width, height);
    (rect.x, rect.y, rect.width, rect.height)
}

pub fn volume_to_position(volume: i32) -> i32 {
    ((volume.clamp(0, 100) * 51 + 50) / 100).clamp(0, 51)
}

pub fn position_to_volume(position: i32) -> i32 {
    ((position.clamp(0, 51) * 100) as f64 / 51.0) as i32
}

pub fn volume_to_eq_shaded_position(volume: i32) -> i32 {
    ((volume.clamp(0, 100) * 94 + 50) / 100).clamp(0, 94)
}

pub fn balance_to_position(balance: i32) -> i32 {
    (12 + (balance.clamp(-100, 100) * 12) / 100).clamp(0, 24)
}

pub fn position_to_balance(position: i32) -> i32 {
    (((position.clamp(0, 24) - 12) * 100) as f64 / 12.0) as i32
}

pub fn balance_to_eq_shaded_position(balance: i32) -> i32 {
    (19 + (balance.clamp(-100, 100) * 19) / 100).clamp(0, 39)
}

pub fn format_duration(milliseconds: i64) -> String {
    let seconds = (milliseconds.max(0) / 1000) as i32;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn format_playlist_footer_duration(milliseconds: i64, more: bool) -> String {
    if milliseconds <= 0 && more {
        return "?".to_string();
    }

    let seconds = milliseconds.max(0) / 1000;
    if seconds > 3600 {
        format!(
            "{}:{:02}:{:02}{}",
            seconds / 3600,
            (seconds / 60) % 60,
            seconds % 60,
            if more { "+" } else { "" }
        )
    } else {
        format!(
            "{}:{:02}{}",
            seconds / 60,
            seconds % 60,
            if more { "+" } else { "" }
        )
    }
}

pub fn playlist_rows_render_state(
    state: &AppState,
    scroll_offset: usize,
    scrollbar_dragging: bool,
    search_query: Option<String>,
    width: i32,
    height: i32,
) -> PlaylistRowsRenderState {
    crate::perf_span!("playlist_rows_render_state");
    let projection = playlist_projection(state);
    playlist_rows_render_state_from_projection(
        state,
        &projection,
        scroll_offset,
        scrollbar_dragging,
        search_query,
        width,
        height,
    )
}

pub fn playlist_rows_render_state_from_projection(
    state: &AppState,
    projection: &PlaylistProjection,
    scroll_offset: usize,
    scrollbar_dragging: bool,
    search_query: Option<String>,
    width: i32,
    height: i32,
) -> PlaylistRowsRenderState {
    let visible_rows =
        ((height.max(0) - PLAYLIST_ROW_AREA_CHROME) / PLAYLIST_ROW_HEIGHT).max(0) as usize;
    let first_index = scroll_offset.saturating_sub(PLAYLIST_ROW_OVERSCAN);
    let end_index = scroll_offset
        .saturating_add(visible_rows)
        .saturating_add(PLAYLIST_ROW_OVERSCAN)
        .min(projection.rows.len());
    PlaylistRowsRenderState {
        entries: projection
            .rows
            .get(first_index..end_index)
            .unwrap_or_default()
            .iter()
            .map(|row| PlaylistRowRenderEntry {
                title: row.title.clone(),
                length_ms: state
                    .playlist
                    .entries()
                    .get(row.index)
                    .map(|entry| entry.length_ms)
                    .unwrap_or(-1),
                selected: row.selected,
                current: row.current,
                queue_position: state.playlist.queue_position(row.index),
            })
            .collect(),
        first_index,
        total_entries: projection.rows.len(),
        scroll_offset,
        scrollbar_dragging,
        search_query,
        show_numbers: state.config.show_numbers_in_pl,
        font_family: state.config.playlist_font.clone(),
        width,
        height,
    }
}

pub fn playlist_footer_info(state: &AppState) -> String {
    crate::perf_span!("playlist_footer_aggregation");
    let mut selected_ms = 0_i64;
    let mut total_ms = 0_i64;
    let mut selected_more = false;
    let mut total_more = false;

    for entry in state.playlist.entries() {
        if entry.length_ms >= 0 {
            total_ms += entry.length_ms;
        } else {
            total_more = true;
        }

        if entry.selected {
            if entry.length_ms >= 0 {
                selected_ms += entry.length_ms;
            } else {
                selected_more = true;
            }
        }
    }

    format!(
        "{}/{}",
        format_playlist_footer_duration(selected_ms, selected_more),
        format_playlist_footer_duration(total_ms, total_more)
    )
}

pub fn format_title_for_preferences(
    format: &str,
    filename: &str,
    title: &str,
    config: &Config,
) -> String {
    let title = title.trim();
    let fallback_title =
        if title.is_empty() || title == crate::playlist::format_title(filename, None) {
            filename_title(filename, config)
        } else {
            normalize_title_text(title, config)
        };
    let (artist, track_title) = split_artist_title(&fallback_title);
    let file_title = filename_title(filename, config);
    let format = if format.trim().is_empty() {
        "%p - %t"
    } else {
        format.trim()
    };

    let mut output = String::new();
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('p') => output.push_str(artist.unwrap_or("")),
            Some('t') => output.push_str(track_title),
            Some('f') => output.push_str(&file_title),
            Some('a') | Some('g') => {}
            Some('%') => output.push('%'),
            Some(other) => {
                output.push('%');
                output.push(other);
            }
            None => output.push('%'),
        }
    }

    cleanup_formatted_title(&output).unwrap_or(fallback_title)
}

pub fn split_artist_title(title: &str) -> (Option<&str>, &str) {
    title
        .split_once(" - ")
        .map(|(artist, track)| (Some(artist.trim()), track.trim()))
        .unwrap_or((None, title.trim()))
}

pub fn filename_title(filename: &str, config: &Config) -> String {
    let without_query = filename.split(['?', '#']).next().unwrap_or(filename);
    let normalized = normalize_title_text(without_query, config);
    let path = normalized
        .strip_prefix("file://")
        .unwrap_or(normalized.as_str())
        .trim_end_matches('/');
    let basename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    let stem = basename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(basename);
    stem.to_string()
}

pub fn normalize_title_text(text: &str, config: &Config) -> String {
    let mut normalized = text.to_string();
    if config.convert_twenty {
        normalized = normalized.replace("%20", " ");
    }
    if config.convert_underscore {
        normalized = normalized.replace('_', " ");
    }
    normalized
}

pub fn cleanup_formatted_title(text: &str) -> Option<String> {
    let mut cleaned = text.trim().to_string();
    for prefix in ["- ", ":", "/", "|"] {
        cleaned = cleaned.trim_start_matches(prefix).trim_start().to_string();
    }
    for suffix in [" -", ":", "/", "|"] {
        cleaned = cleaned.trim_end_matches(suffix).trim_end().to_string();
    }
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub fn ellipsize_chars(text: &str, max_len: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_len {
        return text.to_string();
    }
    if max_len > 3 {
        let mut truncated: String = text.chars().take(max_len - 3).collect();
        truncated.push_str("...");
        truncated
    } else {
        text.chars().take(max_len).collect()
    }
}

pub fn eq_shaded_position_to_volume(position: i32) -> i32 {
    ((position.clamp(0, 94) * 100 + 47) / 94).clamp(0, 100)
}

pub fn eq_shaded_position_to_balance(position: i32) -> i32 {
    let position = position.clamp(0, 38);
    (((position - 19) * 100 + if position >= 19 { 9 } else { -9 }) / 19).clamp(-100, 100)
}

pub fn eq_slider_position_to_pixel(position: i32) -> i32 {
    let pixel = position.clamp(0, 100) / 2;
    if (24..=26).contains(&pixel) {
        25
    } else {
        pixel
    }
}

pub fn eq_slider_pixel_to_position(pixel: i32) -> i32 {
    let pixel = pixel.clamp(0, 50);
    if (24..=26).contains(&pixel) {
        50
    } else {
        pixel * 2
    }
}

pub fn scale_event_coords(
    width: f64,
    height: f64,
    base_width: i32,
    base_height: i32,
    x: f64,
    y: f64,
) -> (i32, i32) {
    (
        (x / (width / f64::from(base_width))) as i32,
        (y / (height / f64::from(base_height))) as i32,
    )
}

pub fn parse_time_ms(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    if let Some((minutes, seconds)) = text.split_once(':') {
        if seconds.contains(':') {
            return None;
        }
        let minutes = minutes.parse::<i64>().ok()?;
        let seconds = seconds.parse::<i64>().ok()?;
        return Some((minutes * 60 + seconds) * 1000);
    }
    Some(text.parse::<i64>().ok()? * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_marquee_detects_only_real_overflow() {
        assert_eq!(title_overflow_px("12345", 25), 0);
        assert_eq!(title_overflow_px("123456", 25), 5);

        let mut marquee = TitleMarquee::default();
        marquee.update(
            "12345",
            25,
            PlayerState::Playing,
            true,
            Duration::from_secs(10),
        );
        assert_eq!(marquee.offset_px(), 0);
        assert!(!marquee.is_scrolling(PlayerState::Playing, true));
    }

    #[test]
    fn title_marquee_progresses_from_elapsed_time_after_initial_pause() {
        let mut marquee = TitleMarquee::default();
        marquee.update("1234567890", 25, PlayerState::Playing, true, Duration::ZERO);
        marquee.update(
            "1234567890",
            25,
            PlayerState::Playing,
            true,
            Duration::from_millis(1_500),
        );
        assert_eq!(marquee.offset_px(), 0);

        marquee.update(
            "1234567890",
            25,
            PlayerState::Playing,
            true,
            Duration::from_millis(500),
        );
        assert_eq!(marquee.offset_px(), 6);
    }

    #[test]
    fn title_marquee_resets_for_title_stop_shade_and_fitting_layout() {
        let mut marquee = TitleMarquee::default();
        marquee.update("1234567890", 25, PlayerState::Playing, true, Duration::ZERO);
        marquee.update(
            "1234567890",
            25,
            PlayerState::Playing,
            true,
            Duration::from_secs(2),
        );
        assert!(marquee.offset_px() > 0);

        marquee.update(
            "different long title",
            25,
            PlayerState::Playing,
            true,
            Duration::ZERO,
        );
        assert_eq!(marquee.offset_px(), 0);
        marquee.update(
            "different long title",
            25,
            PlayerState::Playing,
            true,
            Duration::from_secs(2),
        );
        marquee.update(
            "different long title",
            25,
            PlayerState::Playing,
            true,
            Duration::from_secs(2),
        );
        marquee.update(
            "different long title",
            25,
            PlayerState::Stopped,
            true,
            Duration::ZERO,
        );
        assert_eq!(marquee.offset_px(), 0);

        marquee.update(
            "different long title",
            25,
            PlayerState::Playing,
            false,
            Duration::from_secs(2),
        );
        assert_eq!(marquee.offset_px(), 0);
        marquee.update(
            "fits",
            25,
            PlayerState::Playing,
            true,
            Duration::from_secs(2),
        );
        assert_eq!(marquee.offset_px(), 0);
    }

    #[test]
    fn title_marquee_loops_smoothly_back_to_the_start() {
        let mut marquee = TitleMarquee::default();
        marquee.update("1234567890", 26, PlayerState::Playing, true, Duration::ZERO);
        marquee.update(
            "1234567890",
            26,
            PlayerState::Playing,
            true,
            Duration::from_secs(7),
        );
        assert_eq!(marquee.offset_px(), 0);

        marquee.update(
            "1234567890",
            26,
            PlayerState::Playing,
            true,
            Duration::from_millis(2_500),
        );
        assert_eq!(marquee.offset_px(), 12);
    }

    #[test]
    fn parse_time_accepts_seconds_and_minutes_seconds() {
        assert_eq!(parse_time_ms("42"), Some(42_000));
        assert_eq!(parse_time_ms("1:23"), Some(83_000));
        assert_eq!(parse_time_ms(""), None);
        assert_eq!(parse_time_ms("1:2:3"), None);
        assert_eq!(parse_time_ms("not-time"), None);
    }

    #[test]
    fn duration_formatting_matches_xmms_style() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(83_000), "1:23");
        assert_eq!(format_playlist_footer_duration(0, true), "?");
        assert_eq!(format_playlist_footer_duration(83_000, false), "1:23");
        assert_eq!(format_playlist_footer_duration(3_661_000, true), "1:01:01+");
    }

    #[test]
    fn playlist_footer_info_formats_selected_and_total_durations() {
        let mut state = AppState::default();
        state
            .playlist
            .add_timed_uri("file:///tmp/one.ogg", "One", 60_000);
        state.playlist.add_uri("file:///tmp/unknown.ogg");
        state
            .playlist
            .add_timed_uri("file:///tmp/two.ogg", "Two", 90_000);
        state.playlist.set_position(0);

        assert_eq!(playlist_footer_info(&state), "0:00/2:30+");

        state.playlist.entries_mut()[1].selected = true;
        assert_eq!(playlist_footer_info(&state), "?/2:30+");

        state.playlist.entries_mut()[1].selected = false;
        state.playlist.entries_mut()[2].selected = true;
        assert_eq!(playlist_footer_info(&state), "1:30/2:30+");
    }

    #[test]
    fn slider_conversions_clamp_to_skin_ranges() {
        assert_eq!(volume_to_position(-1), 0);
        assert_eq!(volume_to_position(100), 51);
        assert_eq!(position_to_volume(51), 100);
        assert_eq!(balance_to_position(-100), 0);
        assert_eq!(balance_to_position(100), 24);
        assert_eq!(eq_slider_position_to_pixel(50), 25);
        assert_eq!(eq_slider_pixel_to_position(25), 50);
    }

    #[test]
    fn formatted_current_title_uses_preferences_and_fallback() {
        let mut state = AppState::default();
        assert_eq!(formatted_current_title(&state), "XMMS Renascene");

        state.config.title_format = "%t (%p)".to_string();
        state
            .playlist
            .add_timed_uri("file:///tmp/song.ogg", "Artist - Song", 12_000);
        state.playlist.set_position(0);

        assert_eq!(formatted_current_title(&state), "Song (Artist)");
        assert_eq!(main_player_view_model(&state).title, "Song (Artist)");
    }

    #[test]
    fn playlist_rows_render_state_applies_preferences_title_format() {
        let mut state = AppState::default();
        state.config.title_format = "%t (%p)".to_string();
        state
            .playlist
            .add_timed_uri("file:///tmp/song.ogg", "Artist - Song", 12_000);
        state.playlist.entries_mut()[0].selected = true;
        state.playlist.set_position(0);

        let rows = playlist_rows_render_state(&state, 2, true, Some("Song".to_string()), 275, 232);

        assert_eq!(rows.entries[0].title, "Song (Artist)");
        assert_eq!(rows.entries[0].length_ms, 12_000);
        assert!(rows.entries[0].selected);
        assert!(rows.entries[0].current);
        assert_eq!(rows.scroll_offset, 2);
        assert!(rows.scrollbar_dragging);
        assert_eq!(rows.search_query.as_deref(), Some("Song"));
        assert_eq!(rows.width, 275);
        assert_eq!(rows.height, 232);
    }

    #[test]
    fn playlist_rows_render_state_is_bounded_for_large_playlists() {
        let mut state = AppState::default();
        for index in 0..10_000 {
            state.playlist.add_timed_uri(
                format!("file:///tmp/{index}.ogg"),
                format!("Track {index}"),
                1_000,
            );
        }

        let rows = playlist_rows_render_state(&state, 5_000, false, None, 275, 232);

        assert_eq!(rows.total_entries, 10_000);
        assert_eq!(rows.first_index, 4_998);
        assert!(rows.entries.len() <= 19);
        assert_eq!(rows.entries[2].title, "Track 5000");
    }

    #[test]
    fn playlist_view_model_marks_current_and_selected_rows() {
        let mut state = AppState::default();
        state
            .playlist
            .add_timed_uri("file:///tmp/one.ogg", "One", 83_000);
        state
            .playlist
            .add_timed_uri("file:///tmp/two.ogg", "Two", -1);
        state.playlist.set_position(0);
        state.playlist.entries_mut()[1].selected = true;

        let view_model = playlist_view_model(&state);

        assert_eq!(view_model.current_index, Some(0));
        assert!(view_model.rows[0].current);
        assert_eq!(view_model.rows[0].duration_text.as_deref(), Some("1:23"));
        assert!(view_model.rows[1].selected);
        assert_eq!(view_model.rows[1].duration_text, None);
    }

    #[test]
    fn playlist_projection_key_ignores_unrelated_player_state() {
        let mut state = AppState::default();
        state.playlist.add_timed_uri("one.mp3", "One", 1_000);
        let key = PlaylistProjectionKey::from_state(&state);

        state.player.set_volume(25);
        assert_eq!(PlaylistProjectionKey::from_state(&state), key);

        state.playlist.select_all(true);
        assert_ne!(PlaylistProjectionKey::from_state(&state), key);
    }

    #[test]
    fn equalizer_view_model_follows_config_state() {
        let state = AppState::from_config(Config {
            equalizer_visible: true,
            equalizer_shaded: true,
            equalizer_detached: true,
            equalizer_active: true,
            equalizer_auto: true,
            equalizer_preamp_pos: 42,
            equalizer_band_pos: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            ..Config::default()
        });

        let view_model = equalizer_view_model(&state);

        assert!(view_model.visible);
        assert!(view_model.shaded);
        assert!(view_model.detached);
        assert!(view_model.active);
        assert!(view_model.auto);
        assert_eq!(view_model.preamp_position, 42);
        assert_eq!(view_model.band_positions[9], 10);
    }
}
