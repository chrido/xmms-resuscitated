use crate::audio_model::{SpectrumData, SPECTRUM_BANDS};

use super::SkinPixmapKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WidgetRect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width
            && y >= self.y
            && y < self.y + self.height
            && self.width > 0
            && self.height > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widget {
    id: WidgetId,
    rect: WidgetRect,
    visible: bool,
    redraw: bool,
}

impl Widget {
    pub fn new(id: WidgetId, rect: WidgetRect) -> Self {
        Self {
            id,
            rect,
            visible: true,
            redraw: false,
        }
    }

    pub fn id(&self) -> WidgetId {
        self.id
    }

    pub fn rect(&self) -> WidgetRect {
        self.rect
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn needs_redraw(&self) -> bool {
        self.redraw
    }

    pub fn queue_draw(&mut self) {
        self.redraw = true;
    }

    pub fn clear_redraw(&mut self) {
        self.redraw = false;
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.visible && self.rect.contains(x, y)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WidgetList {
    widgets: Vec<Widget>,
    next_id: usize,
}

impl WidgetList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, rect: WidgetRect) -> WidgetId {
        let id = WidgetId(self.next_id);
        self.next_id += 1;
        self.widgets.push(Widget::new(id, rect));
        id
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Widget> {
        self.widgets.iter()
    }

    pub fn iter_visible(&self) -> impl Iterator<Item = &Widget> {
        self.widgets.iter().filter(|widget| widget.is_visible())
    }

    pub fn get(&self, id: WidgetId) -> Option<&Widget> {
        self.widgets.iter().find(|widget| widget.id == id)
    }

    pub fn get_mut(&mut self, id: WidgetId) -> Option<&mut Widget> {
        self.widgets.iter_mut().find(|widget| widget.id == id)
    }

    pub fn find_at(&self, x: i32, y: i32) -> Option<&Widget> {
        self.widgets
            .iter()
            .rev()
            .find(|widget| widget.contains(x, y))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinSource {
    pub kind: SkinPixmapKind,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushButton {
    widget: Widget,
    normal: SkinSource,
    pressed_source: SkinSource,
    pressed: bool,
    inside: bool,
    allow_draw: bool,
}

impl PushButton {
    pub fn new(
        id: WidgetId,
        rect: WidgetRect,
        normal: SkinSource,
        pressed_source: SkinSource,
    ) -> Self {
        Self {
            widget: Widget::new(id, rect),
            normal,
            pressed_source,
            pressed: false,
            inside: false,
            allow_draw: true,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    pub fn is_inside(&self) -> bool {
        self.inside
    }

    pub fn allow_draw(&self) -> bool {
        self.allow_draw
    }

    pub fn set_allow_draw(&mut self, allow_draw: bool) {
        if self.allow_draw != allow_draw {
            self.allow_draw = allow_draw;
            self.widget.queue_draw();
        }
    }

    pub fn current_source(&self) -> Option<SkinSource> {
        if !self.allow_draw {
            return None;
        }
        if self.pressed && self.inside {
            Some(self.pressed_source)
        } else {
            Some(self.normal)
        }
    }

    pub fn press(&mut self, button: u32) {
        if button != 1 {
            return;
        }
        self.pressed = true;
        self.inside = true;
        self.widget.queue_draw();
    }

    pub fn release(&mut self, button: u32) -> bool {
        if button != 1 {
            return false;
        }
        let activated = self.pressed && self.inside;
        self.pressed = false;
        self.widget.queue_draw();
        activated
    }

    pub fn motion(&mut self, x: i32, y: i32) {
        if !self.pressed {
            return;
        }
        let was_inside = self.inside;
        self.inside = self.widget.rect().contains(x, y);
        if was_inside != self.inside {
            self.widget.queue_draw();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleButtonSources {
    pub normal_unselected: SkinSource,
    pub pressed_unselected: SkinSource,
    pub normal_selected: SkinSource,
    pub pressed_selected: SkinSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleButton {
    widget: Widget,
    sources: ToggleButtonSources,
    selected: bool,
    pressed: bool,
    inside: bool,
}

impl ToggleButton {
    pub fn new(id: WidgetId, rect: WidgetRect, sources: ToggleButtonSources) -> Self {
        Self {
            widget: Widget::new(id, rect),
            sources,
            selected: false,
            pressed: false,
            inside: false,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    pub fn is_inside(&self) -> bool {
        self.inside
    }

    pub fn set_selected(&mut self, selected: bool) {
        if self.selected != selected {
            self.selected = selected;
            self.widget.queue_draw();
        }
    }

    pub fn current_source(&self) -> SkinSource {
        match (self.selected, self.pressed && self.inside) {
            (true, true) => self.sources.pressed_selected,
            (true, false) => self.sources.normal_selected,
            (false, true) => self.sources.pressed_unselected,
            (false, false) => self.sources.normal_unselected,
        }
    }

    pub fn press(&mut self, button: u32) {
        if button != 1 {
            return;
        }
        self.pressed = true;
        self.inside = true;
        self.widget.queue_draw();
    }

    pub fn release(&mut self, button: u32) -> Option<bool> {
        if button != 1 {
            return None;
        }
        let activated = self.pressed && self.inside;
        if activated {
            self.selected = !self.selected;
        }
        self.pressed = false;
        self.widget.queue_draw();

        activated.then_some(self.selected)
    }

    pub fn motion(&mut self, x: i32, y: i32) {
        if !self.pressed {
            return;
        }
        let was_inside = self.inside;
        self.inside = self.widget.rect().contains(x, y);
        if was_inside != self.inside {
            self.widget.queue_draw();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBox {
    widget: Widget,
    scroll_enabled: bool,
    skin: SkinPixmapKind,
    original_text: Option<String>,
    text: Option<String>,
    offset: i32,
    rendered_width: i32,
    scrollable: bool,
}

impl TextBox {
    pub const CHAR_WIDTH: i32 = 5;
    pub const CHAR_HEIGHT: i32 = 6;
    pub const SCROLL_SEPARATOR: &'static str = "  ***  ";

    pub fn new(
        id: WidgetId,
        x: i32,
        y: i32,
        width: i32,
        scroll_enabled: bool,
        skin: SkinPixmapKind,
    ) -> Self {
        Self {
            widget: Widget::new(
                id,
                WidgetRect {
                    x,
                    y,
                    width,
                    height: Self::CHAR_HEIGHT,
                },
            ),
            scroll_enabled,
            skin,
            original_text: None,
            text: None,
            offset: 0,
            rendered_width: Self::CHAR_WIDTH,
            scrollable: false,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn original_text(&self) -> Option<&str> {
        self.original_text.as_deref()
    }

    pub fn offset(&self) -> i32 {
        self.offset
    }

    pub fn rendered_width(&self) -> i32 {
        self.rendered_width
    }

    pub fn is_scrollable(&self) -> bool {
        self.scrollable
    }

    pub fn skin(&self) -> SkinPixmapKind {
        self.skin
    }

    pub fn set_text(&mut self, text: Option<&str>) {
        if self.original_text.as_deref() == text {
            return;
        }

        self.original_text = text.map(ToOwned::to_owned);
        self.text = text.map(|text| {
            let text_width = text.len() as i32 * Self::CHAR_WIDTH;
            if self.scroll_enabled && text_width > self.widget.rect().width {
                format!("{text}{}", Self::SCROLL_SEPARATOR)
            } else {
                text.to_owned()
            }
        });
        self.offset = 0;
        self.update_metrics();
        self.widget.queue_draw();
    }

    pub fn scroll_tick(&mut self) -> bool {
        if !self.scrollable || !self.scroll_enabled {
            return false;
        }

        self.offset += 1;
        if self.offset >= self.rendered_width {
            self.offset = 0;
        }
        self.widget.queue_draw();
        true
    }

    pub fn glyph_source(ch: char) -> Option<(i32, i32)> {
        match ch {
            'A'..='Z' => Some((((ch as u8 - b'A') as i32) * Self::CHAR_WIDTH, 0)),
            'a'..='z' => Some((((ch as u8 - b'a') as i32) * Self::CHAR_WIDTH, 0)),
            '0'..='9' => Some((((ch as u8 - b'0') as i32) * Self::CHAR_WIDTH, 6)),
            ' ' => None,
            '"' => Some((130, 0)),
            '@' => Some((135, 0)),
            '.' => Some((55, 6)),
            ':' => Some((60, 6)),
            '(' => Some((65, 6)),
            ')' => Some((70, 6)),
            '-' => Some((75, 6)),
            '\'' => Some((80, 6)),
            '!' => Some((85, 6)),
            '_' => Some((90, 6)),
            '+' => Some((95, 6)),
            '\\' => Some((100, 6)),
            '/' => Some((105, 6)),
            '[' => Some((110, 6)),
            ']' => Some((115, 6)),
            '^' => Some((120, 6)),
            '&' => Some((125, 6)),
            '%' => Some((130, 6)),
            ',' => Some((135, 6)),
            '=' => Some((140, 6)),
            '$' => Some((145, 6)),
            '#' => Some((150, 6)),
            '?' => Some((50, 12)),
            '*' => Some((55, 12)),
            _ => None,
        }
    }

    fn update_metrics(&mut self) {
        self.rendered_width = self
            .text
            .as_deref()
            .map(|text| (text.len() as i32 * Self::CHAR_WIDTH).max(Self::CHAR_WIDTH))
            .unwrap_or(Self::CHAR_WIDTH);
        self.scrollable = self.rendered_width > self.widget.rect().width;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HorizontalSlider {
    widget: Widget,
    skin: SkinPixmapKind,
    knob_normal: SkinSource,
    knob_pressed: SkinSource,
    knob_width: i32,
    knob_height: i32,
    frame_height: i32,
    frame_offset: i32,
    min: i32,
    max: i32,
    position: i32,
    draw_frame: bool,
    pressed: bool,
    press_offset: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HorizontalSliderSpec {
    pub id: WidgetId,
    pub rect: WidgetRect,
    pub skin: SkinPixmapKind,
    pub knob_normal: SkinSource,
    pub knob_pressed: SkinSource,
    pub knob_width: i32,
    pub knob_height: i32,
    pub frame_height: i32,
    pub frame_offset: i32,
    pub min: i32,
    pub max: i32,
}

impl HorizontalSlider {
    pub fn new(spec: HorizontalSliderSpec) -> Self {
        Self {
            widget: Widget::new(spec.id, spec.rect),
            skin: spec.skin,
            knob_normal: spec.knob_normal,
            knob_pressed: spec.knob_pressed,
            knob_width: spec.knob_width,
            knob_height: spec.knob_height,
            frame_height: spec.frame_height,
            frame_offset: spec.frame_offset,
            min: spec.min,
            max: spec.max,
            position: spec.min,
            draw_frame: true,
            pressed: false,
            press_offset: 0,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn position(&self) -> i32 {
        self.position
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    pub fn draw_frame(&self) -> bool {
        self.draw_frame
    }

    pub fn set_draw_frame(&mut self, draw_frame: bool) {
        self.draw_frame = draw_frame;
        self.widget.queue_draw();
    }

    pub fn set_position(&mut self, position: i32) {
        self.position = self.clamp_position(position);
        self.widget.queue_draw();
    }

    pub fn current_knob_source(&self) -> SkinSource {
        if self.pressed {
            self.knob_pressed
        } else {
            self.knob_normal
        }
    }

    pub fn knob_destination_x(&self) -> i32 {
        self.widget.rect().x + self.position
    }

    pub fn knob_size(&self) -> (i32, i32) {
        (self.knob_width, self.knob_height)
    }

    pub fn frame_source(&self, frame: i32) -> SkinSource {
        SkinSource {
            kind: self.skin,
            x: self.frame_offset,
            y: frame * self.frame_height,
        }
    }

    pub fn press(&mut self, x: i32, button: u32) -> Option<i32> {
        if button != 1 {
            return None;
        }

        self.pressed = true;
        let knob_x = self.widget.rect().x + self.position;
        let mut changed = None;
        if x >= knob_x && x < knob_x + self.knob_width {
            self.press_offset = x - knob_x;
        } else {
            self.press_offset = self.knob_width / 2;
            let position = self.clamp_position(x - self.widget.rect().x - self.press_offset);
            if self.position != position {
                self.position = position;
                changed = Some(self.position);
            }
        }
        self.widget.queue_draw();
        changed
    }

    pub fn motion(&mut self, x: i32) -> Option<i32> {
        if !self.pressed {
            return None;
        }

        let position = self.clamp_position(x - self.widget.rect().x - self.press_offset);
        self.position = position;
        self.widget.queue_draw();
        Some(position)
    }

    pub fn release(&mut self, button: u32) -> Option<i32> {
        if button != 1 {
            return None;
        }
        self.pressed = false;
        self.widget.queue_draw();
        Some(self.position)
    }

    fn clamp_position(&self, position: i32) -> i32 {
        position.clamp(self.min, self.max)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberDisplay {
    widget: Widget,
    skin: SkinPixmapKind,
    value: i32,
}

impl NumberDisplay {
    pub const WIDTH: i32 = 9;
    pub const HEIGHT: i32 = 13;
    pub const BLANK: i32 = 10;
    pub const DASH: i32 = 11;

    pub fn new(id: WidgetId, x: i32, y: i32, skin: SkinPixmapKind) -> Self {
        Self {
            widget: Widget::new(
                id,
                WidgetRect {
                    x,
                    y,
                    width: Self::WIDTH,
                    height: Self::HEIGHT,
                },
            ),
            skin,
            value: Self::BLANK,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn value(&self) -> i32 {
        self.value
    }

    pub fn set_value(&mut self, value: i32) {
        self.value = value;
        self.widget.queue_draw();
    }

    pub fn source(&self) -> SkinSource {
        let digit = if (0..=Self::DASH).contains(&self.value) {
            self.value
        } else {
            Self::DASH
        };
        SkinSource {
            kind: self.skin,
            x: digit * Self::WIDTH,
            y: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Visualization {
    widget: Widget,
    mode: VisMode,
    analyzer_style: VisAnalyzerStyle,
    analyzer_mode: VisAnalyzerMode,
    scope_mode: VisScopeMode,
    peaks_enabled: bool,
    analyzer_falloff: VisFalloffSpeed,
    peaks_falloff: VisFalloffSpeed,
    data: SpectrumData,
    peak: SpectrumData,
    peak_speed: SpectrumData,
    milkdrop_energy: f32,
    milkdrop_phase: f32,
}

impl Visualization {
    const ANALYZER_FALLOFF_SPEEDS: [f32; 5] =
        [0.34 / 16.0, 0.5 / 16.0, 1.0 / 16.0, 1.3 / 16.0, 1.6 / 16.0];
    const PEAK_FALLOFF_SPEEDS: [f32; 5] = [1.2, 1.3, 1.4, 1.5, 1.6];

    pub fn new(id: WidgetId, x: i32, y: i32, width: i32) -> Self {
        Self {
            widget: Widget::new(
                id,
                WidgetRect {
                    x,
                    y,
                    width,
                    height: 16,
                },
            ),
            mode: VisMode::Analyzer,
            analyzer_style: VisAnalyzerStyle::Bars,
            analyzer_mode: VisAnalyzerMode::Normal,
            scope_mode: VisScopeMode::Line,
            peaks_enabled: true,
            analyzer_falloff: VisFalloffSpeed::Fast,
            peaks_falloff: VisFalloffSpeed::Slow,
            data: [0.0; SPECTRUM_BANDS],
            peak: [0.0; SPECTRUM_BANDS],
            peak_speed: [0.0; SPECTRUM_BANDS],
            milkdrop_energy: 0.0,
            milkdrop_phase: 0.0,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn mode(&self) -> VisMode {
        self.mode
    }

    pub fn analyzer_style(&self) -> VisAnalyzerStyle {
        self.analyzer_style
    }

    pub fn analyzer_mode(&self) -> VisAnalyzerMode {
        self.analyzer_mode
    }

    pub fn scope_mode(&self) -> VisScopeMode {
        self.scope_mode
    }

    pub fn peaks_enabled(&self) -> bool {
        self.peaks_enabled
    }

    pub fn data(&self) -> &SpectrumData {
        &self.data
    }

    pub fn peak(&self) -> &SpectrumData {
        &self.peak
    }

    pub fn milkdrop_energy(&self) -> f32 {
        self.milkdrop_energy
    }

    pub fn milkdrop_phase(&self) -> f32 {
        self.milkdrop_phase
    }

    pub fn set_data(&mut self, data: &[f32]) {
        for (index, value) in data.iter().take(SPECTRUM_BANDS).enumerate() {
            let value = value.clamp(0.0, 15.0 / 16.0);
            if value > self.data[index] {
                self.data[index] = value;
            }
            if value > self.peak[index] {
                self.peak[index] = value;
                self.peak_speed[index] = 0.01 / 16.0;
            }
        }
    }

    pub fn clear_data(&mut self) {
        self.data = [0.0; SPECTRUM_BANDS];
        self.peak = [0.0; SPECTRUM_BANDS];
        self.peak_speed = [0.0; SPECTRUM_BANDS];
        self.milkdrop_energy = 0.0;
        self.milkdrop_phase = 0.0;
        self.widget.queue_draw();
    }

    pub fn tick(&mut self, data: Option<&[f32]>) {
        self.tick_with_steps(data, 1);
    }

    pub fn tick_with_steps(&mut self, data: Option<&[f32]>, steps: usize) {
        if self.mode == VisMode::Analyzer {
            self.tick_analyzer(data);
            for _ in 1..steps.max(1) {
                self.tick_analyzer(None);
            }
        } else if let Some(data) = data {
            for (target, value) in self.data.iter_mut().zip(data.iter().copied()) {
                *target = value.clamp(0.0, 1.0);
            }
        }
        let energy = self.data.iter().take(32).sum::<f32>() / 32.0;
        self.milkdrop_energy = self.milkdrop_energy * 0.88 + energy * 0.12;
        self.milkdrop_phase =
            (self.milkdrop_phase + 0.08 + self.milkdrop_energy * 0.08) % std::f32::consts::TAU;
        self.widget.queue_draw();
    }

    pub fn set_mode(&mut self, mode: VisMode) {
        self.mode = mode;
        self.widget.queue_draw();
    }

    pub fn set_analyzer_style(&mut self, style: VisAnalyzerStyle) {
        self.analyzer_style = style;
        self.widget.queue_draw();
    }

    pub fn set_analyzer_mode(&mut self, mode: VisAnalyzerMode) {
        self.analyzer_mode = mode;
        self.widget.queue_draw();
    }

    pub fn set_scope_mode(&mut self, mode: VisScopeMode) {
        self.scope_mode = mode;
        self.widget.queue_draw();
    }

    pub fn set_peaks_enabled(&mut self, enabled: bool) {
        self.peaks_enabled = enabled;
        self.widget.queue_draw();
    }

    pub fn set_falloff(&mut self, analyzer: VisFalloffSpeed, peaks: VisFalloffSpeed) {
        self.analyzer_falloff = analyzer;
        self.peaks_falloff = peaks;
        self.widget.queue_draw();
    }

    pub fn level(value: f32) -> i32 {
        (value * 16.0 + 0.5).clamp(0.0, 16.0) as i32
    }

    fn tick_analyzer(&mut self, new_data: Option<&[f32]>) {
        let analyzer_falloff = self.analyzer_falloff as usize;
        let peaks_falloff = self.peaks_falloff as usize;
        for index in 0..SPECTRUM_BANDS {
            let incoming = new_data
                .and_then(|data| data.get(index))
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 15.0 / 16.0);
            if incoming > self.data[index] {
                self.data[index] = incoming;
                if self.data[index] > self.peak[index] {
                    self.peak[index] = self.data[index];
                    self.peak_speed[index] = 0.01 / 16.0;
                } else {
                    Self::decay_peak(
                        &mut self.peak[index],
                        &mut self.peak_speed[index],
                        self.data[index],
                        peaks_falloff,
                    );
                }
            } else {
                self.data[index] =
                    (self.data[index] - Self::ANALYZER_FALLOFF_SPEEDS[analyzer_falloff]).max(0.0);
                Self::decay_peak(
                    &mut self.peak[index],
                    &mut self.peak_speed[index],
                    self.data[index],
                    peaks_falloff,
                );
            }
        }
    }

    fn decay_peak(peak: &mut f32, speed: &mut f32, level: f32, falloff: usize) {
        if *peak <= 0.0 {
            return;
        }
        *peak = (*peak - *speed).max(level).max(0.0);
        *speed *= Self::PEAK_FALLOFF_SPEEDS[falloff];
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorSegment {
    pub source: SkinSource,
    pub dest_x: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonoStereoIndicator {
    widget: Widget,
    skin: SkinPixmapKind,
    channels: i32,
}

impl MonoStereoIndicator {
    pub const WIDTH: i32 = 56;
    pub const HEIGHT: i32 = 12;

    pub fn new(id: WidgetId, x: i32, y: i32, skin: SkinPixmapKind) -> Self {
        Self {
            widget: Widget::new(
                id,
                WidgetRect {
                    x,
                    y,
                    width: Self::WIDTH,
                    height: Self::HEIGHT,
                },
            ),
            skin,
            channels: 0,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn channels(&self) -> i32 {
        self.channels
    }

    pub fn set_channels(&mut self, channels: i32) {
        self.channels = channels;
        self.widget.queue_draw();
    }

    pub fn segments(&self) -> [IndicatorSegment; 2] {
        let (stereo_y, mono_y) = match self.channels {
            2 => (0, 12),
            1 => (12, 0),
            _ => (12, 12),
        };
        [
            IndicatorSegment {
                source: SkinSource {
                    kind: self.skin,
                    x: 29,
                    y: mono_y,
                },
                dest_x: 0,
                width: 27,
                height: 12,
            },
            IndicatorSegment {
                source: SkinSource {
                    kind: self.skin,
                    x: 0,
                    y: stereo_y,
                },
                dest_x: 27,
                width: 29,
                height: 12,
            },
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayStatusValue {
    Stopped = 0,
    Paused = 1,
    Playing = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayStatusIndicator {
    widget: Widget,
    skin: SkinPixmapKind,
    status: PlayStatusValue,
}

impl PlayStatusIndicator {
    pub const WIDTH: i32 = 11;
    pub const HEIGHT: i32 = 9;

    pub fn new(id: WidgetId, x: i32, y: i32, skin: SkinPixmapKind) -> Self {
        Self {
            widget: Widget::new(
                id,
                WidgetRect {
                    x,
                    y,
                    width: Self::WIDTH,
                    height: Self::HEIGHT,
                },
            ),
            skin,
            status: PlayStatusValue::Stopped,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn status(&self) -> PlayStatusValue {
        self.status
    }

    pub fn set_status(&mut self, status: PlayStatusValue) {
        self.status = status;
        self.widget.queue_draw();
    }

    pub fn source(&self) -> SkinSource {
        let y = match self.status {
            PlayStatusValue::Playing => 0,
            PlayStatusValue::Paused => 9,
            PlayStatusValue::Stopped => 18,
        };
        SkinSource {
            kind: self.skin,
            x: 0,
            y,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleButton {
    widget: Widget,
    pressed: bool,
    inside: bool,
}

impl SimpleButton {
    pub fn new(id: WidgetId, rect: WidgetRect) -> Self {
        Self {
            widget: Widget::new(id, rect),
            pressed: false,
            inside: false,
        }
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    pub fn is_inside(&self) -> bool {
        self.inside
    }

    pub fn press(&mut self, button: u32) {
        if button != 1 {
            return;
        }
        self.pressed = true;
        self.inside = true;
    }

    pub fn release(&mut self, button: u32) -> bool {
        if button != 1 {
            return false;
        }
        let activated = self.pressed && self.inside;
        self.pressed = false;
        activated
    }

    pub fn motion(&mut self, x: i32, y: i32) {
        if !self.pressed {
            return;
        }
        self.inside = self.widget.rect().contains(x, y);
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisMode {
    Analyzer = 0,
    Scope = 1,
    Off = 2,
    Milkdrop = 3,
}

impl VisMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Scope,
            2 => Self::Off,
            3 => Self::Milkdrop,
            _ => Self::Analyzer,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisAnalyzerStyle {
    Bars = 0,
    Lines = 1,
}

impl VisAnalyzerStyle {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Lines,
            _ => Self::Bars,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisAnalyzerMode {
    Normal = 0,
    Fire = 1,
    VerticalLines = 2,
}

impl VisAnalyzerMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Fire,
            2 => Self::VerticalLines,
            _ => Self::Normal,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisScopeMode {
    Dot = 0,
    Line = 1,
    Solid = 2,
}

impl VisScopeMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Dot,
            2 => Self::Solid,
            _ => Self::Line,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisFalloffSpeed {
    Slowest = 0,
    Slow = 1,
    Medium = 2,
    Fast = 3,
    Fastest = 4,
}

impl VisFalloffSpeed {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Slowest,
            1 => Self::Slow,
            3 => Self::Fast,
            4 => Self::Fastest,
            _ => Self::Medium,
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisVuMode {
    Segmented = 0,
    Smooth = 1,
}

impl VisVuMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Smooth,
            _ => Self::Segmented,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_rect_contains_matches_c_exclusive_edges() {
        let rect = WidgetRect {
            x: 10,
            y: 20,
            width: 5,
            height: 3,
        };

        assert!(rect.contains(10, 20));
        assert!(rect.contains(14, 22));
        assert!(!rect.contains(15, 22));
        assert!(!rect.contains(14, 23));
        assert!(!rect.contains(9, 20));
    }

    #[test]
    fn widget_list_hit_tests_visible_widgets_in_reverse_order() {
        let mut list = WidgetList::new();
        let bottom = list.add(WidgetRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        });
        let top = list.add(WidgetRect {
            x: 5,
            y: 5,
            width: 10,
            height: 10,
        });

        assert_eq!(list.find_at(6, 6).unwrap().id(), top);
        list.get_mut(top).unwrap().set_visible(false);
        assert_eq!(list.find_at(6, 6).unwrap().id(), bottom);
    }

    #[test]
    fn widget_queue_draw_tracks_redraw_flag() {
        let mut list = WidgetList::new();
        let id = list.add(WidgetRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        });

        assert!(!list.get(id).unwrap().needs_redraw());
        list.get_mut(id).unwrap().queue_draw();
        assert!(list.get(id).unwrap().needs_redraw());
        list.get_mut(id).unwrap().clear_redraw();
        assert!(!list.get(id).unwrap().needs_redraw());
    }

    #[test]
    fn push_button_tracks_press_motion_release_like_c() {
        let normal = SkinSource {
            kind: SkinPixmapKind::CButtons,
            x: 0,
            y: 0,
        };
        let pressed = SkinSource {
            kind: SkinPixmapKind::CButtons,
            x: 0,
            y: 18,
        };
        let mut button = PushButton::new(
            WidgetId(1),
            WidgetRect {
                x: 10,
                y: 10,
                width: 5,
                height: 5,
            },
            normal,
            pressed,
        );

        assert_eq!(button.current_source(), Some(normal));
        button.press(2);
        assert!(!button.is_pressed());

        button.press(1);
        assert!(button.is_pressed());
        assert!(button.is_inside());
        assert_eq!(button.current_source(), Some(pressed));

        button.motion(20, 20);
        assert!(!button.is_inside());
        assert_eq!(button.current_source(), Some(normal));
        assert!(!button.release(1));

        button.press(1);
        assert!(button.release(1));
        assert!(!button.is_pressed());
    }

    #[test]
    fn push_button_allow_draw_controls_source_and_queues_redraw() {
        let source = SkinSource {
            kind: SkinPixmapKind::Titlebar,
            x: 0,
            y: 0,
        };
        let mut button = PushButton::new(
            WidgetId(1),
            WidgetRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            source,
            source,
        );

        button.set_allow_draw(false);
        assert!(!button.allow_draw());
        assert!(button.widget().needs_redraw());
        assert_eq!(button.current_source(), None);
    }

    #[test]
    fn toggle_button_toggles_on_left_release_inside() {
        let sources = ToggleButtonSources {
            normal_unselected: SkinSource {
                kind: SkinPixmapKind::ShufRep,
                x: 0,
                y: 0,
            },
            pressed_unselected: SkinSource {
                kind: SkinPixmapKind::ShufRep,
                x: 0,
                y: 15,
            },
            normal_selected: SkinSource {
                kind: SkinPixmapKind::ShufRep,
                x: 28,
                y: 0,
            },
            pressed_selected: SkinSource {
                kind: SkinPixmapKind::ShufRep,
                x: 28,
                y: 15,
            },
        };
        let mut button = ToggleButton::new(
            WidgetId(2),
            WidgetRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            sources,
        );

        assert_eq!(button.current_source(), sources.normal_unselected);
        button.press(1);
        assert_eq!(button.current_source(), sources.pressed_unselected);
        assert_eq!(button.release(1), Some(true));
        assert!(button.is_selected());
        assert_eq!(button.current_source(), sources.normal_selected);

        button.press(1);
        assert_eq!(button.current_source(), sources.pressed_selected);
        assert_eq!(button.release(1), Some(false));
        assert!(!button.is_selected());
    }

    #[test]
    fn toggle_button_release_outside_does_not_toggle() {
        let source = SkinSource {
            kind: SkinPixmapKind::ShufRep,
            x: 0,
            y: 0,
        };
        let sources = ToggleButtonSources {
            normal_unselected: source,
            pressed_unselected: source,
            normal_selected: source,
            pressed_selected: source,
        };
        let mut button = ToggleButton::new(
            WidgetId(2),
            WidgetRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            sources,
        );

        button.press(1);
        button.motion(12, 12);
        assert_eq!(button.release(1), None);
        assert!(!button.is_selected());
        button.set_selected(true);
        assert!(button.is_selected());
    }

    #[test]
    fn textbox_appends_scroll_separator_only_when_needed() {
        let mut textbox = TextBox::new(WidgetId(3), 0, 0, 20, true, SkinPixmapKind::Text);
        textbox.set_text(Some("abc"));
        assert_eq!(textbox.text(), Some("abc"));
        assert!(!textbox.is_scrollable());

        textbox.set_text(Some("abcdef"));
        assert_eq!(textbox.text(), Some("abcdef  ***  "));
        assert_eq!(textbox.original_text(), Some("abcdef"));
        assert!(textbox.is_scrollable());
        assert_eq!(textbox.offset(), 0);
        assert!(textbox.widget().needs_redraw());
    }

    #[test]
    fn textbox_scroll_tick_wraps_at_rendered_width() {
        let mut textbox = TextBox::new(WidgetId(3), 0, 0, 5, true, SkinPixmapKind::Text);
        textbox.set_text(Some("ab"));
        assert!(textbox.is_scrollable());
        let rendered_width = textbox.rendered_width();
        for _ in 0..rendered_width {
            assert!(textbox.scroll_tick());
        }
        assert_eq!(textbox.offset(), 0);
    }

    #[test]
    fn textbox_glyph_sources_match_c_font_map() {
        assert_eq!(TextBox::glyph_source('A'), Some((0, 0)));
        assert_eq!(TextBox::glyph_source('z'), Some((125, 0)));
        assert_eq!(TextBox::glyph_source('9'), Some((45, 6)));
        assert_eq!(TextBox::glyph_source('?'), Some((50, 12)));
        assert_eq!(TextBox::glyph_source(' '), None);
        assert_eq!(TextBox::glyph_source('~'), None);
    }

    fn slider_spec() -> HorizontalSliderSpec {
        HorizontalSliderSpec {
            id: WidgetId(4),
            rect: WidgetRect {
                x: 10,
                y: 20,
                width: 100,
                height: 10,
            },
            skin: SkinPixmapKind::PosBar,
            knob_normal: SkinSource {
                kind: SkinPixmapKind::PosBar,
                x: 0,
                y: 0,
            },
            knob_pressed: SkinSource {
                kind: SkinPixmapKind::PosBar,
                x: 0,
                y: 10,
            },
            knob_width: 10,
            knob_height: 10,
            frame_height: 10,
            frame_offset: 20,
            min: 0,
            max: 90,
        }
    }

    #[test]
    fn horizontal_slider_press_inside_knob_keeps_drag_offset() {
        let mut slider = HorizontalSlider::new(slider_spec());
        slider.set_position(20);

        assert_eq!(slider.press(35, 1), None);
        assert!(slider.is_pressed());
        assert_eq!(slider.current_knob_source(), slider_spec().knob_pressed);
        assert_eq!(slider.motion(45), Some(30));
        assert_eq!(slider.position(), 30);
        assert_eq!(slider.release(1), Some(30));
        assert!(!slider.is_pressed());
    }

    #[test]
    fn horizontal_slider_press_outside_knob_jumps_and_clamps() {
        let mut slider = HorizontalSlider::new(slider_spec());

        assert_eq!(slider.press(200, 1), Some(90));
        assert_eq!(slider.position(), 90);
        assert_eq!(slider.motion(-50), Some(0));
        assert_eq!(slider.release(2), None);
        assert!(slider.is_pressed());
        assert_eq!(slider.release(1), Some(0));
    }

    #[test]
    fn horizontal_slider_frame_and_draw_flags_match_c_state() {
        let mut slider = HorizontalSlider::new(slider_spec());
        assert!(slider.draw_frame());
        assert_eq!(
            slider.frame_source(3),
            SkinSource {
                kind: SkinPixmapKind::PosBar,
                x: 20,
                y: 30
            }
        );
        assert_eq!(slider.knob_destination_x(), 10);
        assert_eq!(slider.knob_size(), (10, 10));
        slider.set_draw_frame(false);
        assert!(!slider.draw_frame());
        assert!(slider.widget().needs_redraw());
    }

    #[test]
    fn number_display_defaults_to_blank_and_maps_invalid_to_dash() {
        let mut number = NumberDisplay::new(WidgetId(5), 3, 4, SkinPixmapKind::Numbers);
        assert_eq!(number.value(), NumberDisplay::BLANK);
        assert_eq!(
            number.source(),
            SkinSource {
                kind: SkinPixmapKind::Numbers,
                x: 90,
                y: 0
            }
        );
        assert_eq!(number.widget().rect().width, 9);
        assert_eq!(number.widget().rect().height, 13);

        number.set_value(7);
        assert_eq!(number.source().x, 63);
        assert!(number.widget().needs_redraw());

        number.set_value(12);
        assert_eq!(number.source().x, 99);
        number.set_value(-1);
        assert_eq!(number.source().x, 99);
    }

    #[test]
    fn visualization_defaults_match_c_widget() {
        let vis = Visualization::new(WidgetId(6), 24, 43, 76);
        assert_eq!(vis.widget().rect().height, 16);
        assert_eq!(vis.mode(), VisMode::Analyzer);
        assert_eq!(vis.analyzer_style(), VisAnalyzerStyle::Bars);
        assert_eq!(vis.analyzer_mode(), VisAnalyzerMode::Normal);
        assert_eq!(vis.scope_mode(), VisScopeMode::Line);
        assert!(vis.peaks_enabled());
    }

    #[test]
    fn visualization_set_data_clamps_and_updates_peaks() {
        let mut vis = Visualization::new(WidgetId(6), 0, 0, 75);
        vis.set_data(&[-1.0, 0.5, 2.0]);
        assert_eq!(vis.data()[0], 0.0);
        assert_eq!(vis.data()[1], 0.5);
        assert_eq!(vis.data()[2], 15.0 / 16.0);
        assert_eq!(vis.peak()[1], 0.5);
        assert_eq!(vis.peak()[2], 15.0 / 16.0);

        vis.set_peaks_enabled(false);
        assert!(!vis.peaks_enabled());
        assert_eq!(vis.peak()[2], 15.0 / 16.0);
    }

    #[test]
    fn visualization_tick_holds_new_data_then_decays_like_reference() {
        let mut vis = Visualization::new(WidgetId(6), 0, 0, 75);
        vis.tick(Some(&[1.0; 32]));
        assert_eq!(vis.data()[0], 15.0 / 16.0);
        assert_eq!(vis.peak()[0], 15.0 / 16.0);
        vis.tick(None);
        assert!((vis.data()[0] - 13.7 / 16.0).abs() < 0.0001);
        assert!(vis.peak()[0] < 15.0 / 16.0);
        assert!(vis.milkdrop_energy() > 0.0);
        assert!(vis.milkdrop_phase() > 0.0);
        assert!(vis.widget().needs_redraw());
        assert_eq!(Visualization::level(0.5), 8);
        assert_eq!(Visualization::level(2.0), 16);
    }

    #[test]
    fn visualization_refresh_steps_preserve_reference_falloff_rate() {
        let mut full = Visualization::new(WidgetId(6), 0, 0, 75);
        let mut quarter = full.clone();
        full.tick(Some(&[0.8]));
        for _ in 0..3 {
            full.tick(None);
        }
        quarter.tick_with_steps(Some(&[0.8]), 4);
        assert!((full.data()[0] - quarter.data()[0]).abs() < f32::EPSILON);
        assert!((full.peak()[0] - quarter.peak()[0]).abs() < f32::EPSILON);
    }

    #[test]
    fn visualization_falloff_change_requests_redraw() {
        let mut vis = Visualization::new(WidgetId(6), 0, 0, 75);
        assert!(!vis.widget().needs_redraw());

        vis.set_falloff(VisFalloffSpeed::Fastest, VisFalloffSpeed::Slowest);

        assert!(vis.widget().needs_redraw());
    }

    #[test]
    fn mono_stereo_indicator_maps_channels_to_segments() {
        let mut indicator = MonoStereoIndicator::new(WidgetId(7), 1, 2, SkinPixmapKind::MonoStereo);
        assert_eq!(indicator.widget().rect().width, 56);
        assert_eq!(indicator.widget().rect().height, 12);

        indicator.set_channels(2);
        let stereo = indicator.segments();
        assert_eq!((stereo[0].source.x, stereo[0].source.y), (29, 12));
        assert_eq!((stereo[0].dest_x, stereo[0].width), (0, 27));
        assert_eq!((stereo[1].source.x, stereo[1].source.y), (0, 0));
        assert_eq!((stereo[1].dest_x, stereo[1].width), (27, 29));
        assert!(indicator.widget().needs_redraw());

        indicator.set_channels(1);
        let mono = indicator.segments();
        assert_eq!(mono[0].source.y, 0);
        assert_eq!(mono[1].source.y, 12);

        indicator.set_channels(0);
        let inactive = indicator.segments();
        assert_eq!(inactive[0].source.y, 12);
        assert_eq!(inactive[1].source.y, 12);
        assert_eq!(inactive[0].width, 27);
        assert_eq!(inactive[1].width, 29);
    }

    #[test]
    fn play_status_indicator_maps_status_to_source_rows() {
        let mut indicator = PlayStatusIndicator::new(WidgetId(8), 1, 2, SkinPixmapKind::PlayPause);
        assert_eq!(indicator.widget().rect().width, 11);
        assert_eq!(indicator.widget().rect().height, 9);
        assert_eq!(indicator.status(), PlayStatusValue::Stopped);
        assert_eq!(indicator.source().y, 18);

        indicator.set_status(PlayStatusValue::Playing);
        assert_eq!(indicator.source().y, 0);
        assert!(indicator.widget().needs_redraw());

        indicator.set_status(PlayStatusValue::Paused);
        assert_eq!(indicator.source().y, 9);
    }

    #[test]
    fn simple_button_activates_only_on_left_release_inside() {
        let mut button = SimpleButton::new(
            WidgetId(9),
            WidgetRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        );

        button.press(2);
        assert!(!button.is_pressed());

        button.press(1);
        assert!(button.is_pressed());
        assert!(button.is_inside());
        button.motion(20, 20);
        assert!(!button.is_inside());
        assert!(!button.release(1));

        button.press(1);
        assert!(button.release(1));
        assert!(!button.is_pressed());
    }
}
