//! GTK preferences window helpers.

use super::super::*;

pub(crate) fn build_preferences_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_window: &gtk::ApplicationWindow,
    main_area: &gtk::DrawingArea,
    equalizer_window: &gtk::ApplicationWindow,
    equalizer_area: &gtk::DrawingArea,
    playlist_window: &gtk::ApplicationWindow,
    playlist_area: &gtk::DrawingArea,
) -> gtk::ApplicationWindow {
    let (default_width, default_height) = preferences_window_default_size();
    let window = skinned_application_window(
        app,
        "Preferences",
        default_width,
        default_height,
        &["xmms-preferences"],
    );

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.add_css_class("xmms-skinned-window");
    root.add_css_class("xmms-preferences");
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    root.set_margin_start(10);
    root.set_margin_end(10);

    let notebook = gtk::Notebook::new();
    notebook.set_vexpand(true);
    let preferences_changed: PreferencesChanged = Rc::new({
        let main_state = Rc::clone(main_state);
        let main_window = main_window.clone();
        let main_area = main_area.clone();
        let equalizer_window = equalizer_window.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_window = playlist_window.clone();
        let playlist_area = playlist_area.clone();
        move || {
            sync_single_panel_window_from_state(
                PanelKind::Equalizer,
                &equalizer_window,
                &equalizer_area,
                &main_state,
            );
            sync_single_panel_window_from_state(
                PanelKind::Playlist,
                &playlist_window,
                &playlist_area,
                &main_state,
            );
            resize_main_window(&main_window, &main_area, &main_state.borrow());
            main_area.queue_draw();
        }
    });

    for (page, label, page_widget) in [
        (
            PreferencesPage::Audio,
            "Audio I/O Plugins",
            build_preferences_audio_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
        (
            PreferencesPage::Visualization,
            "Visualization Plugins",
            build_preferences_visualization_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
        (
            PreferencesPage::Options,
            "Options",
            build_preferences_options_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
        (
            PreferencesPage::Fonts,
            "Fonts",
            build_preferences_fonts_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
        (
            PreferencesPage::Title,
            "Title",
            build_preferences_title_page(main_state, Some(Rc::clone(&preferences_changed))),
        ),
    ] {
        let scrolled = gtk::ScrolledWindow::new();
        scrolled.add_css_class("xmms-skinned-window");
        scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled.set_vexpand(true);
        scrolled.set_child(Some(&page_widget));
        notebook.append_page(&scrolled, Some(&gtk::Label::new(Some(label))));
        if page == PreferencesPage::Options {
            notebook.set_current_page(Some(2));
        }
    }
    {
        let main_state = Rc::clone(main_state);
        notebook.connect_switch_page(move |_notebook, _page_widget, page_num| {
            let page = match page_num {
                0 => PreferencesPage::Audio,
                1 => PreferencesPage::Visualization,
                2 => PreferencesPage::Options,
                3 => PreferencesPage::Fonts,
                _ => PreferencesPage::Title,
            };
            main_state.borrow_mut().set_preferences_page(page);
        });
    }
    root.append(&notebook);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    buttons.set_halign(gtk::Align::End);
    let reset = gtk::Button::with_label("Reset to Defaults");
    {
        let main_state = Rc::clone(main_state);
        let preferences_changed = Rc::clone(&preferences_changed);
        reset.connect_clicked(move |_| {
            main_state.borrow_mut().reset_preferences_to_defaults();
            preferences_changed();
        });
    }
    buttons.append(&reset);
    root.append(&buttons);
    window.set_child(Some(&root));

    {
        let main_state = Rc::clone(main_state);
        window.connect_close_request(move |_| {
            main_state.borrow_mut().set_preferences_visible(false);
            gtk::glib::Propagation::Proceed
        });
    }

    window
}

fn prefs_page_box() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.set_margin_top(8);
    page.set_margin_bottom(8);
    page.set_margin_start(8);
    page.set_margin_end(8);
    page
}

fn prefs_frame(title: &str, parent: &gtk::Box) -> gtk::Box {
    let frame = gtk::Frame::new(Some(title));
    frame.set_margin_top(6);
    frame.set_margin_bottom(6);
    frame.set_margin_start(6);
    frame.set_margin_end(6);
    parent.append(&frame);

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);
    box_.set_margin_start(8);
    box_.set_margin_end(8);
    frame.set_child(Some(&box_));
    box_
}

fn prefs_grid() -> gtk::Grid {
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(12);
    grid
}

fn prefs_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn prefs_attach_label(grid: &gtk::Grid, label: &str, child: &impl IsA<gtk::Widget>, row: i32) {
    grid.attach(&prefs_label(label), 0, row, 1, 1);
    grid.attach(child, 1, row, 1, 1);
    child.set_hexpand(true);
}

fn find_spin_button_by_name(root: &impl IsA<gtk::Widget>, name: &str) -> Option<gtk::SpinButton> {
    let root = root.as_ref();
    if root.widget_name() == name {
        if let Ok(spin) = root.clone().downcast::<gtk::SpinButton>() {
            return Some(spin);
        }
    }

    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(spin) = find_spin_button_by_name(&widget, name) {
            return Some(spin);
        }
        child = widget.next_sibling();
    }
    None
}

fn set_spin_value_if_changed(spin: &gtk::SpinButton, value: i32) {
    if spin.value_as_int() != value {
        spin.set_value(value as f64);
    }
}

pub(crate) fn sync_preferences_options_controls(
    preferences_window: &gtk::ApplicationWindow,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let (volume, balance) = {
        let state = main_state.borrow();
        (state.volume(), state.balance())
    };
    if let Some(spin) = find_spin_button_by_name(preferences_window, PREFERENCES_VOLUME_WIDGET) {
        set_spin_value_if_changed(&spin, volume);
    }
    if let Some(spin) = find_spin_button_by_name(preferences_window, PREFERENCES_BALANCE_WIDGET) {
        set_spin_value_if_changed(&spin, balance);
    }
}

fn prefs_check(label: &str, active: bool) -> gtk::CheckButton {
    let check = gtk::CheckButton::with_label(label);
    check.set_halign(gtk::Align::Start);
    check.set_active(active);
    check
}

fn build_preferences_audio_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let input = prefs_frame("Input Plugins", &page);
    input.append(&prefs_label("GStreamer input support (built in)"));
    input.append(&prefs_label(
        "File, URI, and stream decoding are provided by installed GStreamer plugins.",
    ));

    let output = prefs_frame("Output Plugin", &page);
    let grid = prefs_grid();
    output.append(&grid);
    let output_combo = gtk::ComboBoxText::new();
    output_combo.append(Some("auto"), "Automatic (System Default)");
    {
        let state = main_state.borrow();
        for device in state
            .output_device_groups()
            .local
            .iter()
            .chain(state.output_device_groups().network.iter())
        {
            output_combo.append(Some(&device.id), &device.display_name);
        }
    }
    if let Some(device) = main_state.borrow().preference_output_device() {
        if !output_combo.set_active_id(Some(device)) {
            output_combo.append(Some(device), device);
            output_combo.set_active_id(Some(device));
        }
    } else {
        output_combo.set_active_id(Some("auto"));
    }
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        output_combo.connect_changed(move |combo| {
            let selected = combo.active_id().map(|id| id.to_string());
            let device = selected.filter(|id| id != "auto");
            main_state.borrow_mut().set_preference_output_device(device);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Output device:", &output_combo, 0);

    let configure = gtk::Button::with_label("Configure");
    configure.connect_clicked(|_| {
        eprintln!("xmms-rs: output device configuration is handled by the system audio settings");
    });
    grid.attach(&configure, 1, 1, 1, 1);
    page
}

fn build_preferences_options_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let box_ = prefs_frame("Options", &page);
    let grid = prefs_grid();
    box_.append(&grid);

    let volume = gtk::SpinButton::with_range(0.0, 100.0, 1.0);
    volume.set_widget_name(PREFERENCES_VOLUME_WIDGET);
    volume.set_value(main_state.borrow().volume() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        volume.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_volume(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Volume:", &volume, 0);

    let balance = gtk::SpinButton::with_range(-100.0, 100.0, 1.0);
    balance.set_widget_name(PREFERENCES_BALANCE_WIDGET);
    balance.set_value(main_state.borrow().balance() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        balance.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_balance(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Balance:", &balance, 1);

    let (scale, zoom_text) = {
        let state = main_state.borrow();
        let scale = state.store.state().config.scale_factor.clamp(1.0, 5.0);
        (scale, format!("{scale:.1}x"))
    };
    let zoom = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 5.0, 0.1);
    zoom.set_digits(1);
    zoom.set_draw_value(false);
    zoom.set_value(scale);
    let zoom_value = gtk::Entry::new();
    zoom_value.set_editable(false);
    zoom_value.set_width_chars(5);
    zoom_value.set_hexpand(false);
    zoom_value.set_text(&zoom_text);
    zoom.set_hexpand(true);
    let zoom_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    zoom_box.set_hexpand(true);
    zoom_box.append(&zoom);
    zoom_box.append(&zoom_value);
    {
        let main_state = Rc::clone(main_state);
        let zoom_value = zoom_value.clone();
        let on_change = on_change.clone();
        zoom.connect_value_changed(move |scale| {
            let value = scale.value().clamp(1.0, 5.0);
            zoom_value.set_text(&format!("{value:.1}x"));
            main_state.borrow_mut().set_preference_scale_factor(value);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    grid.attach(&prefs_label("Zoom level:"), 0, 2, 2, 1);
    grid.attach(&zoom_box, 0, 3, 2, 1);

    let pause_time = gtk::SpinButton::with_range(0.0, 1000.0, 1.0);
    pause_time.set_value(main_state.borrow().preference_pause_between_songs_time() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        pause_time.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_pause_between_songs_time(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Pause between songs time (seconds):", &pause_time, 4);

    let mouse_wheel = gtk::SpinButton::with_range(1.0, 100.0, 1.0);
    mouse_wheel.set_value(main_state.borrow().preference_mouse_wheel_change() as f64);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        mouse_wheel.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_mouse_wheel_change(spin.value_as_int());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Mouse Wheel adjusts Volume by (%):", &mouse_wheel, 5);

    let checks = {
        let state = main_state.borrow();
        [
            ("Repeat", state.repeat(), PreferenceCheck::Repeat),
            ("Shuffle", state.shuffle(), PreferenceCheck::Shuffle),
            (
                "No playlist advance",
                state.preference_no_playlist_advance(),
                PreferenceCheck::NoAdvance,
            ),
            (
                "Pause between songs",
                state.preference_pause_between_songs(),
                PreferenceCheck::PauseBetweenSongs,
            ),
            (
                "Stop with fadeout",
                state.preference_stop_with_fadeout(),
                PreferenceCheck::StopWithFadeout,
            ),
            (
                "Time remaining",
                state.preference_timer_remaining(),
                PreferenceCheck::TimerRemaining,
            ),
            (
                "Dock playlist",
                !state.is_panel_detached(PanelKind::Playlist),
                PreferenceCheck::DockPlaylist,
            ),
            (
                "Dock equalizer",
                !state.is_panel_detached(PanelKind::Equalizer),
                PreferenceCheck::DockEqualizer,
            ),
            (
                "Convert %20 to space",
                state.preference_convert_twenty(),
                PreferenceCheck::ConvertTwenty,
            ),
            (
                "Convert underscore to space",
                state.preference_convert_underscore(),
                PreferenceCheck::ConvertUnderscore,
            ),
            (
                "Show numbers in playlist",
                state.preference_show_numbers_in_playlist(),
                PreferenceCheck::ShowNumbers,
            ),
            (
                "Vim-style playlist navigation",
                state.preference_vim_playlist_navigation(),
                PreferenceCheck::VimPlaylistNavigation,
            ),
        ]
    };
    for (index, (label, active, action)) in checks.into_iter().enumerate() {
        let check = prefs_check(label, active);
        {
            let main_state = Rc::clone(main_state);
            let on_change = on_change.clone();
            check.connect_toggled(move |check| {
                let mut state = main_state.borrow_mut();
                match action {
                    PreferenceCheck::Repeat => state.set_preference_repeat(check.is_active()),
                    PreferenceCheck::Shuffle => state.set_preference_shuffle(check.is_active()),
                    PreferenceCheck::NoAdvance => {
                        state.set_preference_no_playlist_advance(check.is_active())
                    }
                    PreferenceCheck::PauseBetweenSongs => {
                        state.set_preference_pause_between_songs(check.is_active())
                    }
                    PreferenceCheck::StopWithFadeout => {
                        state.set_preference_stop_with_fadeout(check.is_active())
                    }
                    PreferenceCheck::TimerRemaining => {
                        state.set_preference_timer_remaining(check.is_active())
                    }
                    PreferenceCheck::DockPlaylist => {
                        state.set_preference_playlist_docked(check.is_active())
                    }
                    PreferenceCheck::DockEqualizer => {
                        state.set_preference_equalizer_docked(check.is_active())
                    }
                    PreferenceCheck::ConvertTwenty => {
                        state.set_preference_convert_twenty(check.is_active())
                    }
                    PreferenceCheck::ConvertUnderscore => {
                        state.set_preference_convert_underscore(check.is_active())
                    }
                    PreferenceCheck::ShowNumbers => {
                        state.set_preference_show_numbers_in_playlist(check.is_active())
                    }
                    PreferenceCheck::VimPlaylistNavigation => {
                        state.set_preference_vim_playlist_navigation(check.is_active())
                    }
                }
                drop(state);
                if let Some(on_change) = &on_change {
                    on_change();
                }
            });
        }
        grid.attach(&check, (index % 2) as i32, 6 + (index / 2) as i32, 1, 1);
    }
    page
}

#[derive(Debug, Clone, Copy)]
enum PreferenceCheck {
    Repeat,
    Shuffle,
    NoAdvance,
    PauseBetweenSongs,
    StopWithFadeout,
    TimerRemaining,
    DockPlaylist,
    DockEqualizer,
    ConvertTwenty,
    ConvertUnderscore,
    ShowNumbers,
    VimPlaylistNavigation,
}

fn build_preferences_fonts_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let playlist = prefs_frame("Playlist", &page);
    let grid = prefs_grid();
    playlist.append(&grid);
    let playlist_font_size = gtk::SpinButton::with_range(6.0, 24.0, 0.5);
    playlist_font_size.set_digits(1);
    playlist_font_size.set_value(main_state.borrow().preference_playlist_font_size());
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        playlist_font_size.connect_value_changed(move |spin| {
            main_state
                .borrow_mut()
                .set_preference_playlist_font_size(spin.value());
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Playlist font size:", &playlist_font_size, 0);

    let main = prefs_frame("Main Window", &page);
    main.append(&prefs_label("Skin bitmap font"));
    main.append(&prefs_label(
        "The main window uses the active skin bitmap font, matching XMMS skins.",
    ));
    let skin_browser = gtk::Button::with_label("Open Skin Browser");
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        skin_browser.connect_clicked(move |_| {
            main_state.borrow_mut().set_skin_browser_visible(true);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    main.append(&skin_browser);
    page
}

fn build_preferences_title_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let box_ = prefs_frame("Title", &page);
    let grid = prefs_grid();
    box_.append(&grid);
    let title = gtk::Entry::new();
    title.set_text(main_state.borrow().preference_title_format());
    let preview = gtk::Label::new(Some(&format!(
        "Preview: {}",
        title_format_preview(&main_state.borrow().store.state().config)
    )));
    preview.set_halign(gtk::Align::Start);
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        let preview = preview.clone();
        title.connect_changed(move |entry| {
            main_state
                .borrow_mut()
                .set_preference_title_format(entry.text().as_str());
            preview.set_label(&format!(
                "Preview: {}",
                title_format_preview(&main_state.borrow().store.state().config)
            ));
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Title format:", &title, 0);
    box_.append(&preview);
    box_.append(&prefs_label("Original XMMS tokens include %p artist, %a album, %g genre, %f filename, and %t title. The current decoder uses embedded titles when available and stores this format for compatibility."));
    page
}

fn build_preferences_visualization_page(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    on_change: Option<PreferencesChanged>,
) -> gtk::Box {
    let page = prefs_page_box();
    let box_ = prefs_frame("Visualization", &page);
    let grid = prefs_grid();
    box_.append(&grid);
    box_.append(&prefs_label(
        "Controls that do not affect the selected visualization mode are disabled.",
    ));

    let mode = gtk::ComboBoxText::new();
    for (id, label) in [
        ("analyzer", "Analyzer"),
        ("scope", "Scope"),
        ("milkdrop", "MilkDrop-inspired"),
        ("off", "Off"),
    ] {
        mode.append(Some(id), label);
    }
    mode.set_active_id(Some(match main_state.borrow().visualization_mode() {
        VisMode::Scope => "scope",
        VisMode::Milkdrop => "milkdrop",
        VisMode::Off => "off",
        VisMode::Analyzer => "analyzer",
    }));
    prefs_attach_label(&grid, "Visualization mode:", &mode, 0);

    let analyzer_mode = gtk::ComboBoxText::new();
    for (id, label) in [
        ("normal", "Analyzer normal"),
        ("fire", "Analyzer fire"),
        ("vlines", "Analyzer vertical lines"),
    ] {
        analyzer_mode.append(Some(id), label);
    }
    analyzer_mode.set_active_id(Some(
        match main_state.borrow().visualization_analyzer_mode() {
            VisAnalyzerMode::Fire => "fire",
            VisAnalyzerMode::VerticalLines => "vlines",
            VisAnalyzerMode::Normal => "normal",
        },
    ));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        analyzer_mode.connect_changed(move |combo| {
            let mode = match combo.active_id().as_deref() {
                Some("fire") => VisAnalyzerMode::Fire,
                Some("vlines") => VisAnalyzerMode::VerticalLines,
                _ => VisAnalyzerMode::Normal,
            };
            main_state
                .borrow_mut()
                .set_visualization_analyzer_mode(mode);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Analyzer mode:", &analyzer_mode, 1);

    let style = gtk::ComboBoxText::new();
    style.append(Some("bars"), "Analyzer bars");
    style.append(Some("lines"), "Analyzer lines");
    style.set_active_id(Some(
        match main_state.borrow().visualization_analyzer_style() {
            VisAnalyzerStyle::Lines => "lines",
            VisAnalyzerStyle::Bars => "bars",
        },
    ));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        style.connect_changed(move |combo| {
            let style = match combo.active_id().as_deref() {
                Some("lines") => VisAnalyzerStyle::Lines,
                _ => VisAnalyzerStyle::Bars,
            };
            main_state
                .borrow_mut()
                .set_visualization_analyzer_style(style);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Analyzer style:", &style, 2);

    let scope = gtk::ComboBoxText::new();
    for (id, label) in [
        ("dot", "Dot scope"),
        ("line", "Line scope"),
        ("solid", "Solid scope"),
    ] {
        scope.append(Some(id), label);
    }
    scope.set_active_id(Some(match main_state.borrow().visualization_scope_mode() {
        VisScopeMode::Dot => "dot",
        VisScopeMode::Solid => "solid",
        VisScopeMode::Line => "line",
    }));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        scope.connect_changed(move |combo| {
            let mode = match combo.active_id().as_deref() {
                Some("dot") => VisScopeMode::Dot,
                Some("solid") => VisScopeMode::Solid,
                _ => VisScopeMode::Line,
            };
            main_state.borrow_mut().set_visualization_scope_mode(mode);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Scope mode:", &scope, 3);

    let peaks = prefs_check(
        "Show analyzer peaks",
        main_state.borrow().visualization_peaks_enabled(),
    );
    grid.attach(&peaks, 1, 4, 1, 1);

    let falloff = falloff_combo(main_state.borrow().visualization_analyzer_falloff());
    let peaks_falloff = falloff_combo(main_state.borrow().visualization_peaks_falloff());
    {
        let main_state = Rc::clone(main_state);
        let peaks_falloff = peaks_falloff.clone();
        let on_change = on_change.clone();
        falloff.connect_changed(move |combo| {
            let analyzer = falloff_from_combo(combo);
            let peaks = falloff_from_combo(&peaks_falloff);
            main_state
                .borrow_mut()
                .set_visualization_falloff(analyzer, peaks);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    {
        let main_state = Rc::clone(main_state);
        let falloff = falloff.clone();
        let on_change = on_change.clone();
        peaks_falloff.connect_changed(move |combo| {
            let analyzer = falloff_from_combo(&falloff);
            let peaks = falloff_from_combo(combo);
            main_state
                .borrow_mut()
                .set_visualization_falloff(analyzer, peaks);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Analyzer falloff:", &falloff, 5);
    prefs_attach_label(&grid, "Peaks falloff:", &peaks_falloff, 6);

    let vu = gtk::ComboBoxText::new();
    vu.append(Some("segmented"), "Segmented");
    vu.append(Some("smooth"), "Smooth");
    vu.set_active_id(Some(match main_state.borrow().visualization_vu_mode() {
        VisVuMode::Smooth => "smooth",
        VisVuMode::Segmented => "segmented",
    }));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        vu.connect_changed(move |combo| {
            let mode = match combo.active_id().as_deref() {
                Some("smooth") => VisVuMode::Smooth,
                _ => VisVuMode::Segmented,
            };
            main_state.borrow_mut().set_visualization_vu_mode(mode);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Window Shade Level Meter Style:", &vu, 7);

    let refresh = gtk::ComboBoxText::new();
    for (id, label) in [
        ("full", "Full"),
        ("half", "Half"),
        ("quarter", "Quarter"),
        ("eighth", "Eighth"),
    ] {
        refresh.append(Some(id), label);
    }
    refresh.set_active_id(Some(
        match main_state.borrow().visualization_refresh_divisor() {
            8.. => "eighth",
            4..=7 => "quarter",
            2..=3 => "half",
            _ => "full",
        },
    ));
    {
        let main_state = Rc::clone(main_state);
        let on_change = on_change.clone();
        refresh.connect_changed(move |combo| {
            let divisor = match combo.active_id().as_deref() {
                Some("eighth") => 8,
                Some("quarter") => 4,
                Some("half") => 2,
                _ => 1,
            };
            main_state
                .borrow_mut()
                .set_visualization_refresh_divisor(divisor);
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    prefs_attach_label(&grid, "Refresh rate:", &refresh, 8);

    update_visualization_preference_sensitivity(
        &mode,
        &analyzer_mode,
        &style,
        &scope,
        &peaks,
        &falloff,
        &peaks_falloff,
        &vu,
        &refresh,
    );
    {
        let main_state = Rc::clone(main_state);
        let analyzer_mode = analyzer_mode.clone();
        let style = style.clone();
        let scope = scope.clone();
        let peaks = peaks.clone();
        let falloff = falloff.clone();
        let peaks_falloff = peaks_falloff.clone();
        let vu = vu.clone();
        let refresh = refresh.clone();
        let on_change = on_change.clone();
        mode.connect_changed(move |combo| {
            let mode = match combo.active_id().as_deref() {
                Some("scope") => VisMode::Scope,
                Some("milkdrop") => VisMode::Milkdrop,
                Some("off") => VisMode::Off,
                _ => VisMode::Analyzer,
            };
            main_state.borrow_mut().set_visualization_mode(mode);
            update_visualization_preference_sensitivity(
                combo,
                &analyzer_mode,
                &style,
                &scope,
                &peaks,
                &falloff,
                &peaks_falloff,
                &vu,
                &refresh,
            );
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    {
        let main_state = Rc::clone(main_state);
        let mode = mode.clone();
        let analyzer_mode = analyzer_mode.clone();
        let style = style.clone();
        let scope = scope.clone();
        let falloff = falloff.clone();
        let peaks_falloff = peaks_falloff.clone();
        let vu = vu.clone();
        let refresh = refresh.clone();
        let on_change = on_change.clone();
        peaks.connect_toggled(move |check| {
            main_state
                .borrow_mut()
                .set_visualization_peaks_enabled(check.is_active());
            update_visualization_preference_sensitivity(
                &mode,
                &analyzer_mode,
                &style,
                &scope,
                check,
                &falloff,
                &peaks_falloff,
                &vu,
                &refresh,
            );
            if let Some(on_change) = &on_change {
                on_change();
            }
        });
    }
    page
}

fn update_visualization_preference_sensitivity(
    mode: &gtk::ComboBoxText,
    analyzer_mode: &gtk::ComboBoxText,
    analyzer_style: &gtk::ComboBoxText,
    scope_mode: &gtk::ComboBoxText,
    peaks: &gtk::CheckButton,
    analyzer_falloff: &gtk::ComboBoxText,
    peaks_falloff: &gtk::ComboBoxText,
    vu: &gtk::ComboBoxText,
    refresh: &gtk::ComboBoxText,
) {
    let mode = match mode.active_id().as_deref() {
        Some("scope") => VisMode::Scope,
        Some("milkdrop") => VisMode::Milkdrop,
        Some("off") => VisMode::Off,
        _ => VisMode::Analyzer,
    };
    let sensitivity = visualization_preference_sensitivity(mode, peaks.is_active());
    analyzer_mode.set_sensitive(sensitivity.analyzer_mode);
    analyzer_style.set_sensitive(sensitivity.analyzer_style);
    peaks.set_sensitive(sensitivity.analyzer_peaks);
    analyzer_falloff.set_sensitive(sensitivity.analyzer_falloff);
    peaks_falloff.set_sensitive(sensitivity.peaks_falloff);
    scope_mode.set_sensitive(sensitivity.scope_mode);
    vu.set_sensitive(sensitivity.windowshade_vu);
    refresh.set_sensitive(sensitivity.refresh_rate);
}

fn falloff_combo(active: VisFalloffSpeed) -> gtk::ComboBoxText {
    let combo = gtk::ComboBoxText::new();
    for (id, label) in [
        ("slowest", "Slowest"),
        ("slow", "Slow"),
        ("medium", "Medium"),
        ("fast", "Fast"),
        ("fastest", "Fastest"),
    ] {
        combo.append(Some(id), label);
    }
    combo.set_active_id(Some(falloff_id(active)));
    combo
}

fn falloff_id(speed: VisFalloffSpeed) -> &'static str {
    match speed {
        VisFalloffSpeed::Slowest => "slowest",
        VisFalloffSpeed::Slow => "slow",
        VisFalloffSpeed::Fast => "fast",
        VisFalloffSpeed::Fastest => "fastest",
        VisFalloffSpeed::Medium => "medium",
    }
}

fn falloff_from_combo(combo: &gtk::ComboBoxText) -> VisFalloffSpeed {
    match combo.active_id().as_deref() {
        Some("slowest") => VisFalloffSpeed::Slowest,
        Some("slow") => VisFalloffSpeed::Slow,
        Some("fast") => VisFalloffSpeed::Fast,
        Some("fastest") => VisFalloffSpeed::Fastest,
        _ => VisFalloffSpeed::Medium,
    }
}
