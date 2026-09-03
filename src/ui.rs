use std::cell::{Cell, RefCell};
use std::fmt;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use gtk::prelude::*;

use crate::app::command::{
    AppCommand, AudioCommand, EqualizerCommand, PanelCommand, PlayerCommand, PlaylistCommand,
    UiCommand,
};
use crate::app::effect::AppEffect;
pub use crate::app::equalizer_actions::EqualizerPresetAction;
use crate::app::equalizer_actions::EQUALIZER_PRESET_FILE_ITEMS;
use crate::app::input::{AppShortcut, APP_SHORTCUTS};
pub use crate::app::panel::PanelKind;
use crate::app::panel::{PanelState, PanelVisibility};
pub use crate::app::playlist_actions::PlaylistSortAction;
use crate::app::playlist_actions::{
    playlist_queue_target_indices, playlist_row_click_commands, PlaylistMenuCommand,
    PLAYLIST_SORT_MENU_ITEMS,
};
use crate::app::preferences_model::{
    normalize_title_format, set_scale_factor as set_config_scale_factor, title_format_preview,
};
use crate::app::preview::{
    apply_preview_options_to_config, apply_preview_playlist, PreviewOptions,
};
use crate::app::store::{AppStore, DispatchResult, StateChangeSet};
use crate::app::view_model::{
    balance_to_eq_shaded_position, balance_to_position, ellipsize_chars,
    eq_shaded_position_to_balance, eq_shaded_position_to_volume, eq_slider_pixel_to_position,
    eq_slider_position_to_pixel, format_duration,
    formatted_current_title as shared_formatted_current_title,
    formatted_playlist_entry_title as shared_formatted_playlist_entry_title, parse_time_ms,
    playlist_footer_info as shared_playlist_footer_info, playlist_menu_at, playlist_menu_rect,
    playlist_rows_render_state as shared_playlist_rows_render_state, position_to_balance,
    position_to_volume, scale_event_coords, volume_to_eq_shaded_position, volume_to_position,
    TitleMarquee,
};
use crate::app_state::AppState;
use crate::audio_model::{equalizer_position_to_db, EqualizerBandDb, SpectrumLayout};
use crate::config::{Config, TimerMode};
use crate::equalizer::{
    built_in_equalizer_presets, default_equalizer_presets, find_preset, load_preset_store,
    load_winamp_eqf_first, load_xmms_preset_file, preset_store_path, save_winamp_eqf,
    EqualizerPreset,
};
use crate::mpris::{
    app_action_for_mpris_command, gio_service::MprisService, mpris_player_properties,
    mpris_root_properties, MprisAppAction, MprisCommand, MprisEvent, MprisPlayerProperties,
    MprisRootProperties,
};
use crate::playback::backend::{create_backend, PlaybackBackend, PlaybackBackendKind};
use crate::playback::model::{
    EqualizerBackendState, OutputDevice, OutputDeviceGroups, OutputDeviceSelection, PlaybackEvent,
    PlayerState,
};
use crate::player::group_output_devices;
pub use crate::playlist::PlaylistMenuKind;
use crate::{app_log_debug, app_log_info, app_log_trace};
use gtk::cairo;

use crate::playlist::{
    file_uri_to_path, DurationIndexResult, Playlist, PlaylistRevisions, PlaylistSortKey,
};
use crate::render::{
    docked_panel_size, equalizer_window_height, main_window_height, paint_scaled,
    playlist_window_height, render_equalizer_state, render_main_player_state,
    render_playlist_frame, render_playlist_menu, render_playlist_rows, render_scaled, scale_dim,
    surface_from_xpm, Context, DockedPanelState, EqualizerControl, EqualizerRenderState, Format,
    ImageSurface, MainPushButton, MainSlider, MainToggleButton, MainWindowRenderState,
    PlaylistMenuRenderKind, PlaylistMenuRenderState, PlaylistRowsRenderState, RenderPass,
    VisualizationRenderState, EQUALIZER_WINDOW_HEIGHT, EQUALIZER_WINDOW_WIDTH,
    MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH, PLAYLIST_DEFAULT_HEIGHT,
    PLAYLIST_DEFAULT_WIDTH, PLAYLIST_MIN_HEIGHT, PLAYLIST_MIN_WIDTH,
};
use crate::session::{
    default_config_dir, fallback_state_paths, load_saved_state, save_fallback_state,
};
use crate::skin::layout::{
    equalizer_control_at, equalizer_shaded_slider_at, equalizer_slider_at, equalizer_slider_layout,
    main_push_button_rect, main_slider_layout, main_toggle_button_rect, panel_title_button_at,
    playlist_footer_button_at, snap_playlist_size, EqualizerSlider, LayoutPanelKind as LayoutPanel,
    PanelTitleButton, PlaylistFooterButton, SkinRect,
};
use crate::skin::widget::{
    NumberDisplay, PlayStatusValue, VisAnalyzerMode, VisAnalyzerStyle, VisFalloffSpeed, VisMode,
    VisScopeMode, VisVuMode, Visualization, WidgetId,
};
use crate::skin::{
    discover_skins_in_dirs, skin_browser_search_dirs, DefaultSkin, SkinEntry, SkinPixmapKind,
};
use crate::skineditor::{
    ElementSlot, SkinEditorState, SkinGradient, Tool, COLOR_SHELF_SIZE, GRADIENT_SHELF_SIZE,
    MAX_ZOOM, MIN_ZOOM, ZOOM_STEP,
};
use crate::socket_control::{
    start_socket_control_with_wakeup, SocketCommand, SocketControl, SocketRequest, SocketUiCommand,
};

pub(crate) mod file_info;
#[path = "ui/gtk/mod.rs"]
pub(crate) mod gtk_frontend;
mod style;

use file_info::{file_info_details_for_entry, show_file_info_dialog, FileInfoDetails};
use gtk_frontend::equalizer_window::{
    build_equalizer_presets_popover, show_docked_equalizer_presets_menu,
    show_equalizer_presets_menu,
};
use gtk_frontend::playlist_menu::{build_playlist_sort_popover, show_playlist_sort_menu};
use gtk_frontend::playlist_window::build_playlist_window;
use gtk_frontend::preferences::{build_preferences_window, sync_preferences_options_controls};
use style::{
    refresh_xmms_skin_css, style_color_shelf_button, style_skin_color_button,
    style_skin_editor_custom_color_button,
};

type SharedPlaybackBackend = Rc<RefCell<Box<dyn PlaybackBackend>>>;

const DEFAULT_SCALE: i32 = 2;
const GTK_TRANSITION_TICK: Duration = Duration::from_millis(20);
const GTK_MARQUEE_TICK: Duration = Duration::from_millis(84);
const GTK_PLAYBACK_TICK: Duration = Duration::from_millis(250);
const GTK_PAUSED_TICK: Duration = Duration::from_millis(500);
const GTK_IDLE_TICK: Duration = Duration::from_secs(1);
const STOP_FADE_DURATION_MS: i64 = 1_000;
type PreferencesChanged = Rc<dyn Fn()>;
const PREFERENCES_VOLUME_WIDGET: &str = "xmms-preferences-volume";
const PREFERENCES_BALANCE_WIDGET: &str = "xmms-preferences-balance";
const SKIN_BROWSER_ROOT_WIDGET: &str = "xmms-skin-browser-root";
const SKIN_BROWSER_HEADER_WIDGET: &str = "xmms-skin-browser-header";
const SKIN_BROWSER_LIST_WIDGET: &str = "xmms-skin-browser-list";
const SKIN_BROWSER_ADD_WIDGET: &str = "xmms-skin-browser-add";
const SKIN_BROWSER_CLOSE_WIDGET: &str = "xmms-skin-browser-close";
const SKIN_EDITOR_COLOR_SHELF_COLUMNS: usize = 8;
const SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE: i32 = 34;
const SKIN_EDITOR_COLOR_SHELF_GAP: i32 = 4;
const SKIN_EDITOR_SIDEBAR_WIDTH: i32 = SKIN_EDITOR_COLOR_SHELF_COLUMNS as i32
    * SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE
    + (SKIN_EDITOR_COLOR_SHELF_COLUMNS as i32 - 1) * SKIN_EDITOR_COLOR_SHELF_GAP;
const SKIN_EDITOR_GRADIENT_WIDTH: i32 = SKIN_EDITOR_SIDEBAR_WIDTH;
const SKIN_EDITOR_GRADIENT_HEIGHT: i32 = 34;

pub fn run_default_skin_preview(options: PreviewOptions) {
    run_preview_application(PreviewMode::Interactive, options);
}

pub fn run_default_skin_preview_smoke(options: PreviewOptions) {
    run_preview_application(PreviewMode::Smoke, options);
}

pub fn write_player_screenshot(options: PreviewOptions, path: &Path) -> Result<(), String> {
    let state = preview_state_from_options(options)?;
    let docked_state = state.docked_panel_state();
    let (width, height) = docked_panel_size(docked_state);
    let mut surface = ImageSurface::create(Format::ARgb32, width, height)
        .map_err(|err| format!("failed to create screenshot surface: {err}"))?;
    let cr = Context::new(&surface)
        .map_err(|err| format!("failed to create screenshot context: {err}"))?;
    render_docked_ui_state(&cr, state.active_skin(), &state, RenderPass::Bitmap)
        .map_err(|err| format!("failed to render screenshot: {err}"))?;
    render_docked_ui_state(&cr, state.active_skin(), &state, RenderPass::Text)
        .map_err(|err| format!("failed to render screenshot: {err}"))?;
    drop(cr);
    write_surface_png(&mut surface, path)
        .map_err(|err| format!("failed to write screenshot '{}': {err}", path.display()))
}

enum PreviewMode {
    Interactive,
    Smoke,
}

fn run_preview_application(mode: PreviewMode, options: PreviewOptions) {
    let mut flags = gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    if std::env::var_os("XMMS_NON_UNIQUE").is_some() {
        flags |= gtk::gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = gtk::Application::builder()
        .application_id("org.xmms.Renascene.RustPreview")
        .flags(flags)
        .register_session(true)
        .build();

    app.connect_command_line(|app, _cmdline| {
        app.activate();
        gtk::glib::ExitCode::SUCCESS
    });

    app.connect_activate(move |app| {
        let persist_session = matches!(mode, PreviewMode::Interactive);
        if let Err(err) = build_preview_window(app, options.clone(), persist_session) {
            eprintln!("xmms-rs: failed to create GTK preview: {err}");
            app.quit();
            return;
        }

        if matches!(mode, PreviewMode::Smoke) {
            let app = app.clone();
            gtk::glib::idle_add_local_once(move || app.quit());
        }
    });

    app.run_with_args(&["xmms-rs"]);
}

fn build_preview_window(
    app: &gtk::Application,
    options: PreviewOptions,
    persist_session: bool,
) -> Result<(), String> {
    crate::perf_span!("gtk_startup");
    let (config_path, playlist_path) = fallback_state_paths(&default_config_dir());
    let app_state = if persist_session {
        load_saved_state(&config_path, &playlist_path, options.reset)
            .map_err(|err| format!("failed to load saved state: {err}"))?
    } else {
        AppState::default()
    };
    let open_preferences = options.open_preferences;
    let open_skin_editor = options.open_skin_editor;
    let socket_port = options.socket_port;
    let mut state = preview_state_from_app_state(app_state, options)?;
    if let Some(config_dir) = config_path.parent() {
        state.set_equalizer_preset_dir(config_dir.to_path_buf());
    }
    let (initial_width, initial_height) = state.docked_panel_size();
    let initial_scale = state.scale_factor();
    let initial_device_width = scale_dim(initial_width, initial_scale);
    let initial_device_height = scale_dim(initial_height, initial_scale);
    let main_state = Rc::new(RefCell::new(state));
    refresh_xmms_skin_css(main_state.borrow().active_skin());

    crate::perf_span!("gtk_window_build");
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Renascene Rust Preview")
        .resizable(false)
        .decorated(false)
        .default_width(initial_device_width)
        .default_height(initial_device_height)
        .build();
    if persist_session {
        let main_state = Rc::clone(&main_state);
        let config_path = config_path.clone();
        let playlist_path = playlist_path.clone();
        window.connect_close_request(move |_| {
            if let Err(err) = main_state
                .borrow_mut()
                .save_runtime_snapshot(&config_path, &playlist_path)
            {
                eprintln!("xmms-rs: failed to save session: {err}");
            }
            gtk::glib::Propagation::Proceed
        });
    }
    if persist_session {
        let main_state = Rc::clone(&main_state);
        let config_path = config_path.clone();
        let playlist_path = playlist_path.clone();
        app.connect_shutdown(move |_| {
            if let Err(err) = main_state
                .borrow_mut()
                .save_runtime_snapshot(&config_path, &playlist_path)
            {
                eprintln!("xmms-rs: failed to save session: {err}");
            }
        });
    }

    let drawing_area = gtk::DrawingArea::builder()
        .content_width(initial_device_width)
        .content_height(initial_device_height)
        .focusable(true)
        .build();
    let panel_windows = Rc::new(PanelWindows::new(app, &main_state, &drawing_area, &window));
    let socket_wakeup_action = gtk::gio::SimpleAction::new("socket-control-wakeup", None);
    app.add_action(&socket_wakeup_action);
    let socket_wakeup = gtk::glib::SendWeakRef::from(socket_wakeup_action.downgrade());
    let socket_control = socket_port
        .map(|port| {
            start_socket_control_with_wakeup(port, move || {
                let socket_wakeup = socket_wakeup.clone();
                gtk::glib::idle_add_once(move || {
                    if let Some(action) = socket_wakeup.upgrade() {
                        action.activate(None);
                    }
                });
            })
        })
        .transpose()?;
    let socket_control = Rc::new(socket_control);
    let mpris_service = Rc::new(MprisService::own_session_bus(Rc::clone(&main_state)));
    sync_panel_windows(&panel_windows, &main_state.borrow());
    resize_main_window(&window, &drawing_area, &main_state.borrow());
    let menu_popover = Rc::new(build_main_menu_popover(
        app,
        &window,
        &drawing_area,
        &panel_windows.preferences,
        &panel_windows.open_location,
        &panel_windows.skin_browser,
        &panel_windows.skin_editor,
        &main_state,
    ));
    let playlist_sort_popover = Rc::new(build_playlist_sort_popover(
        &drawing_area,
        &main_state,
        &drawing_area,
    ));
    let equalizer_presets_popover = Rc::new(build_equalizer_presets_popover(
        &drawing_area,
        &main_state,
        &drawing_area,
    ));
    let runtime_context = GtkRuntimeTickContext {
        app: app.clone(),
        window: window.clone(),
        drawing_area: drawing_area.clone(),
        panel_windows: Rc::clone(&panel_windows),
        menu_popover: Rc::clone(&menu_popover),
        main_state: Rc::clone(&main_state),
        mpris_service: Rc::clone(&mpris_service),
        socket_control: Rc::clone(&socket_control),
        last_tick: Rc::new(Cell::new(Instant::now())),
    };
    {
        let runtime_context = runtime_context.clone();
        socket_wakeup_action.connect_activate(move |_action, _parameter| {
            process_gtk_socket_control(&runtime_context, true);
        });
    }

    {
        let render_cache = Rc::new(RefCell::new(GtkRenderSurfaceCache::default()));
        let main_state = Rc::clone(&main_state);
        drawing_area.set_draw_func(move |_area, cr, width, height| {
            let state = main_state.borrow();
            let docked_state = state.docked_panel_state();
            let (base_width, base_height) = docked_panel_size(docked_state);
            let render_key = state.docked_render_key();
            match render_scaled_to_gtk_cached(
                &mut render_cache.borrow_mut(),
                render_key,
                cr,
                width,
                height,
                base_width,
                base_height,
                |cr, pass| {
                    render_docked_ui_state(cr, state.active_skin(), &state, pass).map(|_| ())
                },
            ) {
                Ok(()) => {
                    app_log_trace!(
                        render,
                        "gtk main-window",
                        width,
                        height,
                        base_width,
                        base_height
                    )
                }
                Err(err) => eprintln!("xmms-rs: failed to render main-window preview: {err}"),
            }
        });
    }

    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let drawing_area = drawing_area.clone();
        let window = window.clone();
        let main_state = Rc::clone(&main_state);
        let panel_windows = Rc::clone(&panel_windows);
        click.connect_pressed(move |gesture, n_press, x, y| {
            let (base_x, base_y) = event_to_base_coords(&drawing_area, &main_state.borrow(), x, y);
            let docked_panel = { main_state.borrow().docked_panel_at(base_x, base_y) };
            if let Some((kind, panel_x, panel_y)) = docked_panel {
                main_state.borrow_mut().select_docked_panel(kind);
                if n_press >= 2
                    && kind == PanelKind::Equalizer
                    && main_state
                        .borrow()
                        .panel_title_drag_region(kind, panel_x, panel_y)
                {
                    main_state.borrow_mut().toggle_equalizer_shaded();
                    sync_panel_windows(&panel_windows, &main_state.borrow());
                    resize_main_window(&window, &drawing_area, &main_state.borrow());
                    drawing_area.queue_draw();
                    return;
                }
                match kind {
                    PanelKind::Equalizer => {
                        if main_state.borrow_mut().equalizer_press(panel_x, panel_y) {
                            drawing_area.queue_draw();
                        }
                    }
                    PanelKind::Playlist => {
                        if n_press >= 2
                            && main_state
                                .borrow_mut()
                                .activate_playlist_entry_at(panel_x, panel_y)
                        {
                            drawing_area.queue_draw();
                            return;
                        }
                        let ctrl_pressed = gesture
                            .current_event_state()
                            .contains(gtk::gdk::ModifierType::CONTROL_MASK);
                        let pressed = main_state.borrow_mut().playlist_press_with_ctrl(
                            panel_x,
                            panel_y,
                            ctrl_pressed,
                        ) || main_state
                            .borrow_mut()
                            .playlist_scrollbar_press(panel_x, panel_y);
                        if pressed {
                            drawing_area.queue_draw();
                        }
                        if !pressed
                            && main_state.borrow().playlist_resize_region(panel_x, panel_y)
                            && main_state
                                .borrow_mut()
                                .begin_docked_playlist_resize(panel_y)
                        {
                            drawing_area.queue_draw();
                        }
                    }
                }
                return;
            }
            main_state.borrow_mut().select_docked_main();
            drawing_area.queue_draw();
            if main_state.borrow().main_title_drag_region(base_x, base_y) {
                let Some(device) = gesture.current_event_device() else {
                    return;
                };
                let Some(surface) = window.surface() else {
                    return;
                };
                let Ok(toplevel) = surface.downcast::<gtk::gdk::Toplevel>() else {
                    return;
                };
                toplevel.begin_move(
                    &device,
                    gesture.current_button() as i32,
                    x,
                    y,
                    gesture.current_event_time(),
                );
                return;
            }
            main_state.borrow_mut().press(base_x, base_y);
            sync_preferences_options_controls(&panel_windows.preferences, &main_state);
            drawing_area.queue_draw();
        });
    }
    {
        let app = app.clone();
        let window = window.clone();
        let drawing_area = drawing_area.clone();
        let menu_popover = Rc::clone(&menu_popover);
        let playlist_sort_popover = Rc::clone(&playlist_sort_popover);
        let equalizer_presets_popover = Rc::clone(&equalizer_presets_popover);
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        click.connect_released(move |_gesture, _n_press, x, y| {
            let (x, y) = event_to_base_coords(&drawing_area, &main_state.borrow(), x, y);
            if main_state.borrow_mut().end_docked_playlist_resize() {
                sync_panel_windows(&panel_windows, &main_state.borrow());
                resize_main_window(&window, &drawing_area, &main_state.borrow());
                drawing_area.queue_draw();
                return;
            }
            let docked_panel = { main_state.borrow().docked_panel_at(x, y) };
            if let Some((kind, panel_x, panel_y)) = docked_panel {
                let action = {
                    let mut state = main_state.borrow_mut();
                    match kind {
                        PanelKind::Equalizer => {
                            let title_action = state.panel_click(kind, panel_x, panel_y);
                            if title_action == PanelAction::None {
                                state.equalizer_release(panel_x, panel_y)
                            } else {
                                title_action
                            }
                        }
                        PanelKind::Playlist => {
                            if state.playlist_scrollbar_release() {
                                PanelAction::Changed
                            } else if state.playlist_menu_pressed() {
                                state.playlist_release(panel_x, panel_y)
                            } else if state.playlist_entry_release() {
                                PanelAction::Changed
                            } else {
                                state.panel_click(kind, panel_x, panel_y)
                            }
                        }
                    }
                };
                handle_panel_action_for_main_window(
                    action,
                    &window,
                    &drawing_area,
                    &panel_windows,
                    &main_state,
                    &playlist_sort_popover,
                    &equalizer_presets_popover,
                );
                drawing_area.queue_draw();
                return;
            }
            let action = main_state.borrow_mut().release(x, y);
            apply_ui_action(
                action,
                &app,
                &window,
                &drawing_area,
                &menu_popover,
                &main_state,
            );
            sync_preferences_options_controls(&panel_windows.preferences, &main_state);
            sync_panel_windows(&panel_windows, &main_state.borrow());
            resize_main_window(&window, &drawing_area, &main_state.borrow());
            drawing_area.queue_draw();
        });
    }
    window.add_controller(click);

    let docked_playlist_resize_drag = Rc::new(Cell::new(None::<(i32, f64)>));
    let resize_drag = gtk::GestureDrag::new();
    resize_drag.set_button(1);
    resize_drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let drawing_area = drawing_area.clone();
        let main_state = Rc::clone(&main_state);
        let docked_playlist_resize_drag = Rc::clone(&docked_playlist_resize_drag);
        resize_drag.connect_drag_begin(move |gesture, start_x, start_y| {
            let mut state = main_state.borrow_mut();
            let (base_x, base_y) = event_to_base_coords(&drawing_area, &state, start_x, start_y);
            let Some((PanelKind::Playlist, panel_x, panel_y)) =
                state.docked_panel_at(base_x, base_y)
            else {
                docked_playlist_resize_drag.set(None);
                return;
            };
            if !state.playlist_resize_region(panel_x, panel_y) {
                docked_playlist_resize_drag.set(None);
                return;
            }
            let start_height = state.playlist_ui.height;
            let scale = state.scale_factor();
            if state.begin_docked_playlist_resize(panel_y) {
                docked_playlist_resize_drag.set(Some((start_height, scale)));
                gesture.set_state(gtk::EventSequenceState::Claimed);
                drawing_area.queue_draw();
            }
        });
    }
    {
        let drawing_area = drawing_area.clone();
        let window = window.clone();
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let docked_playlist_resize_drag = Rc::clone(&docked_playlist_resize_drag);
        resize_drag.connect_drag_update(move |_gesture, _offset_x, offset_y| {
            let Some((start_height, scale)) = docked_playlist_resize_drag.get() else {
                return;
            };
            let base_delta_y = (offset_y / scale).round() as i32;
            let changed = main_state
                .borrow_mut()
                .set_playlist_size(PLAYLIST_MIN_WIDTH, start_height + base_delta_y);
            if changed {
                sync_panel_windows(&panel_windows, &main_state.borrow());
                resize_main_window(&window, &drawing_area, &main_state.borrow());
                drawing_area.queue_draw();
            }
        });
    }
    {
        let drawing_area = drawing_area.clone();
        let window = window.clone();
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let docked_playlist_resize_drag = Rc::clone(&docked_playlist_resize_drag);
        resize_drag.connect_drag_end(move |_gesture, _offset_x, _offset_y| {
            if docked_playlist_resize_drag.take().is_none() {
                return;
            }
            main_state.borrow_mut().end_docked_playlist_resize();
            sync_panel_windows(&panel_windows, &main_state.borrow());
            resize_main_window(&window, &drawing_area, &main_state.borrow());
            drawing_area.queue_draw();
        });
    }
    window.add_controller(resize_drag);

    let motion = gtk::EventControllerMotion::new();
    motion.set_propagation_phase(gtk::PropagationPhase::Capture);
    let main_hover_base = Rc::new(Cell::new(None::<(i32, i32)>));
    {
        let drawing_area = drawing_area.clone();
        let window = window.clone();
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let main_hover_base = Rc::clone(&main_hover_base);
        let docked_playlist_resize_drag = Rc::clone(&docked_playlist_resize_drag);
        motion.connect_motion(move |_motion, x, y| {
            let (x, y) = event_to_base_coords(&drawing_area, &main_state.borrow(), x, y);
            main_hover_base.set(Some((x, y)));
            if docked_playlist_resize_drag.get().is_none()
                && main_state.borrow().is_docked_playlist_resizing()
            {
                if main_state.borrow_mut().docked_playlist_resize_motion(y) {
                    sync_panel_windows(&panel_windows, &main_state.borrow());
                    resize_main_window(&window, &drawing_area, &main_state.borrow());
                    drawing_area.queue_draw();
                }
                return;
            }
            let docked_panel = { main_state.borrow().docked_panel_at(x, y) };
            if let Some((kind, panel_x, panel_y)) = docked_panel {
                let changed = match kind {
                    PanelKind::Equalizer => {
                        main_state.borrow_mut().equalizer_motion(panel_x, panel_y)
                    }
                    PanelKind::Playlist => {
                        let scrolled = main_state
                            .borrow_mut()
                            .playlist_scrollbar_motion(panel_x, panel_y);
                        let menu_changed =
                            main_state.borrow_mut().playlist_motion(panel_x, panel_y);
                        scrolled || menu_changed
                    }
                };
                if changed {
                    drawing_area.queue_draw();
                }
                return;
            }
            if main_state.borrow_mut().motion(x, y) {
                sync_preferences_options_controls(&panel_windows.preferences, &main_state);
                drawing_area.queue_draw();
            }
        });
    }
    window.add_controller(motion);

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let drawing_area = drawing_area.clone();
        let window = window.clone();
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let main_hover_base = Rc::clone(&main_hover_base);
        scroll.connect_scroll(move |scroll, _dx, dy| {
            let hover = main_hover_base.get().or_else(|| {
                scroll
                    .current_event()
                    .and_then(|event| event.position())
                    .map(|(event_x, event_y)| {
                        event_to_base_coords(&drawing_area, &main_state.borrow(), event_x, event_y)
                    })
            });
            let Some((x, y)) = hover else {
                return gtk::glib::Propagation::Proceed;
            };
            if main_state.borrow_mut().scroll_main(x, y, dy) {
                sync_panel_windows(&panel_windows, &main_state.borrow());
                resize_main_window(&window, &drawing_area, &main_state.borrow());
                drawing_area.queue_draw();
                panel_windows.playlist_area.queue_draw();
                panel_windows.equalizer_area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(scroll);

    add_file_drop_controller(&drawing_area, Rc::clone(&main_state), true, true);

    let key_controller = gtk::EventControllerKey::new();
    {
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        let window = window.clone();
        let drawing_area = drawing_area.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
            if focus_cycle_shortcut(key, state) {
                main_state.borrow_mut().cycle_visible_focus();
                drawing_area.queue_draw();
                panel_windows.playlist_area.queue_draw();
                panel_windows.equalizer_area.queue_draw();
                return gtk::glib::Propagation::Stop;
            }
            if handle_main_playlist_key_pressed(&main_state, key, state) {
                drawing_area.queue_draw();
                sync_panel_windows(&panel_windows, &main_state.borrow());
                resize_main_window(&window, &drawing_area, &main_state.borrow());
                return gtk::glib::Propagation::Stop;
            }
            let Some(shortcut) = keyboard_shortcut_from_event(key, state) else {
                return gtk::glib::Propagation::Proceed;
            };
            handle_keyboard_shortcut(
                shortcut,
                &window,
                &drawing_area,
                &panel_windows,
                &main_state,
            );
            gtk::glib::Propagation::Stop
        });
    }
    window.add_controller(key_controller);

    {
        let panel_windows = Rc::clone(&panel_windows);
        let main_state = Rc::clone(&main_state);
        window.connect_is_active_notify(move |window| {
            if window.is_active() {
                let mut state = main_state.borrow_mut();
                state.set_panel_focused(PanelKind::Equalizer, false);
                state.set_panel_focused(PanelKind::Playlist, false);
                panel_windows.equalizer_area.queue_draw();
                panel_windows.playlist_area.queue_draw();
            }
        });
    }

    schedule_gtk_runtime_tick(runtime_context, GTK_TRANSITION_TICK);

    window.set_child(Some(&drawing_area));
    window.present();
    if persist_session {
        let main_state = Rc::clone(&main_state);
        gtk::glib::idle_add_local_once(move || {
            crate::perf_span!("gtk_backend_init");
            match create_backend(PlaybackBackendKind::Auto) {
                Ok(backend) => main_state
                    .borrow_mut()
                    .set_playback_backend(Rc::new(RefCell::new(backend))),
                Err(err) => eprintln!("xmms-rs: audio playback backend unavailable: {err}"),
            }
        });
    }
    present_visible_panel_windows(&panel_windows, &main_state.borrow());
    if open_preferences {
        main_state.borrow_mut().set_preferences_visible(true);
        panel_windows.preferences.present();
    }
    if open_skin_editor {
        main_state.borrow_mut().set_skin_editor_visible(true);
        panel_windows.skin_editor.present();
    }
    Ok(())
}

fn handle_mpris_quit_request(events: &[MprisEvent], quit: impl FnOnce()) -> bool {
    if events.contains(&MprisEvent::QuitRequested) {
        quit();
        true
    } else {
        false
    }
}

#[derive(Clone)]
struct GtkRuntimeTickContext {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    drawing_area: gtk::DrawingArea,
    panel_windows: Rc<PanelWindows>,
    menu_popover: Rc<gtk::Popover>,
    main_state: Rc<RefCell<MainWindowUiState>>,
    mpris_service: Rc<MprisService>,
    socket_control: Rc<Option<SocketControl>>,
    last_tick: Rc<Cell<Instant>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GtkTickRedraw {
    main: bool,
    playlist: bool,
    equalizer: bool,
}

impl GtkTickRedraw {
    fn from_changes(changes: StateChangeSet) -> Self {
        Self {
            main: changes.intersects(StateChangeSet::RENDER_MAIN),
            playlist: changes.intersects(StateChangeSet::RENDER_PLAYLIST),
            equalizer: changes.intersects(StateChangeSet::RENDER_EQUALIZER),
        }
    }

    fn merge(&mut self, other: Self) {
        self.main |= other.main;
        self.playlist |= other.playlist;
        self.equalizer |= other.equalizer;
    }

    fn any(self) -> bool {
        self.main || self.playlist || self.equalizer
    }
}

fn schedule_gtk_runtime_tick(context: GtkRuntimeTickContext, delay: Duration) {
    gtk::glib::timeout_add_local_once(delay, move || {
        let now = Instant::now();
        let elapsed_ms = now
            .saturating_duration_since(context.last_tick.replace(now))
            .as_millis()
            .clamp(1, u128::from(u32::MAX)) as u32;
        process_gtk_socket_control(&context, true);
        let (redraw, next_delay, mpris_events, mpris_properties) = {
            let mut state = context.main_state.borrow_mut();
            let redraw = state.update_timer_tick_targets(elapsed_ms);
            let next_delay = state.runtime_tick_interval();
            let events = state.take_mpris_events();
            let properties = state.mpris_player_properties();
            (redraw, next_delay, events, properties)
        };
        context
            .mpris_service
            .emit_events(&mpris_events, &mpris_properties);
        if handle_mpris_quit_request(&mpris_events, || context.app.quit()) {
            return;
        }
        queue_gtk_tick_redraw(
            redraw,
            &context.drawing_area,
            &context.panel_windows,
            &context.main_state.borrow(),
        );
        schedule_gtk_runtime_tick(context, next_delay);
    });
}

fn process_gtk_socket_control(context: &GtkRuntimeTickContext, queue_main_redraw: bool) {
    let socket_redraw = poll_socket_control_gtk(
        &context.socket_control,
        &context.app,
        &context.window,
        &context.drawing_area,
        &context.panel_windows,
        &context.menu_popover,
        &context.main_state,
    );
    if socket_redraw {
        if queue_main_redraw {
            context.drawing_area.queue_draw();
        }
        context.panel_windows.playlist_area.queue_draw();
        context.panel_windows.equalizer_area.queue_draw();
        sync_panel_windows(&context.panel_windows, &context.main_state.borrow());
        resize_main_window(
            &context.window,
            &context.drawing_area,
            &context.main_state.borrow(),
        );
    }
}

fn queue_gtk_tick_redraw(
    redraw: GtkTickRedraw,
    drawing_area: &gtk::DrawingArea,
    panel_windows: &PanelWindows,
    state: &MainWindowUiState,
) {
    let playlist_docked = state.panel_state(PanelKind::Playlist).is_docked_visible();
    let equalizer_docked = state.panel_state(PanelKind::Equalizer).is_docked_visible();
    if redraw.main || (redraw.playlist && playlist_docked) || (redraw.equalizer && equalizer_docked)
    {
        drawing_area.queue_draw();
    }
    if redraw.playlist && !playlist_docked {
        panel_windows.playlist_area.queue_draw();
    }
    if redraw.equalizer && !equalizer_docked {
        panel_windows.equalizer_area.queue_draw();
    }
}

fn poll_socket_control_gtk(
    socket_control: &Option<SocketControl>,
    app: &gtk::Application,
    window: &gtk::ApplicationWindow,
    drawing_area: &gtk::DrawingArea,
    panel_windows: &PanelWindows,
    menu_popover: &gtk::Popover,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) -> bool {
    let Some(socket_control) = socket_control else {
        return false;
    };
    let mut redraw = false;
    while let Some(request) = socket_control.try_recv() {
        redraw |= handle_socket_request_gtk(
            app,
            window,
            drawing_area,
            panel_windows,
            menu_popover,
            main_state,
            request,
        );
    }
    redraw
}

fn handle_socket_request_gtk(
    app: &gtk::Application,
    window: &gtk::ApplicationWindow,
    drawing_area: &gtk::DrawingArea,
    panel_windows: &PanelWindows,
    menu_popover: &gtk::Popover,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    request: SocketRequest,
) -> bool {
    let command = request.command.clone();
    let redraw = match command {
        SocketCommand::App(command) => {
            apply_socket_app_command_gtk(command, panel_windows, main_state);
            true
        }
        SocketCommand::Ui(command) => {
            apply_socket_ui_command_gtk(
                command,
                panel_windows,
                menu_popover,
                drawing_area,
                main_state,
            );
            true
        }
        SocketCommand::Quit => {
            app.quit();
            false
        }
    };
    request.accept();
    if redraw {
        sync_panel_windows(panel_windows, &main_state.borrow());
        resize_main_window(window, drawing_area, &main_state.borrow());
    }
    redraw
}

fn apply_socket_app_command_gtk(
    command: AppCommand,
    panel_windows: &PanelWindows,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let result = main_state.borrow_mut().dispatch_store_command(command);
    apply_store_effects_gtk(main_state, panel_windows, result.effects);
    sync_panel_windows(panel_windows, &main_state.borrow());
}

fn apply_store_effects_gtk(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    panel_windows: &PanelWindows,
    effects: impl IntoIterator<Item = AppEffect>,
) {
    for effect in effects {
        let ui_effect = main_state.borrow_mut().apply_store_effect(effect);
        match ui_effect {
            GtkUiEffect::None => {}
            GtkUiEffect::OpenPreferences => panel_windows.preferences.present(),
            GtkUiEffect::OpenSkinBrowser => panel_windows.skin_browser.present(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GtkUiEffect {
    None,
    OpenPreferences,
    OpenSkinBrowser,
}

/// Which top-level GTK window/popover a `SocketUiCommand` addresses.
#[derive(Copy, Clone, Debug)]
enum UiTarget {
    Preferences,
    SkinBrowser,
    MainMenu,
}

fn apply_socket_ui_command_gtk(
    command: SocketUiCommand,
    panel_windows: &PanelWindows,
    menu_popover: &gtk::Popover,
    drawing_area: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    // Resolve the target and desired visibility while briefly holding the state
    // borrow, then RELEASE it before touching any GTK widget. Popovers and
    // dialog windows have `connect_closed` / `connect_hide` handlers that call
    // `main_state.borrow_mut()`; keeping our borrow across `.popdown()` /
    // `.hide()` / `.present()` would reenter and panic with "already borrowed".
    let app_command = match command {
        SocketUiCommand::SetPreferencesVisible(v) => UiCommand::SetPreferencesVisible(v),
        SocketUiCommand::TogglePreferences => UiCommand::TogglePreferences,
        SocketUiCommand::SetSkinBrowserVisible(v) => UiCommand::SetSkinBrowserVisible(v),
        SocketUiCommand::ToggleSkinBrowser => UiCommand::ToggleSkinBrowser,
        SocketUiCommand::SetMainMenuVisible(v) => UiCommand::SetMainMenuVisible(v),
    };
    let (target, visible) = {
        let mut state = main_state.borrow_mut();
        state.dispatch_store_command_and_apply_local_effects(app_command);
        match command {
            SocketUiCommand::SetPreferencesVisible(_) | SocketUiCommand::TogglePreferences => (
                UiTarget::Preferences,
                state.store.state().ui.preferences_visible,
            ),
            SocketUiCommand::SetSkinBrowserVisible(_) | SocketUiCommand::ToggleSkinBrowser => (
                UiTarget::SkinBrowser,
                state.store.state().ui.skin_browser_visible,
            ),
            SocketUiCommand::SetMainMenuVisible(_) => {
                (UiTarget::MainMenu, state.store.state().ui.main_menu_visible)
            }
        }
    };

    match target {
        UiTarget::Preferences => set_window_visible_gtk(&panel_windows.preferences, visible),
        UiTarget::SkinBrowser => set_window_visible_gtk(&panel_windows.skin_browser, visible),
        UiTarget::MainMenu => {
            if visible {
                show_main_menu(menu_popover, drawing_area, &main_state.borrow());
            } else {
                menu_popover.popdown();
            }
        }
    }
}

/// Toggle a `gtk::ApplicationWindow` between presented and hidden.
fn set_window_visible_gtk(window: &gtk::ApplicationWindow, visible: bool) {
    if visible {
        window.present();
    } else {
        window.hide();
    }
}

fn apply_skinned_window_chrome(
    window: &impl IsA<gtk::Window>,
    title: &str,
    extra_css_classes: &[&str],
) {
    window.as_ref().add_css_class("xmms-skinned-window");
    for class in extra_css_classes {
        window.as_ref().add_css_class(class);
    }
    set_skinned_window_titlebar(window, title, extra_css_classes);
}

pub(super) fn set_skinned_window_titlebar(
    window: &impl IsA<gtk::Window>,
    title: &str,
    extra_css_classes: &[&str],
) {
    let titlebar = gtk::HeaderBar::new();
    titlebar.add_css_class("xmms-skinned-window");
    titlebar.add_css_class("xmms-skinned-window-titlebar");
    for class in extra_css_classes {
        titlebar.add_css_class(class);
    }
    titlebar.set_show_title_buttons(true);
    titlebar.set_decoration_layout(Some(":close"));
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("xmms-skinned-window-title");
    titlebar.set_title_widget(Some(&title_label));
    window.as_ref().set_titlebar(Some(&titlebar));
}

fn skinned_application_window(
    app: &gtk::Application,
    title: &str,
    default_width: i32,
    default_height: i32,
    extra_css_classes: &[&str],
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .build();
    apply_skinned_window_chrome(&window, title, extra_css_classes);
    window
}

pub(super) fn skinned_window(
    title: &str,
    default_width: i32,
    default_height: i32,
    extra_css_classes: &[&str],
) -> gtk::Window {
    let window = gtk::Window::builder()
        .title(title)
        .default_width(default_width)
        .default_height(default_height)
        .build();
    apply_skinned_window_chrome(&window, title, extra_css_classes);
    window
}

fn load_skin_from_config(config: &Config) -> io::Result<DefaultSkin> {
    match config.skin.as_deref() {
        Some(path) => DefaultSkin::load_from_path(Path::new(path)),
        None => DefaultSkin::load_bundled(),
    }
}

fn preview_state_from_options(options: PreviewOptions) -> Result<MainWindowUiState, String> {
    preview_state_from_app_state(AppState::default(), options)
}

fn preview_state_from_app_state(
    mut initial_state: AppState,
    options: PreviewOptions,
) -> Result<MainWindowUiState, String> {
    if options.reset {
        initial_state = AppState::default();
    }
    apply_preview_options_to_config(&mut initial_state.config, &options)?;
    apply_preview_playlist(&mut initial_state, &options)?;
    if let Some(scenario) = options.screenshot_scenario {
        scenario.apply_to_app_state(&mut initial_state);
    }

    let mut state = MainWindowUiState::from_state(initial_state);
    if let Some((width, height)) = options.playlist_size {
        state.set_playlist_size(width, height);
    }
    if let Some(skin_path) = options.skin_path.as_ref() {
        state
            .load_configured_skin()
            .map_err(|err| format!("failed to load skin '{}': {err}", skin_path))?;
    }
    Ok(state)
}

fn write_surface_png(surface: &mut ImageSurface, path: &Path) -> io::Result<()> {
    surface.flush();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    surface.save_png(path)
}

struct GtkRenderSurfaceCache<K> {
    key: Option<K>,
    soft_surface: Option<ImageSurface>,
    cairo_surface: Option<gtk::cairo::ImageSurface>,
    #[cfg(test)]
    render_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GtkPlaylistRenderKey {
    skin_generation: u64,
    focused: bool,
    shaded: bool,
    width: i32,
    height: i32,
    scroll_offset: usize,
    revisions: PlaylistRevisions,
    title_preferences_hash: u64,
    font_hash: u64,
    show_numbers: bool,
    playback_time_second: Option<i64>,
    scrollbar_dragging: bool,
    search_hash: u64,
    menu: Option<PlaylistMenuRenderKind>,
    menu_hover: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
struct GtkDockedRenderKey {
    skin_generation: u64,
    main: MainWindowRenderState,
    equalizer: Option<EqualizerRenderState>,
    playlist: Option<GtkPlaylistRenderKey>,
}

impl<K> Default for GtkRenderSurfaceCache<K> {
    fn default() -> Self {
        Self {
            key: None,
            soft_surface: None,
            cairo_surface: None,
            #[cfg(test)]
            render_count: 0,
        }
    }
}

fn render_scaled_to_gtk_cached<K, F>(
    cache: &mut GtkRenderSurfaceCache<K>,
    key: K,
    cr: &gtk::cairo::Context,
    device_width: i32,
    device_height: i32,
    base_width: i32,
    base_height: i32,
    draw: F,
) -> Result<(), crate::render::RenderError>
where
    K: PartialEq,
    F: Fn(&Context, RenderPass) -> Result<(), crate::render::RenderError>,
{
    let dimensions_changed = cache
        .soft_surface
        .as_ref()
        .is_none_or(|surface| surface.width() != device_width || surface.height() != device_height);
    if dimensions_changed {
        cache.soft_surface = Some(ImageSurface::create(
            Format::ARgb32,
            device_width,
            device_height,
        )?);
        cache.cairo_surface = Some(
            gtk::cairo::ImageSurface::create(
                gtk::cairo::Format::ARgb32,
                device_width,
                device_height,
            )
            .map_err(|err| {
                crate::render::RenderError::Surface(crate::render::Error::new(err.to_string()))
            })?,
        );
    }

    if dimensions_changed || cache.key.as_ref() != Some(&key) {
        #[cfg(test)]
        {
            cache.render_count += 1;
        }
        let surface = cache
            .soft_surface
            .as_ref()
            .expect("GTK soft render surface initialized");
        surface.clear();
        let soft_cr = Context::new(surface)?;
        render_scaled(
            &soft_cr,
            device_width,
            device_height,
            base_width,
            base_height,
            draw,
        )?;
        drop(soft_cr);
        surface.flush();

        let cairo_surface = cache
            .cairo_surface
            .as_mut()
            .expect("GTK Cairo render surface initialized");
        cr.set_source_rgb(0.0, 0.0, 0.0);
        {
            let source = surface.data()?;
            let mut dest = cairo_surface.data().map_err(|err| {
                crate::render::RenderError::SurfaceData(crate::render::BorrowError::new(
                    err.to_string(),
                ))
            })?;
            dest.copy_from_slice(&source);
        }
        cairo_surface.mark_dirty();
        cache.key = Some(key);
    }

    let cairo_surface = cache
        .cairo_surface
        .as_ref()
        .expect("GTK Cairo render surface initialized");
    cr.set_source_surface(cairo_surface, 0.0, 0.0)
        .map_err(|err| {
            crate::render::RenderError::Surface(crate::render::Error::new(err.to_string()))
        })?;
    cr.paint().map_err(|err| {
        crate::render::RenderError::Surface(crate::render::Error::new(err.to_string()))
    })
}

fn style_xmms_popover(popover: &gtk::Popover) {
    popover.add_css_class("xmms-menu-popover");
}

fn xmms_menu_box(spacing: i32) -> gtk::Box {
    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, spacing);
    menu_box.add_css_class("xmms-menu-box");
    menu_box
}

fn xmms_menu_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_halign(gtk::Align::Fill);
    button.add_css_class("xmms-menu-button");
    button
}

macro_rules! bind_visibility_window {
    ($window:expr, $main_state:expr, $setter:ident) => {{
        {
            let main_state = Rc::clone($main_state);
            $window.connect_close_request(move |window| {
                main_state.borrow_mut().$setter(false);
                window.hide();
                gtk::glib::Propagation::Stop
            });
        }
        {
            let main_state = Rc::clone($main_state);
            $window.connect_hide(move |_| {
                main_state.borrow_mut().$setter(false);
            });
        }
    }};
}

macro_rules! connect_clicked_cloned {
    ($button:expr, clone [$($clone:ident),* $(,)?], rc [$($rc:ident),* $(,)?], move |_| $body:block) => {{
        $(let $clone = $clone.clone();)*
        $(let $rc = Rc::clone($rc);)*
        $button.connect_clicked(move |_| $body);
    }};
}

type MainKeyboardShortcut = AppShortcut;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
}

impl ArrowKey {
    fn from_gdk(key: gtk::gdk::Key) -> Option<Self> {
        match key {
            gtk::gdk::Key::Up | gtk::gdk::Key::KP_Up => Some(Self::Up),
            gtk::gdk::Key::Down | gtk::gdk::Key::KP_Down => Some(Self::Down),
            gtk::gdk::Key::Left | gtk::gdk::Key::KP_Left => Some(Self::Left),
            gtk::gdk::Key::Right | gtk::gdk::Key::KP_Right => Some(Self::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyCommand {
    Volume(i32),
    Balance(i32),
    Seek(i32),
    PreviousTrack,
    NextTrack,
    PlaylistMove(isize),
    EqualizerAdjust(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferencesPage {
    Audio,
    Visualization,
    Options,
    Fonts,
    Title,
}

pub fn preferences_window_default_size() -> (i32, i32) {
    (560, 680)
}

pub fn preferences_zoom_spans_full_width() -> bool {
    true
}

pub fn preferences_page_parity_controls(page: PreferencesPage) -> &'static [&'static str] {
    match page {
        PreferencesPage::Audio => &[
            "Input Plugins",
            "Output Plugin",
            "Output device:",
            "Configure",
        ],
        PreferencesPage::Visualization => &[
            "Visualization mode:",
            "Analyzer mode:",
            "Analyzer style:",
            "Scope mode:",
            "Show analyzer peaks",
            "Analyzer falloff:",
            "Peaks falloff:",
            "Window Shade Level Meter Style:",
            "Refresh rate:",
        ],
        PreferencesPage::Options => &[
            "Volume:",
            "Balance:",
            "Zoom level:",
            "Repeat",
            "Shuffle",
            "No playlist advance",
            "Pause between songs",
            "Pause between songs time (seconds):",
            "Mouse Wheel adjusts Volume by (%):",
            "Stop with fadeout",
            "Time remaining",
            "Dock playlist",
            "Dock equalizer",
            "Convert %20 to space",
            "Convert underscore to space",
            "Show numbers in playlist",
            "Vim-style playlist navigation",
        ],
        PreferencesPage::Fonts => &[
            "Playlist font size:",
            "Open Skin Browser",
            "Skin bitmap font",
        ],
        PreferencesPage::Title => &["Title format:"],
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualizationPreferenceSensitivity {
    pub analyzer_mode: bool,
    pub analyzer_style: bool,
    pub analyzer_peaks: bool,
    pub analyzer_falloff: bool,
    pub peaks_falloff: bool,
    pub scope_mode: bool,
    pub windowshade_vu: bool,
    pub refresh_rate: bool,
}

pub fn visualization_preference_sensitivity(
    mode: VisMode,
    peaks_enabled: bool,
) -> VisualizationPreferenceSensitivity {
    let analyzer = mode == VisMode::Analyzer;
    let scope = mode == VisMode::Scope;
    VisualizationPreferenceSensitivity {
        analyzer_mode: analyzer,
        analyzer_style: analyzer,
        analyzer_peaks: analyzer,
        analyzer_falloff: analyzer,
        peaks_falloff: analyzer && peaks_enabled,
        scope_mode: scope,
        windowshade_vu: analyzer,
        refresh_rate: analyzer || scope,
    }
}

fn keyboard_shortcut_from_event(
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> Option<MainKeyboardShortcut> {
    APP_SHORTCUTS
        .iter()
        .find_map(|spec| shortcut_matches(key, state, spec.accelerator).then_some(spec.shortcut))
}

fn handle_main_playlist_key_pressed(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    {
        let mut ui_state = main_state.borrow_mut();
        if let Some(handled) = handle_active_playlist_search_key_pressed(&mut ui_state, key, state)
        {
            return handled;
        }
    }
    let mut ui_state = main_state.borrow_mut();
    match ui_state.selected_docked_panel() {
        Some(PanelKind::Playlist) => {}
        Some(PanelKind::Equalizer) => {
            return handle_equalizer_key_pressed(&mut ui_state, key, state)
        }
        None => return handle_main_player_key_pressed(&mut ui_state, key, state),
    }
    if handle_playlist_parity_key_pressed(&mut ui_state, key, state) {
        return true;
    }
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return false;
    }
    if handle_playlist_navigation_key_pressed(&mut ui_state, key) {
        return true;
    }
    if key == gtk::gdk::Key::slash {
        return ui_state.start_playlist_search();
    }
    if key != gtk::gdk::Key::Delete && key != gtk::gdk::Key::KP_Delete {
        return false;
    }
    ui_state.remove_selected_playlist_entries()
}

fn handle_keyboard_shortcut(
    shortcut: MainKeyboardShortcut,
    window: &gtk::ApplicationWindow,
    drawing_area: &gtk::DrawingArea,
    panel_windows: &PanelWindows,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    match shortcut {
        MainKeyboardShortcut::Previous => {
            main_state
                .borrow_mut()
                .activate_push(MainPushButton::Previous);
        }
        MainKeyboardShortcut::Play => {
            main_state.borrow_mut().activate_push(MainPushButton::Play);
        }
        MainKeyboardShortcut::Pause => {
            main_state.borrow_mut().activate_push(MainPushButton::Pause);
        }
        MainKeyboardShortcut::Stop => {
            main_state.borrow_mut().activate_push(MainPushButton::Stop);
        }
        MainKeyboardShortcut::Next => {
            main_state.borrow_mut().activate_push(MainPushButton::Next);
        }
        MainKeyboardShortcut::OpenFiles => {
            main_state.borrow_mut().set_file_dialog_visible(true);
            show_open_file_dialog(window, Rc::clone(main_state));
        }
        MainKeyboardShortcut::ToggleRepeat => {
            main_state
                .borrow_mut()
                .activate_toggle(MainToggleButton::Repeat);
        }
        MainKeyboardShortcut::ToggleShuffle => {
            main_state
                .borrow_mut()
                .activate_toggle(MainToggleButton::Shuffle);
        }
        MainKeyboardShortcut::Preferences => {
            main_state.borrow_mut().set_preferences_visible(true);
            panel_windows.preferences.present();
        }
        MainKeyboardShortcut::OpenLocation => {
            main_state.borrow_mut().set_open_location_visible(true);
            panel_windows.open_location.present();
        }
        MainKeyboardShortcut::ToggleNoAdvance => {
            main_state
                .borrow_mut()
                .dispatch_store_command_and_apply_local_effects(PlaylistCommand::ToggleNoAdvance);
        }
        MainKeyboardShortcut::ShadeMain => {
            let toggled_panel = main_state.borrow_mut().toggle_selected_window_shade();
            match toggled_panel {
                Some(PanelKind::Playlist) => sync_single_panel_window_from_state(
                    PanelKind::Playlist,
                    &panel_windows.playlist,
                    &panel_windows.playlist_area,
                    main_state,
                ),
                Some(PanelKind::Equalizer) => sync_single_panel_window_from_state(
                    PanelKind::Equalizer,
                    &panel_windows.equalizer,
                    &panel_windows.equalizer_area,
                    main_state,
                ),
                None => resize_main_window(window, drawing_area, &main_state.borrow()),
            }
        }
        MainKeyboardShortcut::JumpTime => {
            main_state.borrow_mut().set_jump_time_visible(true);
            panel_windows.jump_time.present();
        }
        MainKeyboardShortcut::SkinBrowser => {
            main_state.borrow_mut().set_skin_browser_visible(true);
            panel_windows.skin_browser.present();
        }
        MainKeyboardShortcut::OpenDirectory => {
            main_state.borrow_mut().set_directory_dialog_visible(true);
            show_open_directory_dialog(window, Rc::clone(main_state));
        }
        MainKeyboardShortcut::PresentMain => {
            window.present();
        }
        MainKeyboardShortcut::TogglePlaylist => {
            main_state
                .borrow_mut()
                .activate_toggle(MainToggleButton::Playlist);
            sync_panel_windows(panel_windows, &main_state.borrow());
        }
        MainKeyboardShortcut::ToggleEqualizer => {
            main_state
                .borrow_mut()
                .activate_toggle(MainToggleButton::Equalizer);
            sync_panel_windows(panel_windows, &main_state.borrow());
        }
        MainKeyboardShortcut::ShadePlaylist => {
            {
                let mut state = main_state.borrow_mut();
                state.toggle_playlist_shaded();
            }
            sync_single_panel_window_from_state(
                PanelKind::Playlist,
                &panel_windows.playlist,
                &panel_windows.playlist_area,
                main_state,
            );
        }
        MainKeyboardShortcut::ShadeEqualizer => {
            {
                let mut state = main_state.borrow_mut();
                state.toggle_equalizer_shaded();
            }
            sync_single_panel_window_from_state(
                PanelKind::Equalizer,
                &panel_windows.equalizer,
                &panel_windows.equalizer_area,
                main_state,
            );
        }
        MainKeyboardShortcut::ToggleTimerRemaining => {
            let enabled = !main_state.borrow().preference_timer_remaining();
            main_state
                .borrow_mut()
                .set_preference_timer_remaining(enabled);
        }
        MainKeyboardShortcut::ToggleSticky => {
            main_state.borrow_mut().toggle_sticky();
        }
        MainKeyboardShortcut::DoubleScale => {
            main_state.borrow_mut().double_fractional_scale();
        }
        MainKeyboardShortcut::HalfScale => {
            main_state.borrow_mut().halve_fractional_scale();
        }
        MainKeyboardShortcut::ToggleEasyMove => {
            main_state.borrow_mut().toggle_easy_move();
        }
        MainKeyboardShortcut::StartOfList => {
            main_state.borrow_mut().select_first_playlist_entry();
        }
        MainKeyboardShortcut::FileInfo => {
            show_file_info_dialog(window, Rc::clone(main_state));
        }
    }
    resize_main_window(window, drawing_area, &main_state.borrow());
    drawing_area.queue_draw();
}

fn add_file_drop_controller(
    widget: &impl IsA<gtk::Widget>,
    main_state: Rc<RefCell<MainWindowUiState>>,
    clear_first: bool,
    start_playback: bool,
) {
    let drop = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    {
        let widget = widget.clone();
        drop.connect_drop(move |_target, value, _x, _y| {
            let Ok(files) = value.get::<gtk::gdk::FileList>() else {
                return false;
            };
            let uris = files
                .files()
                .into_iter()
                .map(|file| file.uri().to_string())
                .collect::<Vec<_>>();
            if !main_state
                .borrow_mut()
                .accept_dropped_uris(uris, clear_first, start_playback)
            {
                return false;
            }
            widget.queue_draw();
            true
        });
    }
    widget.add_controller(drop);
}

fn resize_main_window(
    window: &gtk::ApplicationWindow,
    drawing_area: &gtk::DrawingArea,
    state: &MainWindowUiState,
) {
    let (width, height) = state.docked_panel_size();
    let scale = state.scale_factor();
    drawing_area.set_content_width(scale_dim(width, scale));
    drawing_area.set_content_height(scale_dim(height, scale));
    window.set_default_size(scale_dim(width, scale), scale_dim(height, scale));
}

fn unscale_dim(value: i32, scale: f64) -> i32 {
    ((f64::from(value) / scale.clamp(1.0, 5.0)) + 0.5).max(1.0) as i32
}

fn render_docked_ui_state(
    cr: &Context,
    skin: &DefaultSkin,
    state: &MainWindowUiState,
    pass: RenderPass,
) -> Result<bool, crate::render::RenderError> {
    let mut y = 0;
    let mut rendered = false;
    if pass.is_bitmap() {
        rendered |= render_main_player_state(cr, skin, &state.render_state())?;
    }
    y += main_window_height(state.is_shaded());

    if state.panel_state(PanelKind::Equalizer).is_docked_visible() {
        if pass.is_bitmap() {
            cr.save()?;
            cr.translate(0.0, f64::from(y));
            rendered |= render_equalizer_state(cr, skin, &state.equalizer_render_state())?;
            cr.restore()?;
        }
        y += equalizer_window_height(state.is_equalizer_shaded());
    }

    if state.panel_state(PanelKind::Playlist).is_docked_visible() {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        if pass.is_bitmap() {
            rendered |= render_playlist_frame(
                cr,
                skin,
                state.playlist_focused(),
                state.is_playlist_shaded(),
                state.playlist_ui.width,
                state.playlist_ui.height,
                Some(&state.shaded_playlist_info()),
                Some(&state.playlist_footer_info()),
                Some(&state.playlist_footer_time_min_text()),
                Some(&state.playlist_footer_time_sec_text()),
            )?;
        }
        if !state.is_playlist_shaded() {
            let row_state = state.playlist_rows_render_state();
            rendered |= render_playlist_rows(cr, skin, &row_state, pass)?;
        }
        if pass.is_text() {
            if let Some(menu) = state.playlist_menu() {
                let (x, y, w, h) =
                    playlist_menu_rect(menu, state.playlist_ui.width, state.playlist_ui.height);
                paint_scaled(cr, x, y, w, h, |menu_cr| {
                    render_playlist_menu(
                        menu_cr,
                        skin,
                        PlaylistMenuRenderState {
                            kind: menu.render_kind(),
                            hover: state.playlist_menu_hover(),
                        },
                    )
                    .map(|_| ())
                })?;
                rendered = true;
            }
        }
        cr.restore()?;
    }

    Ok(rendered)
}

fn build_main_menu_popover(
    app: &gtk::Application,
    parent_window: &gtk::ApplicationWindow,
    parent: &gtk::DrawingArea,
    preferences_window: &gtk::ApplicationWindow,
    open_location_window: &gtk::ApplicationWindow,
    skin_browser_window: &gtk::ApplicationWindow,
    skin_editor_window: &gtk::ApplicationWindow,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(false)
        .build();
    style_xmms_popover(&popover);
    popover.set_parent(parent);

    let menu_box = xmms_menu_box(0);
    let open_files = xmms_menu_button("Open Files...");
    connect_clicked_cloned!(open_files, clone [parent_window, popover], rc [main_state], move |_| {
        main_state.borrow_mut().set_menu_visible(false);
        popover.popdown();
        show_open_file_dialog(&parent_window, Rc::clone(&main_state));
    });
    menu_box.append(&open_files);

    let open_location = xmms_menu_button("Open Location...");
    connect_clicked_cloned!(open_location, clone [open_location_window, popover], rc [main_state], move |_| {
        {
            let mut state = main_state.borrow_mut();
            state.set_menu_visible(false);
            state.set_open_location_visible(true);
        }
        popover.popdown();
        open_location_window.present();
    });
    menu_box.append(&open_location);

    let preferences = xmms_menu_button("Preferences");
    connect_clicked_cloned!(preferences, clone [preferences_window, popover], rc [main_state], move |_| {
        {
            let mut state = main_state.borrow_mut();
            state.set_menu_visible(false);
            state.set_preferences_visible(true);
        }
        popover.popdown();
        preferences_window.present();
    });
    menu_box.append(&preferences);

    let skin_browser = xmms_menu_button("Skin Browser");
    connect_clicked_cloned!(skin_browser, clone [skin_browser_window, popover], rc [main_state], move |_| {
        {
            let mut state = main_state.borrow_mut();
            state.set_menu_visible(false);
            state.set_skin_browser_visible(true);
        }
        popover.popdown();
        skin_browser_window.present();
    });
    menu_box.append(&skin_browser);

    let skin_editor = xmms_menu_button("Skin Editor");
    connect_clicked_cloned!(skin_editor, clone [skin_editor_window, popover], rc [main_state], move |_| {
        {
            let mut state = main_state.borrow_mut();
            state.set_menu_visible(false);
            state.set_skin_editor_visible(true);
        }
        popover.popdown();
        skin_editor_window.present();
    });
    menu_box.append(&skin_editor);

    let quit = xmms_menu_button("Quit");
    connect_clicked_cloned!(quit, clone [app, popover], rc [main_state], move |_| {
        main_state.borrow_mut().set_menu_visible(false);
        popover.popdown();
        app.quit();
    });
    menu_box.append(&quit);

    popover.set_child(Some(&menu_box));
    {
        let main_state = Rc::clone(main_state);
        popover.connect_closed(move |_| main_state.borrow_mut().set_menu_visible(false));
    }
    popover
}

struct PanelWindows {
    equalizer: gtk::ApplicationWindow,
    equalizer_area: gtk::DrawingArea,
    playlist: gtk::ApplicationWindow,
    playlist_area: gtk::DrawingArea,
    preferences: gtk::ApplicationWindow,
    open_location: gtk::ApplicationWindow,
    jump_time: gtk::ApplicationWindow,
    skin_browser: gtk::ApplicationWindow,
    skin_editor: gtk::ApplicationWindow,
}

impl PanelWindows {
    fn new(
        app: &gtk::Application,
        main_state: &Rc<RefCell<MainWindowUiState>>,
        main_area: &gtk::DrawingArea,
        parent_window: &gtk::ApplicationWindow,
    ) -> Self {
        let (equalizer, equalizer_area) = build_equalizer_window(app, main_state, main_area);
        let open_location =
            build_prompt_window(app, parent_window, main_state, PromptKind::OpenLocation);
        let jump_time = build_prompt_window(app, parent_window, main_state, PromptKind::JumpTime);
        let (playlist, playlist_area) =
            build_playlist_window(app, main_state, main_area, &open_location);
        let skin_browser =
            build_skin_browser_window(app, main_state, main_area, &equalizer_area, &playlist_area);
        let skin_editor =
            build_skin_editor_window(app, main_state, main_area, &equalizer_area, &playlist_area);
        let preferences = build_preferences_window(
            app,
            main_state,
            parent_window,
            main_area,
            &equalizer,
            &equalizer_area,
            &playlist,
            &playlist_area,
        );
        Self {
            equalizer,
            equalizer_area,
            playlist,
            playlist_area,
            preferences,
            open_location,
            jump_time,
            skin_browser,
            skin_editor,
        }
    }
}

fn build_equalizer_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
) -> (gtk::ApplicationWindow, gtk::DrawingArea) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("XMMS Renascene Rust Equalizer")
        .resizable(false)
        .decorated(false)
        .default_width(EQUALIZER_WINDOW_WIDTH * DEFAULT_SCALE)
        .default_height(EQUALIZER_WINDOW_HEIGHT * DEFAULT_SCALE)
        .build();
    let drawing_area = gtk::DrawingArea::builder()
        .content_width(EQUALIZER_WINDOW_WIDTH * DEFAULT_SCALE)
        .content_height(EQUALIZER_WINDOW_HEIGHT * DEFAULT_SCALE)
        .focusable(true)
        .build();
    let render_cache = Rc::new(RefCell::new(GtkRenderSurfaceCache::default()));
    let state = Rc::clone(main_state);
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let state = state.borrow();
        let render_state = state.equalizer_render_state();
        let base_height = if render_state.shaded {
            MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        };
        let base_width = EQUALIZER_WINDOW_WIDTH;
        let render_key = (state.skin_generation, render_state);
        match render_scaled_to_gtk_cached(
            &mut render_cache.borrow_mut(),
            render_key,
            cr,
            width,
            height,
            base_width,
            base_height,
            |cr, pass| {
                if pass.is_bitmap() {
                    render_equalizer_state(cr, state.active_skin(), &render_state).map(|_| ())
                } else {
                    Ok(())
                }
            },
        ) {
            Ok(()) => app_log_trace!(
                render,
                "gtk equalizer",
                width,
                height,
                base_width,
                base_height
            ),
            Err(err) => eprintln!("xmms-rs: failed to render equalizer preview: {err}"),
        }
    });
    let presets_menu = build_equalizer_presets_popover(&drawing_area, main_state, main_area);
    add_panel_click_controller(
        &window,
        &drawing_area,
        Rc::clone(main_state),
        main_area.clone(),
        PanelKind::Equalizer,
        Some(presets_menu),
        None,
        None,
    );
    add_equalizer_key_controller(&drawing_area, Rc::clone(main_state));
    window.set_child(Some(&drawing_area));
    (window, drawing_area)
}

fn add_playlist_key_controller(
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let key_controller = gtk::EventControllerKey::new();
    {
        let area = area.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
            if handle_playlist_key_pressed(&main_state, key, state) {
                area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    area.add_controller(key_controller);
}

fn add_equalizer_key_controller(
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
            if handle_equalizer_key_pressed(&mut main_state.borrow_mut(), key, state) {
                area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    area.add_controller(key_controller);
}

fn focus_cycle_shortcut(key: gtk::gdk::Key, state: gtk::gdk::ModifierType) -> bool {
    key == gtk::gdk::Key::Tab
        && state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
        && !state.intersects(gtk::gdk::ModifierType::ALT_MASK | gtk::gdk::ModifierType::META_MASK)
}

fn handle_arrow_key_pressed(
    ui_state: &mut MainWindowUiState,
    focus: KeyboardFocus,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return false;
    }
    let Some(arrow) = ArrowKey::from_gdk(key) else {
        return false;
    };
    ui_state.apply_key_command(ui_state.arrow_key_command(focus, arrow))
}

fn handle_main_player_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    handle_arrow_key_pressed(ui_state, KeyboardFocus::Main, key, state)
}

fn handle_equalizer_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    handle_arrow_key_pressed(ui_state, KeyboardFocus::Equalizer, key, state)
}

fn handle_playlist_key_pressed(
    main_state: &Rc<RefCell<MainWindowUiState>>,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    {
        let mut ui_state = main_state.borrow_mut();
        if let Some(handled) = handle_active_playlist_search_key_pressed(&mut ui_state, key, state)
        {
            return handled;
        }
    }

    {
        let mut ui_state = main_state.borrow_mut();
        if handle_playlist_parity_key_pressed(&mut ui_state, key, state) {
            return true;
        }
    }
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return false;
    }
    if key == gtk::gdk::Key::Delete || key == gtk::gdk::Key::KP_Delete {
        return main_state.borrow_mut().remove_selected_playlist_entries();
    }
    {
        let mut ui_state = main_state.borrow_mut();
        if handle_playlist_navigation_key_pressed(&mut ui_state, key) {
            return true;
        }
    }
    if key == gtk::gdk::Key::slash {
        return main_state.borrow_mut().start_playlist_search();
    }
    false
}

fn handle_active_playlist_search_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> Option<bool> {
    if !ui_state.playlist_search_active() {
        return None;
    }
    if key == gtk::gdk::Key::Escape {
        ui_state.stop_playlist_search();
        return Some(true);
    }
    if key == gtk::gdk::Key::Return || key == gtk::gdk::Key::KP_Enter {
        ui_state.stop_playlist_search();
        ui_state.play_selected_playlist_entry();
        return Some(true);
    }
    if key == gtk::gdk::Key::BackSpace {
        ui_state.pop_playlist_search_char();
        return Some(true);
    }
    if state.intersects(
        gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::ALT_MASK
            | gtk::gdk::ModifierType::META_MASK,
    ) {
        return Some(true);
    }
    if let Some(ch) = key.to_unicode().filter(|ch| !ch.is_control()) {
        ui_state.push_playlist_search_char(ch);
        return Some(true);
    }
    Some(true)
}

fn handle_playlist_parity_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> bool {
    let control = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
    let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
    let alt = state.contains(gtk::gdk::ModifierType::ALT_MASK);
    let is_delete = key == gtk::gdk::Key::Delete || key == gtk::gdk::Key::KP_Delete;
    let is_q = key == gtk::gdk::Key::q || key == gtk::gdk::Key::Q;
    if control && is_delete {
        return ui_state.crop_playlist_to_selected_or_current();
    }
    if alt && is_q {
        return ui_state.open_queue_manager();
    }
    if shift && is_q {
        return ui_state.clear_playlist_queue();
    }
    if !control && !shift && !alt && is_q {
        return ui_state.toggle_queue_selected_playlist_entries();
    }
    if handle_arrow_key_pressed(ui_state, KeyboardFocus::Playlist, key, state) {
        return true;
    }
    match key {
        gtk::gdk::Key::Page_Up | gtk::gdk::Key::KP_Page_Up => ui_state.move_playlist_page(-1),
        gtk::gdk::Key::Page_Down | gtk::gdk::Key::KP_Page_Down => ui_state.move_playlist_page(1),
        gtk::gdk::Key::Home | gtk::gdk::Key::KP_Home => ui_state.move_playlist_to_start(),
        gtk::gdk::Key::End | gtk::gdk::Key::KP_End => ui_state.move_playlist_to_end(),
        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
            ui_state.activate_selected_or_current_playlist_entry()
        }
        _ => false,
    }
}

fn handle_playlist_navigation_key_pressed(
    ui_state: &mut MainWindowUiState,
    key: gtk::gdk::Key,
) -> bool {
    match key {
        gtk::gdk::Key::j => ui_state.move_playlist_selection(1),
        gtk::gdk::Key::k => ui_state.move_playlist_selection(-1),
        gtk::gdk::Key::p => ui_state.play_selected_playlist_entry(),
        _ => false,
    }
}

fn add_playlist_context_menu(
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    main_area: gtk::DrawingArea,
) {
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    style_xmms_popover(&popover);
    popover.set_parent(area);

    let menu_box = xmms_menu_box(4);
    for (label, action) in [
        ("Remove Selected", PlaylistContextAction::RemoveSelected),
        ("Remove Dead Files", PlaylistContextAction::RemoveDead),
        ("Physically Delete", PlaylistContextAction::PhysicallyDelete),
        ("Select All", PlaylistContextAction::SelectAll),
        ("Select None", PlaylistContextAction::SelectNone),
        ("Invert Selection", PlaylistContextAction::InvertSelection),
    ] {
        let button = xmms_menu_button(label);
        let state = Rc::clone(&main_state);
        let area = area.clone();
        let main_area = main_area.clone();
        let popover = popover.clone();
        button.connect_clicked(move |_| {
            popover.popdown();
            if action == PlaylistContextAction::PhysicallyDelete {
                show_playlist_delete_confirmation(
                    &area,
                    Rc::clone(&state),
                    area.clone(),
                    main_area.clone(),
                );
            } else {
                state.borrow_mut().activate_playlist_context_action(action);
                area.queue_draw();
                main_area.queue_draw();
            }
        });
        menu_box.append(&button);
    }
    popover.set_child(Some(&menu_box));

    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    right_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        let popover = popover.clone();
        right_click.connect_pressed(move |_gesture, _n_press, x, y| {
            area.grab_focus();
            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        });
    }
    area.add_controller(right_click);
}

fn show_playlist_delete_confirmation(
    parent: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    playlist_area: gtk::DrawingArea,
    main_area: gtk::DrawingArea,
) {
    let window = skinned_window("Delete selected files?", 280, 100, &[]);
    window.set_modal(true);
    if let Some(root) = parent
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
    {
        window.set_transient_for(Some(&root));
    }

    let layout = gtk::Box::new(gtk::Orientation::Vertical, 8);
    layout.add_css_class("xmms-skinned-window");
    layout.set_margin_top(8);
    layout.set_margin_bottom(8);
    layout.set_margin_start(8);
    layout.set_margin_end(8);
    layout.append(&gtk::Label::new(Some(
        "Delete selected local files from disk?",
    )));

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let cancel = gtk::Button::with_label("Cancel");
    let delete = gtk::Button::with_label("Delete");
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| {
            window.close();
        });
    }
    {
        let window = window.clone();
        delete.connect_clicked(move |_| {
            main_state
                .borrow_mut()
                .activate_playlist_context_action(PlaylistContextAction::PhysicallyDelete);
            window.close();
            playlist_area.queue_draw();
            main_area.queue_draw();
        });
    }
    buttons.append(&cancel);
    buttons.append(&delete);
    layout.append(&buttons);
    window.set_child(Some(&layout));
    window.present();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptKind {
    OpenLocation,
    JumpTime,
}

impl PromptKind {
    fn title(self) -> &'static str {
        match self {
            Self::OpenLocation => "Play Location",
            Self::JumpTime => "Jump to Time",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Self::OpenLocation => "https://...",
            Self::JumpTime => "seconds or mm:ss",
        }
    }

    fn set_visible(self, state: &mut MainWindowUiState, visible: bool) {
        match self {
            Self::OpenLocation => state.set_open_location_visible(visible),
            Self::JumpTime => state.set_jump_time_visible(visible),
        }
    }

    fn accept(self, state: &mut MainWindowUiState, text: &str) {
        match self {
            Self::OpenLocation => state.accept_open_location(text),
            Self::JumpTime => state.accept_jump_time(text),
        }
    }
}

impl PlaylistMenuKind {
    fn render_kind(self) -> PlaylistMenuRenderKind {
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelAction {
    None,
    Changed,
    OpenDirectoryDialog,
    OpenFileDialog,
    OpenLocationWindow,
    OpenPlaylistLoadDialog,
    OpenPlaylistSaveDialog,
    ShowPlaylistSortMenu,
    ShowFileInfo,
    ShowPlaylistMenu(PlaylistMenuKind),
    ShowEqualizerPresets,
}

struct GtkPanelActionContext<'a> {
    window: &'a gtk::ApplicationWindow,
    area: &'a gtk::DrawingArea,
    main_state: &'a Rc<RefCell<MainWindowUiState>>,
    open_location_window: Option<&'a gtk::ApplicationWindow>,
    playlist_sort_menu: Option<&'a gtk::Popover>,
    equalizer_presets_menu: Option<&'a gtk::Popover>,
    equalizer_presets_docked: bool,
}

fn apply_panel_action(
    action: PanelAction,
    context: GtkPanelActionContext<'_>,
    on_changed: impl FnOnce(),
) {
    match action {
        PanelAction::None => {}
        PanelAction::Changed => on_changed(),
        PanelAction::OpenDirectoryDialog => {
            context
                .main_state
                .borrow_mut()
                .set_directory_dialog_visible(true);
            show_playlist_add_directory_dialog(
                context.window,
                Rc::clone(context.main_state),
                context.area.clone(),
            );
        }
        PanelAction::OpenFileDialog => {
            context
                .main_state
                .borrow_mut()
                .set_file_dialog_visible(true);
            show_playlist_add_file_dialog(
                context.window,
                Rc::clone(context.main_state),
                context.area.clone(),
            );
        }
        PanelAction::OpenLocationWindow => {
            context
                .main_state
                .borrow_mut()
                .set_open_location_visible(true);
            if let Some(open_location_window) = context.open_location_window {
                open_location_window.present();
            }
        }
        PanelAction::OpenPlaylistLoadDialog => {
            context
                .main_state
                .borrow_mut()
                .set_playlist_load_dialog_visible(true);
            show_playlist_load_dialog(
                context.window,
                Rc::clone(context.main_state),
                context.area.clone(),
            );
        }
        PanelAction::OpenPlaylistSaveDialog => {
            context
                .main_state
                .borrow_mut()
                .set_playlist_save_dialog_visible(true);
            show_playlist_save_dialog(context.window, Rc::clone(context.main_state));
        }
        PanelAction::ShowFileInfo => {
            show_file_info_dialog(context.window, Rc::clone(context.main_state));
        }
        PanelAction::ShowPlaylistSortMenu => {
            if let Some(popover) = context.playlist_sort_menu {
                show_playlist_sort_menu(popover, context.area);
            }
            context.area.queue_draw();
        }
        PanelAction::ShowPlaylistMenu(_) => {
            context.area.queue_draw();
        }
        PanelAction::ShowEqualizerPresets => {
            if let Some(popover) = context.equalizer_presets_menu {
                if context.equalizer_presets_docked {
                    show_docked_equalizer_presets_menu(
                        popover,
                        context.area,
                        &context.main_state.borrow(),
                    );
                } else {
                    show_equalizer_presets_menu(popover, context.area);
                }
            }
            context.area.queue_draw();
        }
    }
}

fn add_panel_click_controller(
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    main_state: Rc<RefCell<MainWindowUiState>>,
    main_area: gtk::DrawingArea,
    kind: PanelKind,
    equalizer_presets_menu: Option<gtk::Popover>,
    open_location_window: Option<gtk::ApplicationWindow>,
    playlist_sort_menu: Option<gtk::Popover>,
) {
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        let window = window.clone();
        let main_state = Rc::clone(&main_state);
        click.connect_pressed(move |gesture, n_press, x, y| {
            area.grab_focus();
            let (base_x, base_y) =
                panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            if n_press >= 2
                && kind == PanelKind::Equalizer
                && main_state
                    .borrow()
                    .panel_title_drag_region(kind, base_x, base_y)
            {
                main_state.borrow_mut().toggle_equalizer_shaded();
                sync_single_panel_window_from_state(kind, &window, &area, &main_state);
                area.queue_draw();
                return;
            }
            if !main_state
                .borrow()
                .panel_title_drag_region(kind, base_x, base_y)
            {
                if kind == PanelKind::Equalizer
                    && main_state.borrow_mut().equalizer_press(base_x, base_y)
                {
                    area.queue_draw();
                } else if kind == PanelKind::Playlist {
                    if n_press >= 2
                        && main_state
                            .borrow_mut()
                            .activate_playlist_entry_at(base_x, base_y)
                    {
                        area.queue_draw();
                        return;
                    }
                    let ctrl_pressed = gesture
                        .current_event_state()
                        .contains(gtk::gdk::ModifierType::CONTROL_MASK);
                    if main_state.borrow_mut().playlist_press_with_ctrl(
                        base_x,
                        base_y,
                        ctrl_pressed,
                    ) {
                        area.queue_draw();
                        return;
                    }
                    if main_state
                        .borrow_mut()
                        .playlist_scrollbar_press(base_x, base_y)
                    {
                        area.queue_draw();
                        return;
                    }
                    if main_state.borrow().playlist_resize_region(base_x, base_y) {
                        return;
                    }
                }
                return;
            }

            main_state.borrow_mut().set_panel_dragging(kind, true);
            area.queue_draw();
            let Some(device) = gesture.current_event_device() else {
                return;
            };
            let Some(surface) = window.surface() else {
                return;
            };
            let Ok(toplevel) = surface.downcast::<gtk::gdk::Toplevel>() else {
                return;
            };
            toplevel.begin_move(
                &device,
                gesture.current_button() as i32,
                x,
                y,
                gesture.current_event_time(),
            );
        });
    }
    {
        let area = area.clone();
        let window = window.clone();
        let main_area = main_area.clone();
        let main_state = Rc::clone(&main_state);
        click.connect_released(move |_gesture, _n_press, x, y| {
            let (x, y) = panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            main_state.borrow_mut().set_panel_dragging(kind, false);
            area.queue_draw();
            let action = if kind == PanelKind::Equalizer {
                let title_action = main_state.borrow_mut().panel_click(kind, x, y);
                if title_action == PanelAction::None {
                    main_state.borrow_mut().equalizer_release(x, y)
                } else {
                    title_action
                }
            } else if main_state.borrow_mut().playlist_scrollbar_release() {
                PanelAction::Changed
            } else if main_state.borrow().playlist_menu_pressed() {
                main_state.borrow_mut().playlist_release(x, y)
            } else if main_state.borrow_mut().playlist_entry_release() {
                PanelAction::Changed
            } else {
                main_state.borrow_mut().panel_click(kind, x, y)
            };
            apply_panel_action(
                action,
                GtkPanelActionContext {
                    window: &window,
                    area: &area,
                    main_state: &main_state,
                    open_location_window: open_location_window.as_ref(),
                    playlist_sort_menu: playlist_sort_menu.as_ref(),
                    equalizer_presets_menu: equalizer_presets_menu.as_ref(),
                    equalizer_presets_docked: false,
                },
                || {
                    sync_single_panel_window_from_state(kind, &window, &area, &main_state);
                    main_area.queue_draw();
                },
            );
        });
    }
    window.add_controller(click);

    let floating_playlist_resize_drag = Rc::new(Cell::new(None::<(i32, i32, f64)>));
    let resize_drag = gtk::GestureDrag::new();
    resize_drag.set_button(1);
    resize_drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        let main_state = Rc::clone(&main_state);
        let floating_playlist_resize_drag = Rc::clone(&floating_playlist_resize_drag);
        resize_drag.connect_drag_begin(move |gesture, start_x, start_y| {
            if kind != PanelKind::Playlist {
                floating_playlist_resize_drag.set(None);
                return;
            }
            let state = main_state.borrow();
            let (base_x, base_y) =
                panel_event_to_base_coords(kind, &area, &state, start_x, start_y);
            if !state.is_panel_detached(kind) || !state.playlist_resize_region(base_x, base_y) {
                floating_playlist_resize_drag.set(None);
                return;
            }
            let (start_width, start_height) = state.playlist_size();
            floating_playlist_resize_drag.set(Some((
                start_width,
                start_height,
                state.scale_factor(),
            )));
            gesture.set_state(gtk::EventSequenceState::Claimed);
            area.queue_draw();
        });
    }
    {
        let area = area.clone();
        let window = window.clone();
        let main_state = Rc::clone(&main_state);
        let main_area = main_area.clone();
        let floating_playlist_resize_drag = Rc::clone(&floating_playlist_resize_drag);
        resize_drag.connect_drag_update(move |_gesture, offset_x, offset_y| {
            let Some((start_width, start_height, scale)) = floating_playlist_resize_drag.get()
            else {
                return;
            };
            let base_delta_x = (offset_x / scale).round() as i32;
            let base_delta_y = (offset_y / scale).round() as i32;
            let changed = main_state
                .borrow_mut()
                .set_playlist_size(start_width + base_delta_x, start_height + base_delta_y);
            if changed {
                sync_single_panel_window_from_state(kind, &window, &area, &main_state);
                main_area.queue_draw();
                area.queue_draw();
            }
        });
    }
    {
        let area = area.clone();
        let window = window.clone();
        let main_state = Rc::clone(&main_state);
        let main_area = main_area.clone();
        let floating_playlist_resize_drag = Rc::clone(&floating_playlist_resize_drag);
        resize_drag.connect_drag_end(move |_gesture, _offset_x, _offset_y| {
            if floating_playlist_resize_drag.take().is_none() {
                return;
            }
            sync_single_panel_window_from_state(kind, &window, &area, &main_state);
            main_area.queue_draw();
            area.queue_draw();
        });
    }
    window.add_controller(resize_drag);

    let motion = gtk::EventControllerMotion::new();
    motion.set_propagation_phase(gtk::PropagationPhase::Capture);
    let panel_hover_base = Rc::new(Cell::new(None::<(i32, i32)>));
    {
        let area = area.clone();
        let main_state = Rc::clone(&main_state);
        let panel_hover_base = Rc::clone(&panel_hover_base);
        motion.connect_motion(move |_motion, x, y| {
            let (x, y) = panel_event_to_base_coords(kind, &area, &main_state.borrow(), x, y);
            panel_hover_base.set(Some((x, y)));
            match kind {
                PanelKind::Equalizer => {
                    if main_state.borrow_mut().equalizer_motion(x, y) {
                        area.queue_draw();
                    }
                }
                PanelKind::Playlist => {
                    let scrolled = main_state.borrow_mut().playlist_scrollbar_motion(x, y);
                    let menu_changed = main_state.borrow_mut().playlist_motion(x, y);
                    if scrolled || menu_changed {
                        area.queue_draw();
                    }
                }
            }
        });
    }
    window.add_controller(motion);

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let area = area.clone();
        let main_state = Rc::clone(&main_state);
        let panel_hover_base = Rc::clone(&panel_hover_base);
        scroll.connect_scroll(move |scroll, _dx, dy| {
            let hover = panel_hover_base.get().or_else(|| {
                scroll
                    .current_event()
                    .and_then(|event| event.position())
                    .map(|(event_x, event_y)| {
                        panel_event_to_base_coords(
                            kind,
                            &area,
                            &main_state.borrow(),
                            event_x,
                            event_y,
                        )
                    })
            });
            let Some((x, y)) = hover else {
                return gtk::glib::Propagation::Proceed;
            };
            let changed = match kind {
                PanelKind::Equalizer => main_state.borrow_mut().equalizer_scroll(x, y, dy),
                PanelKind::Playlist => main_state.borrow_mut().playlist_scroll(dy),
            };
            if changed {
                area.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(scroll);

    {
        let area = area.clone();
        let main_state = Rc::clone(&main_state);
        window.connect_is_active_notify(move |window| {
            main_state
                .borrow_mut()
                .set_panel_focused(kind, window.is_active());
            area.queue_draw();
        });
    }
}

fn build_prompt_window(
    app: &gtk::Application,
    parent: &gtk::ApplicationWindow,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    kind: PromptKind,
) -> gtk::ApplicationWindow {
    let window = skinned_application_window(app, kind.title(), 360, 110, &[]);
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_resizable(false);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.add_css_class("xmms-skinned-window");
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let entry = gtk::Entry::builder()
        .placeholder_text(kind.placeholder())
        .build();
    content.append(&entry);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let cancel = gtk::Button::with_label("Cancel");
    let ok = gtk::Button::with_label("OK");
    buttons.append(&cancel);
    buttons.append(&ok);
    content.append(&buttons);
    window.set_child(Some(&content));

    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.hide());
    }
    {
        let window = window.clone();
        let entry = entry.clone();
        let main_state = Rc::clone(main_state);
        ok.connect_clicked(move |_| {
            kind.accept(&mut main_state.borrow_mut(), entry.text().as_str());
            window.hide();
        });
    }
    {
        let main_state = Rc::clone(main_state);
        window.connect_close_request(move |window| {
            kind.set_visible(&mut main_state.borrow_mut(), false);
            window.hide();
            gtk::glib::Propagation::Stop
        });
    }
    {
        let main_state = Rc::clone(main_state);
        window.connect_hide(move |_| {
            kind.set_visible(&mut main_state.borrow_mut(), false);
        });
    }
    window
}

fn build_skin_browser_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) -> gtk::ApplicationWindow {
    let window = skinned_application_window(app, "Skin selector", 300, 280, &[]);
    let add = gtk::Button::with_label("Add...");
    add.set_widget_name(SKIN_BROWSER_ADD_WIDGET);
    let close = gtk::Button::with_label("Close");
    close.set_widget_name(SKIN_BROWSER_CLOSE_WIDGET);
    let (content, list) = build_skin_browser_content(&add, &close);
    window.set_child(Some(&content));
    let populating = Rc::new(Cell::new(false));

    {
        let window = window.clone();
        let main_state = Rc::clone(main_state);
        let list = list.clone();
        let populating = Rc::clone(&populating);
        add.connect_clicked(move |_| {
            show_add_skin_dialog(
                &window,
                Rc::clone(&main_state),
                list.clone(),
                Rc::clone(&populating),
            );
        });
    }
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.hide());
    }
    {
        let main_state = Rc::clone(main_state);
        let populating = Rc::clone(&populating);
        let list = list.clone();
        window.connect_show(move |_| {
            let dirs = runtime_skin_browser_dirs();
            populating.set(true);
            if let Err(err) = refresh_skin_browser_list(&list, &mut main_state.borrow_mut(), &dirs)
            {
                eprintln!("xmms-rs: failed to scan skins: {err}");
            }
            populating.set(false);
        });
    }
    connect_skin_browser_selection(
        &list,
        main_state,
        main_area,
        equalizer_area,
        playlist_area,
        &populating,
    );
    bind_visibility_window!(&window, main_state, set_skin_browser_visible);
    window
}

fn build_skin_editor_window(
    app: &gtk::Application,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) -> gtk::ApplicationWindow {
    let window = skinned_application_window(app, "Skin Editor (alpha)", 980, 700, &[]);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    root.add_css_class("xmms-skinned-window");
    root.set_margin_top(8);
    root.set_margin_bottom(8);
    root.set_margin_start(8);
    root.set_margin_end(8);

    let canvas = gtk::DrawingArea::builder().focusable(true).build();
    update_skin_editor_canvas_size(&canvas, &main_state.borrow());
    {
        let main_state = Rc::clone(main_state);
        canvas.set_draw_func(move |_area, cr, _width, _height| {
            if let Err(err) = draw_skin_editor_canvas(cr, &main_state.borrow()) {
                eprintln!("xmms-rs: failed to draw skin editor: {err}");
            }
        });
    }

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hexpand(true);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&canvas));
    root.append(&scrolled);

    let (tools, zoom_scale, color_controls) = build_skin_editor_tools(
        &window,
        &canvas,
        main_state,
        main_area,
        equalizer_area,
        playlist_area,
    );
    let tool_scroller = gtk::ScrolledWindow::new();
    tool_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    tool_scroller.set_hexpand(false);
    tool_scroller.set_halign(gtk::Align::Start);
    tool_scroller.set_propagate_natural_width(false);
    tool_scroller.set_min_content_width(SKIN_EDITOR_SIDEBAR_WIDTH);
    tool_scroller.set_max_content_width(SKIN_EDITOR_SIDEBAR_WIDTH);
    tool_scroller.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);
    tool_scroller.set_child(Some(&tools));
    root.append(&tool_scroller);

    let pan_drag = Rc::new(Cell::new(None::<SkinEditorPanDrag>));
    let canvas_hadjustment = scrolled.hadjustment();
    let canvas_vadjustment = scrolled.vadjustment();

    let click = gtk::GestureClick::new();
    click.set_button(1);
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let color_controls = color_controls.clone();
        let pan_drag = Rc::clone(&pan_drag);
        let canvas_hadjustment = canvas_hadjustment.clone();
        let canvas_vadjustment = canvas_vadjustment.clone();
        click.connect_pressed(move |gesture, _n_press, x, y| {
            if main_state.borrow().skin_editor().tool == Tool::Drag {
                let (start_x, start_y) = gesture
                    .current_event()
                    .and_then(|event| event.position())
                    .unwrap_or((x, y));
                pan_drag.set(Some(SkinEditorPanDrag {
                    start_x,
                    start_y,
                    start_hadjustment: canvas_hadjustment.value(),
                    start_vadjustment: canvas_vadjustment.value(),
                }));
                return;
            }
            let (changed, picked_color) = {
                let mut state = main_state.borrow_mut();
                let previous_color = state.skin_editor().color;
                let slots = state.skin_editor.layout(state.active_skin());
                let mut editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.press(state.active_skin_mut(), &slots, x, y);
                let picked_color = (editor.color != previous_color).then_some(editor.color);
                state.skin_editor = editor;
                (changed, picked_color)
            };
            if let Some(color) = picked_color {
                set_skin_editor_color_controls(&color_controls, color);
            }
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let color_controls = color_controls.clone();
        let pan_drag = Rc::clone(&pan_drag);
        click.connect_released(move |_gesture, _n_press, x, y| {
            if pan_drag.take().is_some() {
                return;
            }
            let (changed, picked_color) = {
                let mut state = main_state.borrow_mut();
                let previous_color = state.skin_editor().color;
                let slots = state.skin_editor.layout(state.active_skin());
                let mut editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.release(state.active_skin_mut(), &slots, x, y);
                let picked_color = (editor.color != previous_color).then_some(editor.color);
                state.skin_editor = editor;
                (changed, picked_color)
            };
            if let Some(color) = picked_color {
                set_skin_editor_color_controls(&color_controls, color);
            }
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    canvas.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let color_controls = color_controls.clone();
        let pan_drag = Rc::clone(&pan_drag);
        let canvas_hadjustment = canvas_hadjustment.clone();
        let canvas_vadjustment = canvas_vadjustment.clone();
        motion.connect_motion(move |motion, x, y| {
            if let Some(pan) = pan_drag.get() {
                let (current_x, current_y) = motion
                    .current_event()
                    .and_then(|event| event.position())
                    .unwrap_or((x, y));
                set_adjustment_value(
                    &canvas_hadjustment,
                    pan.start_hadjustment + pan.start_x - current_x,
                );
                set_adjustment_value(
                    &canvas_vadjustment,
                    pan.start_vadjustment + pan.start_y - current_y,
                );
                return;
            }
            let (changed, picked_color) = {
                let mut state = main_state.borrow_mut();
                let previous_color = state.skin_editor().color;
                let slots = state.skin_editor.layout(state.active_skin());
                let mut editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.drag(state.active_skin_mut(), &slots, x, y);
                let picked_color = (editor.color != previous_color).then_some(editor.color);
                state.skin_editor = editor;
                (changed, picked_color)
            };
            if let Some(color) = picked_color {
                set_skin_editor_color_controls(&color_controls, color);
            }
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    canvas.add_controller(motion);

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let zoom_scale = zoom_scale.clone();
        scroll.connect_scroll(move |_scroll, _dx, dy| {
            let zoom = {
                let mut state = main_state.borrow_mut();
                let current = state.skin_editor().zoom;
                let zoom = if dy < 0.0 {
                    current + ZOOM_STEP
                } else if dy > 0.0 {
                    current - ZOOM_STEP
                } else {
                    current
                };
                state.skin_editor_mut().set_zoom(zoom);
                update_skin_editor_canvas_size(&canvas, &state);
                state.skin_editor().zoom
            };
            if (zoom_scale.value() - zoom).abs() > f64::EPSILON {
                zoom_scale.set_value(zoom);
            }
            canvas.queue_draw();
            gtk::glib::Propagation::Stop
        });
    }
    canvas.add_controller(scroll);

    bind_visibility_window!(&window, main_state, set_skin_editor_visible);

    window.set_child(Some(&root));
    window
}

#[derive(Clone)]
struct SkinEditorColorControls {
    chooser: gtk::ColorChooserWidget,
    button: gtk::Button,
}

#[derive(Clone, Copy)]
struct SkinEditorPanDrag {
    start_x: f64,
    start_y: f64,
    start_hadjustment: f64,
    start_vadjustment: f64,
}

fn build_skin_editor_tools(
    window: &gtk::ApplicationWindow,
    canvas: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) -> (gtk::Box, gtk::Scale, SkinEditorColorControls) {
    let tools = gtk::Box::new(gtk::Orientation::Vertical, 8);
    tools.set_hexpand(false);
    tools.set_halign(gtk::Align::Start);
    tools.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);

    let title = gtk::Label::new(Some("Tools"));
    title.set_xalign(0.0);
    tools.append(&title);

    let tool_flow = gtk::FlowBox::new();
    tool_flow.set_selection_mode(gtk::SelectionMode::None);
    tool_flow.set_min_children_per_line(1);
    tool_flow.set_max_children_per_line(SKIN_EDITOR_COLOR_SHELF_COLUMNS as u32);
    tool_flow.set_column_spacing(SKIN_EDITOR_COLOR_SHELF_GAP as u32);
    tool_flow.set_row_spacing(SKIN_EDITOR_COLOR_SHELF_GAP as u32);
    tool_flow.set_hexpand(false);
    let mut tool_group = None;
    for tool in [
        Tool::Brush,
        Tool::SprayCan,
        Tool::Fill,
        Tool::Line,
        Tool::Rectangle,
        Tool::Selection,
        Tool::Lighten,
        Tool::Darken,
        Tool::Dither,
        Tool::ColorPicker,
        Tool::Drag,
    ] {
        let button =
            append_skin_editor_tool(&tool_flow, tool_group.as_ref(), tool, main_state, canvas);
        if tool_group.is_none() {
            tool_group = Some(button);
        }
    }
    tools.append(&tool_flow);

    let fill_rectangle = gtk::CheckButton::with_label("Fill rectangle");
    fill_rectangle.set_active(true);
    {
        let main_state = Rc::clone(main_state);
        fill_rectangle.connect_toggled(move |button| {
            main_state.borrow_mut().skin_editor_mut().fill_rectangle = button.is_active();
        });
    }
    tools.append(&fill_rectangle);

    let brush_size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 15.0, 1.0);
    brush_size.set_hexpand(false);
    brush_size.set_halign(gtk::Align::Start);
    brush_size.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);
    brush_size.set_value(1.0);
    brush_size.set_digits(0);
    brush_size.set_draw_value(true);
    append_labeled_control(&tools, "Brush size (1-15)", &brush_size);
    {
        let main_state = Rc::clone(main_state);
        brush_size.connect_value_changed(move |scale| {
            main_state
                .borrow_mut()
                .skin_editor_mut()
                .set_brush_size(scale.value().round().clamp(1.0, 15.0) as u32);
        });
    }

    let zoom = gtk::Scale::with_range(gtk::Orientation::Horizontal, MIN_ZOOM, MAX_ZOOM, ZOOM_STEP);
    zoom.set_hexpand(false);
    zoom.set_halign(gtk::Align::Start);
    zoom.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);
    zoom.set_value(main_state.borrow().skin_editor().zoom);
    zoom.set_digits(2);
    zoom.set_draw_value(true);
    append_labeled_control(&tools, "Zoom (1x-10x)", &zoom);
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        zoom.connect_value_changed(move |scale| {
            {
                let mut state = main_state.borrow_mut();
                state
                    .skin_editor_mut()
                    .set_zoom(scale.value().clamp(MIN_ZOOM, MAX_ZOOM));
                update_skin_editor_canvas_size(&canvas, &state);
            }
            canvas.queue_draw();
        });
    }

    let color_button = gtk::Button::with_label("Custom color");
    color_button.set_hexpand(false);
    color_button.set_halign(gtk::Align::Start);
    color_button.set_size_request(SKIN_EDITOR_SIDEBAR_WIDTH, -1);
    color_button.set_tooltip_text(Some("Open custom color chooser"));
    style_skin_editor_custom_color_button(&color_button, [0, 0, 0, 255]);
    append_labeled_control(&tools, "Color", &color_button);

    let color_popover = gtk::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .build();
    color_popover.set_parent(&color_button);
    let color = gtk::ColorChooserWidget::new();
    color.set_rgba(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
    color.set_use_alpha(true);
    color.set_show_editor(true);
    color.set_size_request(220, 200);
    color_popover.set_child(Some(&color));
    {
        let color_popover = color_popover.clone();
        color_button.connect_clicked(move |_| {
            color_popover.popup();
        });
    }
    {
        let color_button = color_button.clone();
        let main_state = Rc::clone(main_state);
        color.connect_rgba_notify(move |chooser| {
            let rgba = rgba_to_u8(chooser.rgba());
            main_state.borrow_mut().skin_editor_mut().color = rgba;
            style_skin_editor_custom_color_button(&color_button, rgba);
        });
    }

    let color_controls = SkinEditorColorControls {
        chooser: color.clone(),
        button: color_button.clone(),
    };
    build_skin_editor_color_shelf(&tools, &color_controls, main_state);
    build_skin_editor_gradient_control(&tools, &color_controls, main_state);

    let couple_controls = gtk::CheckButton::with_label("Couple frames");
    couple_controls.set_tooltip_text(Some(
        "When painting volume, balance, equalizer, or shaded equalizer snippets, apply the same edit to later value frames",
    ));
    {
        let main_state = Rc::clone(main_state);
        couple_controls.connect_toggled(move |button| {
            main_state
                .borrow_mut()
                .skin_editor_mut()
                .couple_control_edits = button.is_active();
        });
    }
    tools.append(&couple_controls);

    let couple_gradient = gtk::CheckButton::with_label("Gradient coupling");
    couple_gradient.set_tooltip_text(Some(
        "When coupled edits are enabled, color each coupled value frame from the gradient",
    ));
    {
        let main_state = Rc::clone(main_state);
        couple_gradient.connect_toggled(move |button| {
            main_state
                .borrow_mut()
                .skin_editor_mut()
                .coupled_edits_use_gradient = button.is_active();
        });
    }
    tools.append(&couple_gradient);

    let transparent = gtk::CheckButton::with_label("Paint transparent");
    {
        let main_state = Rc::clone(main_state);
        let color = color.clone();
        let color_button = color_button.clone();
        transparent.connect_toggled(move |button| {
            let mut state = main_state.borrow_mut();
            if button.is_active() {
                state.skin_editor_mut().color[3] = 0;
                let color = state.skin_editor().color;
                style_skin_editor_custom_color_button(&color_button, color);
            } else {
                let rgba = rgba_to_u8(color.rgba());
                state.skin_editor_mut().color = rgba;
                style_skin_editor_custom_color_button(&color_button, rgba);
            }
        });
    }
    tools.append(&transparent);

    let selection_actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let copy = gtk::Button::with_label("Copy");
    let cut = gtk::Button::with_label("Cut");
    let paste = gtk::Button::with_label("Paste");
    {
        let main_state = Rc::clone(main_state);
        copy.connect_clicked(move |_| {
            let mut state = main_state.borrow_mut();
            let mut editor = std::mem::take(&mut state.skin_editor);
            editor.copy_selection(state.active_skin());
            state.skin_editor = editor;
        });
    }
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        cut.connect_clicked(move |_| {
            let changed = {
                let mut state = main_state.borrow_mut();
                let mut editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.cut_selection(state.active_skin_mut());
                state.skin_editor = editor;
                changed
            };
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        paste.connect_clicked(move |_| {
            let changed = {
                let mut state = main_state.borrow_mut();
                let editor = std::mem::take(&mut state.skin_editor);
                let changed = editor.paste_clipboard(state.active_skin_mut());
                state.skin_editor = editor;
                changed
            };
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    selection_actions.append(&copy);
    selection_actions.append(&cut);
    selection_actions.append(&paste);
    tools.append(&selection_actions);

    tools.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let colors_title = gtk::Label::new(Some("Skin color swatches"));
    colors_title.set_xalign(0.0);
    tools.append(&colors_title);
    build_skin_color_swatches(
        &tools,
        canvas,
        main_state,
        main_area,
        equalizer_area,
        playlist_area,
    );

    let name = gtk::Entry::new();
    let initial_name = main_state.borrow().skin_editor().working_name.clone();
    name.set_text(&initial_name);
    append_labeled_control(&tools, "Skin name", &name);
    {
        let main_state = Rc::clone(main_state);
        name.connect_changed(move |entry| {
            main_state.borrow_mut().skin_editor_mut().working_name = entry.text().to_string();
        });
    }

    let clone_current = gtk::Button::with_label("Clone Current");
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let name = name.clone();
        clone_current.connect_clicked(move |_| {
            let cloned_name = {
                let mut state = main_state.borrow_mut();
                match state.clone_configured_skin_for_editor() {
                    Ok(()) => Some(state.skin_editor().working_name.clone()),
                    Err(err) => {
                        eprintln!("xmms-rs: failed to clone skin for editor: {err}");
                        None
                    }
                }
            };
            if let Some(cloned_name) = cloned_name {
                name.set_text(&cloned_name);
                refresh_xmms_skin_css(main_state.borrow().active_skin());
                queue_skin_editor_areas(&canvas, &main_area, &equalizer_area, &playlist_area, true);
            }
        });
    }
    tools.append(&clone_current);

    let save = gtk::Button::with_label("Save");
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        save.connect_clicked(move |_| {
            if let Err(err) = main_state.borrow_mut().save_editor_skin_to_user_dir() {
                eprintln!("xmms-rs: failed to save edited skin: {err}");
            }
            refresh_xmms_skin_css(main_state.borrow().active_skin());
            queue_skin_editor_areas(&canvas, &main_area, &equalizer_area, &playlist_area, true);
        });
    }
    tools.append(&save);

    let export = gtk::Button::with_label("Export .wsz");
    {
        let window = window.clone();
        let main_state = Rc::clone(main_state);
        export.connect_clicked(move |_| {
            show_skin_editor_export_dialog(&window, Rc::clone(&main_state));
        });
    }
    tools.append(&export);

    (tools, zoom, color_controls)
}

fn append_skin_editor_tool(
    parent: &gtk::FlowBox,
    group: Option<&gtk::ToggleButton>,
    tool: Tool,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    canvas: &gtk::DrawingArea,
) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::with_label(skin_editor_tool_icon(tool));
    if let Some(group) = group {
        button.set_group(Some(group));
    }
    button.set_active(tool == Tool::Brush);
    button.set_tooltip_text(Some(skin_editor_tool_name(tool)));
    button.set_size_request(
        SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
        SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
    );
    button.set_hexpand(false);
    button.set_vexpand(false);
    {
        let main_state = Rc::clone(main_state);
        let canvas = canvas.clone();
        button.connect_toggled(move |button| {
            if button.is_active() {
                main_state.borrow_mut().skin_editor_mut().tool = tool;
                canvas.queue_draw();
            }
        });
    }
    parent.insert(&button, -1);
    button
}

fn skin_editor_tool_icon(tool: Tool) -> &'static str {
    match tool {
        Tool::Brush => "🖌️",
        Tool::SprayCan => "💨",
        Tool::Fill => "🪣",
        Tool::Line => "📏",
        Tool::Rectangle => "🟦",
        Tool::Selection => "🔲",
        Tool::Lighten => "🔆",
        Tool::Darken => "🌙",
        Tool::Dither => "🏁",
        Tool::ColorPicker => "🧪",
        Tool::Drag => "✋",
    }
}

fn skin_editor_tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Brush => "Brush",
        Tool::SprayCan => "Spraycan",
        Tool::Fill => "Color fill",
        Tool::Line => "Line",
        Tool::Rectangle => "Rectangle",
        Tool::Selection => "Select rectangle",
        Tool::Lighten => "Lighten",
        Tool::Darken => "Darken",
        Tool::Dither => "Dither checker brush",
        Tool::ColorPicker => "Color picker",
        Tool::Drag => "Drag canvas",
    }
}

fn set_adjustment_value(adjustment: &gtk::Adjustment, value: f64) {
    let upper = adjustment.upper() - adjustment.page_size();
    adjustment.set_value(value.clamp(adjustment.lower(), upper.max(adjustment.lower())));
}

fn build_skin_editor_color_shelf(
    parent: &gtk::Box,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let label = gtk::Label::new(Some("Color shelf"));
    label.set_xalign(0.0);
    parent.append(&label);

    let grid = gtk::Grid::new();
    grid.set_column_spacing(SKIN_EDITOR_COLOR_SHELF_GAP as u32);
    grid.set_row_spacing(SKIN_EDITOR_COLOR_SHELF_GAP as u32);
    let initial_shelf = main_state.borrow().skin_editor().color_shelf;
    for index in 0..COLOR_SHELF_SIZE {
        let color = initial_shelf[index];
        let button = gtk::Button::new();
        button.set_size_request(
            SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
            SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
        );
        button.set_tooltip_text(Some(
            "Left click picks; middle or right click stores current color",
        ));
        style_color_shelf_button(&button, color);

        {
            let main_state = Rc::clone(main_state);
            let color_controls = color_controls.clone();
            button.connect_clicked(move |_| {
                let picked = main_state
                    .borrow_mut()
                    .skin_editor_mut()
                    .pick_color_shelf_slot(index);
                if let Some(picked) = picked {
                    set_skin_editor_color_controls(&color_controls, picked);
                }
            });
        }

        for button_number in [2, 3] {
            let click = gtk::GestureClick::new();
            click.set_button(button_number);
            {
                let main_state = Rc::clone(main_state);
                let shelf_button = button.clone();
                click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                    let stored = main_state
                        .borrow_mut()
                        .skin_editor_mut()
                        .store_color_shelf_slot(index);
                    style_color_shelf_button(&shelf_button, stored);
                });
            }
            button.add_controller(click);
        }

        grid.attach(
            &button,
            (index % SKIN_EDITOR_COLOR_SHELF_COLUMNS) as i32,
            (index / SKIN_EDITOR_COLOR_SHELF_COLUMNS) as i32,
            1,
            1,
        );
    }
    parent.append(&grid);
}

fn build_skin_editor_gradient_control(
    parent: &gtk::Box,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let label = gtk::Label::new(Some("Gradient"));
    label.set_xalign(0.0);
    label.set_margin_top(4);
    parent.append(&label);

    let gradient = gtk::DrawingArea::builder()
        .content_width(SKIN_EDITOR_GRADIENT_WIDTH)
        .content_height(SKIN_EDITOR_GRADIENT_HEIGHT)
        .build();
    gradient.set_size_request(SKIN_EDITOR_GRADIENT_WIDTH, SKIN_EDITOR_GRADIENT_HEIGHT);
    gradient.set_tooltip_text(Some(
        "Click to pick an interpolated color; use + Stop to add editable colors",
    ));
    {
        let main_state = Rc::clone(main_state);
        gradient.set_draw_func(move |_area, cr, width, height| {
            let state = main_state.borrow();
            if let Err(err) =
                draw_skin_editor_gradient(cr, width, height, &state.skin_editor().gradient)
            {
                eprintln!("xmms-rs: failed to draw skin editor gradient: {err}");
            }
        });
    }
    {
        let gradient_for_click = gradient.clone();
        let main_state = Rc::clone(main_state);
        let color_controls = color_controls.clone();
        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.connect_pressed(move |_gesture, _n_press, x, _y| {
            let width = f64::from(gradient_for_click.allocated_width().max(1));
            let fraction = (x / (width - 1.0).max(1.0)).clamp(0.0, 1.0);
            let picked = main_state
                .borrow_mut()
                .skin_editor_mut()
                .pick_gradient_color_at(fraction);
            set_skin_editor_color_controls(&color_controls, picked);
        });
        gradient.add_controller(click);
    }
    parent.append(&gradient);

    let stop_list = gtk::Box::new(gtk::Orientation::Vertical, 3);
    build_gradient_stop_list(&stop_list, &gradient, color_controls, main_state);
    parent.append(&stop_list);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let add_stop = gtk::Button::with_label("+ Stop");
    let remove_stop = gtk::Button::with_label("Remove");
    let reverse = gtk::Button::with_label("Reverse");
    actions.append(&add_stop);
    actions.append(&remove_stop);
    actions.append(&reverse);
    parent.append(&actions);

    {
        let stop_list = stop_list.clone();
        let gradient = gradient.clone();
        let main_state = Rc::clone(main_state);
        let color_controls = color_controls.clone();
        add_stop.connect_clicked(move |_| {
            let color = {
                let mut state = main_state.borrow_mut();
                state.skin_editor_mut().add_gradient_stop(0.5);
                state.skin_editor().color
            };
            set_skin_editor_color_controls(&color_controls, color);
            gradient.queue_draw();
            build_gradient_stop_list(&stop_list, &gradient, &color_controls, &main_state);
        });
    }
    {
        let stop_list = stop_list.clone();
        let gradient = gradient.clone();
        let main_state = Rc::clone(main_state);
        let color_controls = color_controls.clone();
        remove_stop.connect_clicked(move |_| {
            {
                let mut state = main_state.borrow_mut();
                let index = state.skin_editor().selected_gradient_stop();
                state.skin_editor_mut().remove_gradient_stop(index);
            }
            gradient.queue_draw();
            build_gradient_stop_list(&stop_list, &gradient, &color_controls, &main_state);
        });
    }
    {
        let stop_list = stop_list.clone();
        let gradient = gradient.clone();
        let main_state = Rc::clone(main_state);
        let color_controls = color_controls.clone();
        reverse.connect_clicked(move |_| {
            main_state.borrow_mut().skin_editor_mut().reverse_gradient();
            gradient.queue_draw();
            build_gradient_stop_list(&stop_list, &gradient, &color_controls, &main_state);
        });
    }

    let shelf_label = gtk::Label::new(Some("Gradient shelf"));
    shelf_label.set_xalign(0.0);
    parent.append(&shelf_label);
    build_gradient_shelf(parent, &gradient, &stop_list, color_controls, main_state);
}

fn build_gradient_stop_list(
    stop_list: &gtk::Box,
    gradient: &gtk::DrawingArea,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    while let Some(child) = stop_list.first_child() {
        stop_list.remove(&child);
    }

    let (stops, selected) = {
        let state = main_state.borrow();
        (
            state.skin_editor().gradient_stops().to_vec(),
            state.skin_editor().selected_gradient_stop(),
        )
    };

    let stop_count = stops.len();
    for (index, stop) in stops.into_iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let label = gtk::Label::new(Some(if index == selected { "●" } else { "○" }));
        row.append(&label);

        let position = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.01);
        position.set_value(stop.position);
        position.set_digits(2);
        position.set_draw_value(true);
        position.set_hexpand(true);
        position.set_width_request(82);
        position.set_sensitive(index != 0 && index + 1 != stop_count);
        {
            let gradient = gradient.clone();
            let main_state = Rc::clone(main_state);
            position.connect_value_changed(move |scale| {
                main_state
                    .borrow_mut()
                    .skin_editor_mut()
                    .set_gradient_stop_position(index, scale.value());
                gradient.queue_draw();
            });
        }
        row.append(&position);

        let color_button = gtk::Button::new();
        color_button.set_size_request(28, 24);
        color_button.set_tooltip_text(Some(
            "Left click selects this stop; middle/right click stores current color",
        ));
        style_color_shelf_button(&color_button, Some(stop.color));
        {
            let main_state = Rc::clone(main_state);
            let color_controls = color_controls.clone();
            let stop_list = stop_list.clone();
            let gradient = gradient.clone();
            color_button.connect_clicked(move |_| {
                let picked = {
                    main_state
                        .borrow_mut()
                        .skin_editor_mut()
                        .select_gradient_stop(index)
                };
                if let Some(color) = picked {
                    set_skin_editor_color_controls(&color_controls, color);
                }
                queue_gradient_stop_list_rebuild(
                    &stop_list,
                    &gradient,
                    &color_controls,
                    &main_state,
                );
            });
        }
        for button_number in [2, 3] {
            let click = gtk::GestureClick::new();
            click.set_button(button_number);
            {
                let main_state = Rc::clone(main_state);
                let color_button = color_button.clone();
                let gradient = gradient.clone();
                click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                    {
                        let mut state = main_state.borrow_mut();
                        state.skin_editor_mut().selected_gradient_stop = index;
                        if let Some(color) = state.skin_editor_mut().store_selected_gradient_stop()
                        {
                            style_color_shelf_button(&color_button, Some(color));
                        }
                    }
                    gradient.queue_draw();
                });
            }
            color_button.add_controller(click);
        }
        row.append(&color_button);
        stop_list.append(&row);
    }
}

fn queue_gradient_stop_list_rebuild(
    stop_list: &gtk::Box,
    gradient: &gtk::DrawingArea,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let stop_list = stop_list.clone();
    let gradient = gradient.clone();
    let color_controls = color_controls.clone();
    let main_state = Rc::clone(main_state);
    gtk::glib::idle_add_local_once(move || {
        build_gradient_stop_list(&stop_list, &gradient, &color_controls, &main_state);
    });
}

fn build_gradient_shelf(
    parent: &gtk::Box,
    gradient: &gtk::DrawingArea,
    stop_list: &gtk::Box,
    color_controls: &SkinEditorColorControls,
    main_state: &Rc<RefCell<MainWindowUiState>>,
) {
    let shelf_grid = gtk::Grid::new();
    shelf_grid.set_column_spacing(4);
    shelf_grid.set_row_spacing(4);
    let shelf_areas: Rc<RefCell<Vec<gtk::DrawingArea>>> = Rc::new(RefCell::new(Vec::new()));
    for index in 0..GRADIENT_SHELF_SIZE {
        let area = gtk::DrawingArea::builder()
            .content_width(SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE)
            .content_height(SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE)
            .build();
        area.set_size_request(
            SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
            SKIN_EDITOR_COLOR_SHELF_BUTTON_SIZE,
        );
        area.set_tooltip_text(Some(
            "Left click loads; middle or right click stores current gradient",
        ));
        {
            let main_state = Rc::clone(main_state);
            area.set_draw_func(move |_area, cr, width, height| {
                let state = main_state.borrow();
                let gradient = state.skin_editor().gradient_shelf[index].as_ref();
                if let Err(err) = draw_gradient_shelf_slot(cr, width, height, gradient) {
                    eprintln!("xmms-rs: failed to draw gradient shelf slot: {err}");
                }
            });
        }
        {
            let main_state = Rc::clone(main_state);
            let gradient_area = gradient.clone();
            let stop_list = stop_list.clone();
            let color_controls = color_controls.clone();
            let shelf_areas = Rc::clone(&shelf_areas);
            let click = gtk::GestureClick::new();
            click.set_button(1);
            click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                let picked = main_state
                    .borrow_mut()
                    .skin_editor_mut()
                    .pick_gradient_shelf_slot(index);
                if picked.is_some() {
                    let color = main_state.borrow().skin_editor().color;
                    set_skin_editor_color_controls(&color_controls, color);
                    gradient_area.queue_draw();
                    build_gradient_stop_list(
                        &stop_list,
                        &gradient_area,
                        &color_controls,
                        &main_state,
                    );
                    for area in shelf_areas.borrow().iter() {
                        area.queue_draw();
                    }
                }
            });
            area.add_controller(click);
        }
        for button_number in [2, 3] {
            let click = gtk::GestureClick::new();
            click.set_button(button_number);
            {
                let main_state = Rc::clone(main_state);
                let area = area.clone();
                click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                    main_state
                        .borrow_mut()
                        .skin_editor_mut()
                        .store_gradient_shelf_slot(index);
                    area.queue_draw();
                });
            }
            area.add_controller(click);
        }
        shelf_grid.attach(
            &area,
            (index % SKIN_EDITOR_COLOR_SHELF_COLUMNS) as i32,
            (index / SKIN_EDITOR_COLOR_SHELF_COLUMNS) as i32,
            1,
            1,
        );
        shelf_areas.borrow_mut().push(area);
    }
    parent.append(&shelf_grid);
}

fn draw_gradient_shelf_slot(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    gradient: Option<&SkinGradient>,
) -> Result<(), cairo::Error> {
    if let Some(gradient) = gradient {
        draw_skin_editor_gradient(cr, width, height, gradient)
    } else {
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        cr.paint()?;
        cr.set_source_rgb(0.45, 0.45, 0.45);
        cr.set_dash(&[3.0, 2.0], 0.0);
        cr.rectangle(
            0.5,
            0.5,
            f64::from(width.max(1)) - 1.0,
            f64::from(height.max(1)) - 1.0,
        );
        let result = cr.stroke();
        cr.set_dash(&[], 0.0);
        result
    }
}

fn draw_skin_editor_gradient(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    skin_gradient: &SkinGradient,
) -> Result<(), cairo::Error> {
    let width = width.max(1);
    let height = height.max(1);
    draw_alpha_checkerboard(cr, width, height)?;

    let gradient = cairo::LinearGradient::new(0.0, 0.0, f64::from(width), 0.0);
    for stop in skin_gradient.stops() {
        let [r, g, b, a] = rgba_to_cairo(stop.color);
        gradient.add_color_stop_rgba(stop.position, r, g, b, a);
    }
    cr.set_source(&gradient)?;
    cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
    cr.fill()?;

    cr.set_source_rgb(0.12, 0.12, 0.12);
    cr.rectangle(0.5, 0.5, f64::from(width) - 1.0, f64::from(height) - 1.0);
    cr.stroke()
}

fn draw_alpha_checkerboard(
    cr: &cairo::Context,
    width: i32,
    height: i32,
) -> Result<(), cairo::Error> {
    const CELL: i32 = 6;
    cr.set_source_rgb(0.72, 0.72, 0.72);
    cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
    cr.fill()?;
    for y in (0..height).step_by(CELL as usize) {
        for x in (0..width).step_by(CELL as usize) {
            if ((x / CELL) + (y / CELL)) & 1 == 0 {
                cr.set_source_rgb(0.48, 0.48, 0.48);
                cr.rectangle(
                    f64::from(x),
                    f64::from(y),
                    f64::from(CELL.min(width - x)),
                    f64::from(CELL.min(height - y)),
                );
                cr.fill()?;
            }
        }
    }
    Ok(())
}

fn rgba_to_cairo([r, g, b, a]: [u8; 4]) -> [f64; 4] {
    [
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
        f64::from(a) / 255.0,
    ]
}

fn set_skin_editor_color_controls(controls: &SkinEditorColorControls, rgba: [u8; 4]) {
    controls.chooser.set_rgba(&rgba_from_u8(rgba));
    style_skin_editor_custom_color_button(&controls.button, rgba);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkinColorTarget {
    PlaylistNormal,
    PlaylistCurrent,
    PlaylistNormalBg,
    PlaylistSelectedBg,
    Visualization(usize),
    TextBackground(usize),
    TextForeground(usize),
}

impl SkinColorTarget {
    fn affects_playlist_colors(self) -> bool {
        matches!(
            self,
            Self::PlaylistNormal
                | Self::PlaylistCurrent
                | Self::PlaylistNormalBg
                | Self::PlaylistSelectedBg
        )
    }
}

fn build_skin_color_swatches(
    parent: &gtk::Box,
    canvas: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) {
    let playlist_grid = gtk::Grid::new();
    playlist_grid.set_column_spacing(4);
    playlist_grid.set_row_spacing(4);
    for (index, (label, target)) in [
        ("PL normal", SkinColorTarget::PlaylistNormal),
        ("PL current", SkinColorTarget::PlaylistCurrent),
        ("PL bg", SkinColorTarget::PlaylistNormalBg),
        ("PL selected", SkinColorTarget::PlaylistSelectedBg),
    ]
    .into_iter()
    .enumerate()
    {
        playlist_grid.attach(
            &skin_color_button(
                label,
                target,
                canvas,
                main_state,
                main_area,
                equalizer_area,
                playlist_area,
            ),
            (index % 2) as i32,
            (index / 2) as i32,
            1,
            1,
        );
    }
    parent.append(&playlist_grid);

    let vis_label = gtk::Label::new(Some("Visualizer colors"));
    vis_label.set_xalign(0.0);
    parent.append(&vis_label);
    let vis_grid = gtk::Grid::new();
    vis_grid.set_column_spacing(2);
    vis_grid.set_row_spacing(2);
    for index in 0..24 {
        vis_grid.attach(
            &skin_color_button(
                &(index + 1).to_string(),
                SkinColorTarget::Visualization(index),
                canvas,
                main_state,
                main_area,
                equalizer_area,
                playlist_area,
            ),
            (index % 6) as i32,
            (index / 6) as i32,
            1,
            1,
        );
    }
    parent.append(&vis_grid);

    let text_label = gtk::Label::new(Some("Text colors"));
    text_label.set_xalign(0.0);
    parent.append(&text_label);
    let text_grid = gtk::Grid::new();
    text_grid.set_column_spacing(2);
    text_grid.set_row_spacing(2);
    for index in 0..6 {
        text_grid.attach(
            &skin_color_button(
                &format!("BG{}", index + 1),
                SkinColorTarget::TextBackground(index),
                canvas,
                main_state,
                main_area,
                equalizer_area,
                playlist_area,
            ),
            0,
            index as i32,
            1,
            1,
        );
        text_grid.attach(
            &skin_color_button(
                &format!("FG{}", index + 1),
                SkinColorTarget::TextForeground(index),
                canvas,
                main_state,
                main_area,
                equalizer_area,
                playlist_area,
            ),
            1,
            index as i32,
            1,
            1,
        );
    }
    parent.append(&text_grid);
}

fn skin_color_button(
    label: &str,
    target: SkinColorTarget,
    canvas: &gtk::DrawingArea,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_tooltip_text(Some("Apply the current editor color to this skin color"));
    button.set_size_request(58, 28);
    style_skin_color_button(
        &button,
        skin_color_target_rgb(main_state.borrow().active_skin(), target),
    );
    {
        let canvas = canvas.clone();
        let main_state = Rc::clone(main_state);
        let main_area = main_area.clone();
        let equalizer_area = equalizer_area.clone();
        let playlist_area = playlist_area.clone();
        let swatch_button = button.clone();
        button.connect_clicked(move |_| {
            let (changed, rgb) = {
                let mut state = main_state.borrow_mut();
                let color = state.skin_editor().color;
                let rgb = [color[0], color[1], color[2]];
                let changed = apply_skin_color_target(state.active_skin_mut(), target, rgb);
                (changed, rgb)
            };
            style_skin_color_button(&swatch_button, rgb);
            if changed && target.affects_playlist_colors() {
                refresh_xmms_skin_css(main_state.borrow().active_skin());
            }
            queue_skin_editor_areas(
                &canvas,
                &main_area,
                &equalizer_area,
                &playlist_area,
                changed,
            );
        });
    }
    button
}

fn skin_color_target_rgb(skin: &DefaultSkin, target: SkinColorTarget) -> [u8; 3] {
    match target {
        SkinColorTarget::PlaylistNormal => skin.playlist_colors().normal,
        SkinColorTarget::PlaylistCurrent => skin.playlist_colors().current,
        SkinColorTarget::PlaylistNormalBg => skin.playlist_colors().normal_bg,
        SkinColorTarget::PlaylistSelectedBg => skin.playlist_colors().selected_bg,
        SkinColorTarget::Visualization(index) => {
            skin.vis_colors().get(index).copied().unwrap_or([0, 0, 0])
        }
        SkinColorTarget::TextBackground(index) => skin
            .text_colors()
            .background
            .get(index)
            .copied()
            .unwrap_or([0, 0, 0]),
        SkinColorTarget::TextForeground(index) => skin
            .text_colors()
            .foreground
            .get(index)
            .copied()
            .unwrap_or([255, 255, 255]),
    }
}

fn apply_skin_color_target(skin: &mut DefaultSkin, target: SkinColorTarget, rgb: [u8; 3]) -> bool {
    match target {
        SkinColorTarget::PlaylistNormal => {
            let mut colors = skin.playlist_colors();
            colors.normal = rgb;
            skin.set_playlist_colors(colors)
        }
        SkinColorTarget::PlaylistCurrent => {
            let mut colors = skin.playlist_colors();
            colors.current = rgb;
            skin.set_playlist_colors(colors)
        }
        SkinColorTarget::PlaylistNormalBg => {
            let mut colors = skin.playlist_colors();
            colors.normal_bg = rgb;
            skin.set_playlist_colors(colors)
        }
        SkinColorTarget::PlaylistSelectedBg => {
            let mut colors = skin.playlist_colors();
            colors.selected_bg = rgb;
            skin.set_playlist_colors(colors)
        }
        SkinColorTarget::Visualization(index) => skin.set_vis_color(index, rgb),
        SkinColorTarget::TextBackground(index) => {
            let mut colors = skin.text_colors();
            if let Some(color) = colors.background.get_mut(index) {
                *color = rgb;
                skin.set_text_colors(colors)
            } else {
                false
            }
        }
        SkinColorTarget::TextForeground(index) => {
            let mut colors = skin.text_colors();
            if let Some(color) = colors.foreground.get_mut(index) {
                *color = rgb;
                skin.set_text_colors(colors)
            } else {
                false
            }
        }
    }
}

fn append_labeled_control<W: IsA<gtk::Widget>>(parent: &gtk::Box, label: &str, child: &W) {
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    parent.append(&label);
    parent.append(child);
}

fn draw_skin_editor_canvas(cr: &cairo::Context, state: &MainWindowUiState) -> Result<(), String> {
    cr.set_source_rgb(0.12, 0.12, 0.12);
    cr.paint().map_err(|err| err.to_string())?;
    let editor = state.skin_editor();
    let zoom = editor.zoom.max(MIN_ZOOM);
    let slots = editor.layout(state.active_skin());

    cr.save().map_err(|err| err.to_string())?;
    cr.scale(zoom, zoom);
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(8.0);

    for slot in &slots {
        cr.set_source_rgb(0.85, 0.85, 0.85);
        cr.move_to(f64::from(slot.x), f64::from(slot.y - 3));
        cr.show_text(&format!(
            "{}  {}x{}",
            slot.kind.info().file_stem,
            slot.width,
            slot.height
        ))
        .map_err(|err| err.to_string())?;

        cr.set_source_rgb(0.25, 0.25, 0.25);
        cr.rectangle(
            f64::from(slot.x) - 1.0,
            f64::from(slot.y) - 1.0,
            f64::from(slot.width) + 2.0,
            f64::from(slot.height) + 2.0,
        );
        cr.stroke().map_err(|err| err.to_string())?;

        if let Some(image) = state.active_skin().get(slot.kind) {
            let surface = surface_from_xpm(image).map_err(|err| err.to_string())?;
            let mut cairo_surface = gtk::cairo::ImageSurface::create(
                gtk::cairo::Format::ARgb32,
                slot.width,
                slot.height,
            )
            .map_err(|err| err.to_string())?;
            {
                let source = surface.data().map_err(|err| err.to_string())?;
                let mut dest = cairo_surface.data().map_err(|err| err.to_string())?;
                dest.copy_from_slice(&source);
            }
            cairo_surface.mark_dirty();
            cr.save().map_err(|err| err.to_string())?;
            cr.rectangle(
                f64::from(slot.x),
                f64::from(slot.y),
                f64::from(slot.width),
                f64::from(slot.height),
            );
            cr.clip();
            cr.set_source_surface(&cairo_surface, f64::from(slot.x), f64::from(slot.y))
                .map_err(|err| err.to_string())?;
            cr.paint().map_err(|err| err.to_string())?;
            cr.restore().map_err(|err| err.to_string())?;
        }
    }

    if editor.zoom >= 8.0 {
        draw_skin_editor_grid(cr, &slots).map_err(|err| err.to_string())?;
    }
    draw_skin_editor_line_preview(cr, editor, &slots).map_err(|err| err.to_string())?;
    draw_skin_editor_rectangle_preview(cr, editor, &slots).map_err(|err| err.to_string())?;
    draw_skin_editor_selection_preview(cr, editor, &slots).map_err(|err| err.to_string())?;
    cr.restore().map_err(|err| err.to_string())?;
    Ok(())
}

fn draw_skin_editor_grid(cr: &cairo::Context, slots: &[ElementSlot]) -> Result<(), cairo::Error> {
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.12);
    cr.set_line_width(0.1);
    for slot in slots {
        for x in 0..=slot.width {
            cr.move_to(f64::from(slot.x + x), f64::from(slot.y));
            cr.line_to(f64::from(slot.x + x), f64::from(slot.y + slot.height));
        }
        for y in 0..=slot.height {
            cr.move_to(f64::from(slot.x), f64::from(slot.y + y));
            cr.line_to(f64::from(slot.x + slot.width), f64::from(slot.y + y));
        }
    }
    cr.stroke()
}

fn draw_skin_editor_rectangle_preview(
    cr: &cairo::Context,
    editor: &SkinEditorState,
    slots: &[ElementSlot],
) -> Result<(), cairo::Error> {
    let Some((kind, rect)) = editor.rectangle_preview() else {
        return Ok(());
    };
    let Some(slot) = slots.iter().find(|slot| slot.kind == kind) else {
        return Ok(());
    };
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
    cr.set_line_width(1.0);
    cr.rectangle(
        f64::from(slot.x + rect.x),
        f64::from(slot.y + rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    );
    cr.stroke()
}

fn draw_skin_editor_line_preview(
    cr: &cairo::Context,
    editor: &SkinEditorState,
    slots: &[ElementSlot],
) -> Result<(), cairo::Error> {
    let Some((kind, start, end)) = editor.line_preview() else {
        return Ok(());
    };
    let Some(slot) = slots.iter().find(|slot| slot.kind == kind) else {
        return Ok(());
    };
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
    cr.set_line_width(1.0);
    cr.move_to(
        f64::from(slot.x + start.0) + 0.5,
        f64::from(slot.y + start.1) + 0.5,
    );
    cr.line_to(
        f64::from(slot.x + end.0) + 0.5,
        f64::from(slot.y + end.1) + 0.5,
    );
    cr.stroke()
}

fn draw_skin_editor_selection_preview(
    cr: &cairo::Context,
    editor: &SkinEditorState,
    slots: &[ElementSlot],
) -> Result<(), cairo::Error> {
    let Some((kind, rect)) = editor.selection_preview() else {
        return Ok(());
    };
    let Some(slot) = slots.iter().find(|slot| slot.kind == kind) else {
        return Ok(());
    };
    cr.set_source_rgba(0.2, 0.7, 1.0, 0.9);
    cr.set_line_width(1.0);
    cr.set_dash(&[2.0, 2.0], 0.0);
    cr.rectangle(
        f64::from(slot.x + rect.x),
        f64::from(slot.y + rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    );
    let result = cr.stroke();
    cr.set_dash(&[], 0.0);
    result
}

fn update_skin_editor_canvas_size(canvas: &gtk::DrawingArea, state: &MainWindowUiState) {
    let slots = state.skin_editor().layout(state.active_skin());
    let (width, height) = state.skin_editor().canvas_size(&slots);
    canvas.set_content_width(width);
    canvas.set_content_height(height);
}

fn queue_skin_editor_areas(
    canvas: &gtk::DrawingArea,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
    skin_changed: bool,
) {
    canvas.queue_draw();
    if skin_changed {
        main_area.queue_draw();
        equalizer_area.queue_draw();
        playlist_area.queue_draw();
    }
}

fn rgba_to_u8(rgba: gtk::gdk::RGBA) -> [u8; 4] {
    [
        (rgba.red().clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (rgba.green().clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (rgba.blue().clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (rgba.alpha().clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
    ]
}

fn rgba_from_u8(rgba: [u8; 4]) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        f32::from(rgba[0]) / 255.0,
        f32::from(rgba[1]) / 255.0,
        f32::from(rgba[2]) / 255.0,
        f32::from(rgba[3]) / 255.0,
    )
}

fn show_skin_editor_export_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Export Skin"),
        Some(parent),
        gtk::FileChooserAction::Save,
        Some("Export"),
        Some("Cancel"),
    );
    dialog.set_current_name("skin.wsz");
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = dialog.file().and_then(|file| file.path()) {
                let path = ensure_wsz_extension(path);
                if let Err(err) = main_state.borrow().export_editor_skin_wsz(&path) {
                    eprintln!("xmms-rs: failed to export skin '{}': {err}", path.display());
                }
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn connect_skin_browser_selection(
    list: &gtk::ListBox,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    main_area: &gtk::DrawingArea,
    equalizer_area: &gtk::DrawingArea,
    playlist_area: &gtk::DrawingArea,
    populating: &Rc<Cell<bool>>,
) {
    let main_state = Rc::clone(main_state);
    let main_area = main_area.clone();
    let equalizer_area = equalizer_area.clone();
    let playlist_area = playlist_area.clone();
    let populating = Rc::clone(populating);
    list.connect_row_selected(move |list, row| {
        if populating.get() {
            return;
        }
        let Some(row) = row else {
            return;
        };
        let selected = row.index().max(0) as usize;
        if main_state.borrow_mut().select_skin_browser_index(selected) {
            refresh_xmms_skin_css(main_state.borrow().active_skin());
            main_area.queue_draw();
            equalizer_area.queue_draw();
            playlist_area.queue_draw();
        } else {
            populating.set(true);
            populate_skin_browser_list(list, &main_state.borrow());
            populating.set(false);
        }
    });
}

fn show_add_skin_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
    list: gtk::ListBox,
    populating: Rc<Cell<bool>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Add Skin"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Add"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let Some(path) = dialog.file().and_then(|file| file.path()) {
                let user_skin_dir = user_skin_import_dir();
                match import_skin_to_user_dir(&path, &user_skin_dir) {
                    Ok(imported) => {
                        let dirs = runtime_skin_browser_dirs();
                        let mut state = main_state.borrow_mut();
                        state.update_config_via_store(|config| {
                            config.skin = Some(imported.display().to_string());
                        });
                        if let Err(err) = state.reload_skin() {
                            eprintln!("xmms-rs: failed to load imported skin: {err}");
                            state.update_config_via_store(|config| config.skin = None);
                        }
                        refresh_xmms_skin_css(state.active_skin());
                        populating.set(true);
                        if let Err(err) = refresh_skin_browser_list(&list, &mut state, &dirs) {
                            eprintln!("xmms-rs: failed to refresh skins after import: {err}");
                        }
                        populating.set(false);
                    }
                    Err(err) => eprintln!("xmms-rs: failed to import skin: {err}"),
                }
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn build_skin_browser_content(add: &gtk::Button, close: &gtk::Button) -> (gtk::Box, gtk::ListBox) {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 5);
    root.add_css_class("xmms-skinned-window");
    root.set_widget_name(SKIN_BROWSER_ROOT_WIDGET);
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    root.set_margin_start(10);
    root.set_margin_end(10);

    let header = gtk::Label::new(Some("Skins"));
    header.set_widget_name(SKIN_BROWSER_HEADER_WIDGET);
    header.set_xalign(0.0);

    let list = gtk::ListBox::new();
    list.set_widget_name(SKIN_BROWSER_LIST_WIDGET);
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_vexpand(true);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Always);
    scrolled.set_min_content_width(250);
    scrolled.set_min_content_height(200);
    scrolled.set_vexpand(true);
    scrolled.set_child(Some(&list));

    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    buttons.set_halign(gtk::Align::End);
    buttons.append(add);
    buttons.append(close);

    root.append(&header);
    root.append(&scrolled);
    root.append(&separator);
    root.append(&buttons);
    (root, list)
}

fn user_skin_import_dir() -> PathBuf {
    default_config_dir().join("xmms").join("Skins")
}

fn runtime_skin_browser_dirs() -> Vec<PathBuf> {
    let home_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let system_skin_dir = std::env::var_os("XMMS_RS_SYSTEM_SKIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/share/xmms/Skins"));
    let skinsdir = std::env::var("SKINSDIR").ok();
    skin_browser_search_dirs(
        &default_config_dir(),
        &home_dir,
        &system_skin_dir,
        skinsdir.as_deref(),
    )
}

fn import_skin_to_user_dir(source: &Path, user_skin_dir: &Path) -> io::Result<PathBuf> {
    if !source.is_dir() && !source.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a skin file or directory: {}", source.display()),
        ));
    }
    if source.is_file() && !is_importable_skin_archive(source) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported skin archive: {}", source.display()),
        ));
    }

    fs::create_dir_all(user_skin_dir)?;
    let name = source.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("skin path has no file name: {}", source.display()),
        )
    })?;
    let destination = unique_import_destination(user_skin_dir, name);
    if source.is_dir() {
        copy_dir_recursive(source, &destination)?;
    } else {
        fs::copy(source, &destination)?;
    }
    Ok(destination)
}

fn is_importable_skin_archive(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    [
        ".zip", ".wsz", ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".gz", ".bz2",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
}

fn unique_import_destination(user_skin_dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let candidate = user_skin_dir.join(name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Skin");
    let extension = path.extension().and_then(|extension| extension.to_str());
    for index in 1.. {
        let file_name = match extension {
            Some(extension) => format!("{stem} {index}.{extension}"),
            None => format!("{stem} {index}"),
        };
        let candidate = user_skin_dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn sanitized_skin_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_string();
    if sanitized.is_empty() {
        "Edited Skin".to_string()
    } else {
        sanitized
    }
}

fn ensure_wsz_extension(mut path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wsz"))
    {
        return path;
    }
    path.set_extension("wsz");
    path
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_source = entry.path();
        let entry_destination = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry_source, &entry_destination)?;
        } else {
            fs::copy(entry_source, entry_destination)?;
        }
    }
    Ok(())
}

fn refresh_skin_browser_list<P: AsRef<Path>>(
    list: &gtk::ListBox,
    state: &mut MainWindowUiState,
    dirs: &[P],
) -> io::Result<()> {
    state.scan_skin_browser_dirs(dirs)?;
    populate_skin_browser_list(list, state);
    Ok(())
}

fn populate_skin_browser_list(list: &gtk::ListBox, state: &MainWindowUiState) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    append_skin_browser_row(list, "default");
    for entry in state.skin_browser_entries() {
        append_skin_browser_row(list, &entry.name);
    }

    if let Some(row) = list.row_at_index(state.selected_skin_index() as i32) {
        list.select_row(Some(&row));
    }
}

fn append_skin_browser_row(list: &gtk::ListBox, label: &str) {
    let row_label = gtk::Label::new(Some(label));
    row_label.set_xalign(0.0);
    row_label.set_margin_top(2);
    row_label.set_margin_bottom(2);
    row_label.set_margin_start(4);
    row_label.set_margin_end(4);
    list.append(&row_label);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum KeyboardFocus {
    #[default]
    Main,
    Equalizer,
    Playlist,
}

impl From<PanelKind> for KeyboardFocus {
    fn from(kind: PanelKind) -> Self {
        match kind {
            PanelKind::Equalizer => Self::Equalizer,
            PanelKind::Playlist => Self::Playlist,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PlaylistMenu {
    #[default]
    Closed,
    Open {
        kind: PlaylistMenuKind,
        hover: Option<usize>,
        pressed: bool,
    },
}

impl PlaylistMenu {
    fn kind(self) -> Option<PlaylistMenuKind> {
        match self {
            Self::Closed => None,
            Self::Open { kind, .. } => Some(kind),
        }
    }

    fn hover(self) -> Option<usize> {
        match self {
            Self::Closed => None,
            Self::Open { hover, .. } => hover,
        }
    }

    fn pressed(self) -> bool {
        match self {
            Self::Closed => false,
            Self::Open { pressed, .. } => pressed,
        }
    }

    fn is_open(self) -> bool {
        matches!(self, Self::Open { .. })
    }

    fn open(&mut self, kind: PlaylistMenuKind) {
        *self = Self::Open {
            kind,
            hover: Some(kind.item_count().saturating_sub(1)),
            pressed: false,
        };
    }

    fn close(&mut self) {
        *self = Self::Closed;
    }

    fn set_hover(&mut self, hover: Option<usize>) -> bool {
        let Self::Open { hover: current, .. } = self else {
            return false;
        };
        let changed = *current != hover;
        *current = hover;
        changed
    }

    fn press_item(&mut self, item: usize) -> bool {
        let Self::Open { hover, pressed, .. } = self else {
            return false;
        };
        *hover = Some(item);
        *pressed = true;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistContextAction {
    RemoveSelected,
    RemoveDead,
    PhysicallyDelete,
    SelectAll,
    SelectNone,
    InvertSelection,
}

fn panel_event_to_base_coords(
    kind: PanelKind,
    area: &gtk::DrawingArea,
    state: &MainWindowUiState,
    x: f64,
    y: f64,
) -> (i32, i32) {
    let (base_width, base_height) = match kind {
        PanelKind::Equalizer => (
            EQUALIZER_WINDOW_WIDTH,
            if state.is_equalizer_shaded() {
                MAIN_TITLEBAR_HEIGHT
            } else {
                EQUALIZER_WINDOW_HEIGHT
            },
        ),
        PanelKind::Playlist => (
            state.playlist_ui.width,
            if state.is_playlist_shaded() {
                MAIN_TITLEBAR_HEIGHT
            } else {
                state.playlist_ui.height
            },
        ),
    };
    let width = area.allocated_width().max(1) as f64;
    let height = area.allocated_height().max(1) as f64;
    scale_event_coords(width, height, base_width, base_height, x, y)
}

fn handle_panel_action_for_main_window(
    action: PanelAction,
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    panel_windows: &Rc<PanelWindows>,
    main_state: &Rc<RefCell<MainWindowUiState>>,
    playlist_sort_menu: &gtk::Popover,
    equalizer_presets_menu: &gtk::Popover,
) {
    apply_panel_action(
        action,
        GtkPanelActionContext {
            window,
            area,
            main_state,
            open_location_window: Some(&panel_windows.open_location),
            playlist_sort_menu: Some(playlist_sort_menu),
            equalizer_presets_menu: Some(equalizer_presets_menu),
            equalizer_presets_docked: true,
        },
        || {
            sync_panel_windows(panel_windows, &main_state.borrow());
            resize_main_window(window, area, &main_state.borrow());
        },
    );
}

fn sync_single_panel_window_from_state(
    kind: PanelKind,
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    state: &Rc<RefCell<MainWindowUiState>>,
) {
    let (visible, shaded, width, full_height, scale) = {
        let state = state.borrow();
        let (visible, shaded, width, full_height) = panel_window_values(kind, &state);
        (visible, shaded, width, full_height, state.scale_factor())
    };
    sync_single_panel_window_values(
        window,
        area,
        visible,
        shaded,
        width,
        full_height,
        scale,
        kind == PanelKind::Playlist && !shaded,
    );
}

fn panel_window_values(kind: PanelKind, state: &MainWindowUiState) -> (bool, bool, i32, i32) {
    match kind {
        PanelKind::Equalizer => (
            state.panel_state(kind).is_detached_visible(),
            state.panel_state(kind).shaded(),
            EQUALIZER_WINDOW_WIDTH,
            EQUALIZER_WINDOW_HEIGHT,
        ),
        PanelKind::Playlist => (
            state.panel_state(kind).is_detached_visible(),
            state.panel_state(kind).shaded(),
            state.playlist_ui.width,
            state.playlist_ui.height,
        ),
    }
}

fn sync_single_panel_window_values(
    window: &gtk::ApplicationWindow,
    area: &gtk::DrawingArea,
    visible: bool,
    shaded: bool,
    width: i32,
    full_height: i32,
    scale: f64,
    resizable: bool,
) {
    if !visible {
        window.hide();
        return;
    }
    let height = if shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        full_height
    };
    area.set_content_width(scale_dim(width, scale));
    area.set_content_height(scale_dim(height, scale));
    window.set_resizable(resizable);
    window.set_default_size(scale_dim(width, scale), scale_dim(height, scale));
    area.queue_resize();
    window.queue_resize();
    present_if_hidden(window);
    area.queue_draw();
}

fn present_if_hidden(window: &gtk::ApplicationWindow) {
    if !window.is_visible() {
        window.present();
    }
}

fn present_visible_panel_windows(windows: &PanelWindows, state: &MainWindowUiState) {
    let visibility = state.panel_visibility();
    if visibility.equalizer {
        windows.equalizer.present();
    }
    if visibility.playlist {
        windows.playlist.present();
    }
}

fn sync_panel_windows(windows: &PanelWindows, state: &MainWindowUiState) {
    let visibility = state.panel_visibility();
    let scale = state.scale_factor();
    if visibility.equalizer {
        let height = if state.is_equalizer_shaded() {
            MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        };
        windows
            .equalizer_area
            .set_content_width(scale_dim(EQUALIZER_WINDOW_WIDTH, scale));
        windows
            .equalizer_area
            .set_content_height(scale_dim(height, scale));
        windows.equalizer.set_resizable(false);
        windows.equalizer.set_default_size(
            scale_dim(EQUALIZER_WINDOW_WIDTH, scale),
            scale_dim(height, scale),
        );
        present_if_hidden(&windows.equalizer);
        windows.equalizer_area.queue_draw();
    } else {
        windows.equalizer.hide();
    }

    if visibility.playlist {
        let height = if state.is_playlist_shaded() {
            MAIN_TITLEBAR_HEIGHT
        } else {
            state.playlist_ui.height
        };
        windows
            .playlist_area
            .set_content_width(scale_dim(state.playlist_ui.width, scale));
        windows
            .playlist_area
            .set_content_height(scale_dim(height, scale));
        windows.playlist.set_resizable(!state.is_playlist_shaded());
        windows.playlist.set_default_size(
            scale_dim(state.playlist_ui.width, scale),
            scale_dim(height, scale),
        );
        windows.playlist_area.queue_resize();
        windows.playlist.queue_resize();
        present_if_hidden(&windows.playlist);
        windows.playlist_area.queue_draw();
    } else {
        windows.playlist.hide();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum PlaylistSearch {
    #[default]
    Inactive,
    Active {
        query: String,
    },
}

impl PlaylistSearch {
    fn is_active(&self) -> bool {
        matches!(self, Self::Active { .. })
    }

    fn query(&self) -> &str {
        match self {
            Self::Inactive => "",
            Self::Active { query } => query,
        }
    }

    fn active_query(&self) -> Option<&str> {
        match self {
            Self::Inactive => None,
            Self::Active { query } => Some(query),
        }
    }

    fn start(&mut self) {
        *self = Self::Active {
            query: String::new(),
        };
    }

    fn stop(&mut self) {
        *self = Self::Inactive;
    }

    fn push_char(&mut self, ch: char) -> bool {
        let Self::Active { query } = self else {
            return false;
        };
        if ch.is_control() {
            return false;
        }
        query.push(ch);
        true
    }

    fn pop_char(&mut self) -> bool {
        let Self::Active { query } = self else {
            return false;
        };
        query.pop();
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MainPointer {
    #[default]
    Idle,
    PressedButton {
        control: MainControl,
        inside: bool,
    },
    DraggingSlider {
        slider: MainSlider,
        offset: i32,
        position: i32,
    },
}

impl MainPointer {
    fn pressed_control(self) -> Option<MainControl> {
        match self {
            Self::PressedButton {
                control,
                inside: true,
            } => Some(control),
            _ => None,
        }
    }

    fn pressed_slider(self) -> Option<MainSlider> {
        match self {
            Self::DraggingSlider { slider, .. } => Some(slider),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EqualizerPointer {
    #[default]
    Idle,
    PressedControl {
        control: EqualizerControl,
        inside: bool,
    },
    DraggingSlider {
        slider: EqualizerSlider,
        offset: i32,
    },
}

impl EqualizerPointer {
    fn pressed_control(self) -> Option<EqualizerControl> {
        match self {
            Self::PressedControl {
                control,
                inside: true,
            } => Some(control),
            _ => None,
        }
    }

    fn dragging_slider(self) -> Option<EqualizerSlider> {
        match self {
            Self::DraggingSlider { slider, .. } => Some(slider),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PlaylistPointer {
    #[default]
    Idle,
    DraggingEntry {
        index: usize,
        moved: bool,
    },
    DraggingScrollbar {
        offset: i32,
    },
    Resizing {
        offset_y: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainControl {
    Push(MainPushButton),
    Toggle(MainToggleButton),
    Slider(MainSlider),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackControlEvent {
    Play,
    Pause,
    Halt,
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackTransitionState {
    Idle,
    StoppedAt(i64),
    PendingBackendSeek(i64),
    AwaitingBackendSeek(i64),
    WaitingBetweenSongs {
        remaining_ms: i64,
    },
    FadingOut {
        remaining_ms: i64,
        start_volume: i32,
    },
}

impl PlaybackTransitionState {
    fn stopped_at_or_idle(position_ms: i64) -> Self {
        if position_ms > 0 {
            Self::StoppedAt(position_ms)
        } else {
            Self::Idle
        }
    }

    #[allow(dead_code)]
    fn start_playback() -> Self {
        Self::Idle
    }

    fn stop_playback() -> Self {
        Self::Idle
    }

    fn request_backend_seek(position_ms: i64) -> Self {
        Self::PendingBackendSeek(position_ms)
    }

    fn await_backend_seek(position_ms: i64) -> Self {
        Self::AwaitingBackendSeek(position_ms)
    }

    fn start_fadeout(start_volume: i32) -> Self {
        Self::FadingOut {
            remaining_ms: STOP_FADE_DURATION_MS,
            start_volume,
        }
    }

    fn tick_fadeout(self, elapsed_ms: u32) -> Option<(Self, i32)> {
        let (remaining_ms, start_volume) = self.fadeout()?;
        let remaining_ms = (remaining_ms - i64::from(elapsed_ms)).max(0);
        let volume =
            ((i64::from(start_volume) * remaining_ms) / STOP_FADE_DURATION_MS).clamp(0, 100) as i32;
        Some((
            Self::FadingOut {
                remaining_ms,
                start_volume,
            },
            volume,
        ))
    }

    fn wait_between_songs(remaining_ms: i64) -> Self {
        Self::WaitingBetweenSongs { remaining_ms }
    }

    fn tick_eof_pause(self, elapsed_ms: u32) -> Option<(Self, bool)> {
        let remaining = self.eof_pause_remaining_ms()? - i64::from(elapsed_ms);
        if remaining > 0 {
            Some((
                Self::WaitingBetweenSongs {
                    remaining_ms: remaining,
                },
                false,
            ))
        } else {
            Some((Self::Idle, true))
        }
    }

    fn eof_pause_remaining_ms(self) -> Option<i64> {
        match self {
            PlaybackTransitionState::WaitingBetweenSongs { remaining_ms } => Some(remaining_ms),
            _ => None,
        }
    }

    fn pending_backend_seek_ms(self) -> Option<i64> {
        match self {
            PlaybackTransitionState::PendingBackendSeek(position_ms) => Some(position_ms),
            _ => None,
        }
    }

    fn awaiting_backend_seek_ms(self) -> Option<i64> {
        match self {
            PlaybackTransitionState::AwaitingBackendSeek(position_ms) => Some(position_ms),
            _ => None,
        }
    }

    fn fadeout(self) -> Option<(i64, i32)> {
        match self {
            PlaybackTransitionState::FadingOut {
                remaining_ms,
                start_volume,
            } => Some((remaining_ms, start_volume)),
            _ => None,
        }
    }

    #[allow(dead_code)]
    fn play_start_position_ms(self, fallback_ms: i64) -> i64 {
        match self {
            PlaybackTransitionState::StoppedAt(position_ms) => position_ms,
            PlaybackTransitionState::WaitingBetweenSongs { .. } => 0,
            PlaybackTransitionState::Idle
            | PlaybackTransitionState::PendingBackendSeek(_)
            | PlaybackTransitionState::AwaitingBackendSeek(_)
            | PlaybackTransitionState::FadingOut { .. } => fallback_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiAction {
    None,
    Quit,
    Minimize,
    Resize,
    ShowMenu,
    OpenFileDialog,
}

struct EqualizerUiState {
    panel: PanelUiState,
    pointer: EqualizerPointer,
    keyboard_slider: Option<EqualizerSlider>,
    preset_dir: PathBuf,
    presets: Vec<EqualizerPreset>,
    auto_presets: Vec<EqualizerPreset>,
}

impl EqualizerUiState {
    fn new() -> Self {
        Self {
            panel: PanelUiState::default(),
            pointer: EqualizerPointer::default(),
            keyboard_slider: None,
            preset_dir: default_config_dir().join("xmms-renascene"),
            presets: Vec::new(),
            auto_presets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PanelUiState {
    focused: bool,
    dragging_title: bool,
}

impl PanelUiState {
    fn focused(self) -> bool {
        self.focused || self.dragging_title
    }
}

struct PlaylistUiState {
    panel: PanelUiState,
    width: i32,
    height: i32,
    menu: PlaylistMenu,
    scroll_offset: usize,
    pointer: PlaylistPointer,
    last_click: Option<(usize, Instant)>,
    pending_double_click: Option<usize>,
    search: PlaylistSearch,
}

impl PlaylistUiState {
    fn new() -> Self {
        Self {
            panel: PanelUiState::default(),
            width: PLAYLIST_DEFAULT_WIDTH,
            height: PLAYLIST_DEFAULT_HEIGHT,
            menu: PlaylistMenu::default(),
            scroll_offset: 0,
            pointer: PlaylistPointer::default(),
            last_click: None,
            pending_double_click: None,
            search: PlaylistSearch::default(),
        }
    }
}

#[derive(Default)]
struct DialogVisibility {
    playlist_load: bool,
    playlist_save: bool,
    open_location: bool,
    jump_time: bool,
    skin_editor: bool,
    output_device_picker: bool,
    file: bool,
    directory: bool,
}

#[derive(Default)]
struct SkinBrowserState {
    entries: Vec<SkinEntry>,
    selected_index: usize,
    reload_count: u32,
}

pub(crate) struct MainWindowUiState {
    store: AppStore,
    playback_backend: Option<SharedPlaybackBackend>,
    duration_index_sender: Sender<DurationIndexResult>,
    duration_index_receiver: Receiver<DurationIndexResult>,
    last_playback_request: Option<String>,
    docked_focus: KeyboardFocus,
    equalizer: EqualizerUiState,
    playlist_ui: PlaylistUiState,
    dialogs: DialogVisibility,
    last_playlist_file_info: Option<String>,
    active_skin: DefaultSkin,
    skin_generation: u64,
    playlist_options_opened: bool,
    queue_manager_opened: bool,
    preferences_page: PreferencesPage,
    skin_browser: SkinBrowserState,
    skin_editor: SkinEditorState,
    output_device_groups: OutputDeviceGroups,
    output_switch_count: u32,
    mpris_events: Vec<MprisEvent>,
    playback_transition: PlaybackTransitionState,
    main_keyboard_slider: Option<MainSlider>,
    last_open_location: Option<String>,
    last_jump_time_ms: Option<i64>,
    visualization: Visualization,
    visualization_tick_counter: i32,
    playlist_footer_second: Option<i64>,
    main_pointer: MainPointer,
    title_marquee: TitleMarquee,
}

impl fmt::Debug for MainWindowUiState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MainWindowUiState")
            .field("store_revision", &self.store.revision())
            .field("store_state", self.store.state())
            .field("shaded", &self.store.state().config.main_shaded)
            .field(
                "playlist_shaded",
                &self.store.state().config.playlist_shaded,
            )
            .field(
                "preferences_visible",
                &self.store.state().ui.preferences_visible,
            )
            .field("preferences_page", &self.preferences_page)
            .field("player_state", &self.store.state().player.state())
            .finish_non_exhaustive()
    }
}

impl Default for MainWindowUiState {
    fn default() -> Self {
        Self::from_state(AppState::default())
    }
}

impl MainWindowUiState {
    pub(crate) fn from_state(app_state: AppState) -> Self {
        let (duration_index_sender, duration_index_receiver) = mpsc::channel();
        let active_skin = load_skin_from_config(&app_state.config).unwrap_or_else(|err| {
            eprintln!("xmms-rs: failed to load configured skin: {err}");
            DefaultSkin::load_bundled().expect("bundled default skin should load")
        });
        let playback_position_ms = app_state.config.playback_position_ms.max(0);
        let mut state = Self {
            store: AppStore::new(app_state),
            playback_backend: None,
            duration_index_sender,
            duration_index_receiver,
            last_playback_request: None,
            docked_focus: KeyboardFocus::default(),
            equalizer: EqualizerUiState::new(),
            playlist_ui: PlaylistUiState::new(),
            dialogs: DialogVisibility::default(),
            last_playlist_file_info: None,
            active_skin,
            skin_generation: 0,
            playlist_options_opened: false,
            queue_manager_opened: false,
            preferences_page: PreferencesPage::Options,
            skin_browser: SkinBrowserState::default(),
            skin_editor: SkinEditorState::default(),
            output_device_groups: OutputDeviceGroups::default(),
            output_switch_count: 0,
            mpris_events: Vec::new(),
            playback_transition: PlaybackTransitionState::stopped_at_or_idle(playback_position_ms),
            main_keyboard_slider: None,
            last_open_location: None,
            last_jump_time_ms: None,
            visualization: Visualization::new(WidgetId(6), 24, 43, 76),
            visualization_tick_counter: 0,
            playlist_footer_second: None,
            main_pointer: MainPointer::default(),
            title_marquee: TitleMarquee::default(),
        };
        state.apply_visualization_preferences();
        state
    }

    fn dispatch_store_command(&mut self, command: impl Into<AppCommand>) -> DispatchResult {
        self.store.dispatch(command.into())
    }

    fn dispatch_store_command_and_apply_local_effects(
        &mut self,
        command: impl Into<AppCommand>,
    ) -> DispatchResult {
        let mut result = self.dispatch_store_command(command);
        let effects = std::mem::take(&mut result.effects);
        for effect in effects {
            self.apply_store_effect(effect);
        }
        result
    }

    fn update_config_via_store(&mut self, update: impl FnOnce(&mut Config)) {
        let mut config = self.store.state().persistence_snapshot().config;
        update(&mut config);
        let effects = self.store.apply_config_from_preferences(config).effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
    }

    fn start_backend_playback_uri(&mut self, uri: &str, position_ms: i64) {
        self.load_equalizer_auto_preset_for_uri(uri);
        self.last_playback_request = Some(uri.to_string());
        self.playback_transition = if position_ms > 0 {
            PlaybackTransitionState::request_backend_seek(position_ms)
        } else {
            PlaybackTransitionState::Idle
        };
        let pending_seek = position_ms > 0;
        app_log_info!(backend, "gtk play_uri", uri, position_ms, pending_seek);
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().play_uri(uri) {
                eprintln!("xmms-rs: failed to play {uri}: {err}");
                self.playback_transition = PlaybackTransitionState::Idle;
            }
        }
    }

    fn apply_store_effect(&mut self, effect: AppEffect) -> GtkUiEffect {
        app_log_debug!(frontend_effect, "gtk {effect:?}");
        match effect {
            AppEffect::StartPlaybackUri { uri, position_ms } => {
                self.start_backend_playback_uri(&uri, position_ms);
            }
            AppEffect::ResumePlayback => self.unpause_playback(),
            AppEffect::PausePlayback => self.pause_playback(),
            AppEffect::StopPlayback => self.stop_playback(),
            AppEffect::BeginStopFade { start_volume } => {
                self.playback_transition = PlaybackTransitionState::start_fadeout(start_volume);
            }
            AppEffect::SeekPlayback(position_ms) => {
                self.playback_transition = PlaybackTransitionState::Idle;
                app_log_info!(backend, "gtk seek_to_ms", position_ms);
                if let Some(backend) = &self.playback_backend {
                    if let Err(err) = backend.borrow().seek(position_ms) {
                        eprintln!("xmms-rs: failed to seek playback: {err}");
                    }
                }
            }
            AppEffect::SetOutputVolume(volume) | AppEffect::SetBackendVolume(volume) => {
                if let Some(backend) = &self.playback_backend {
                    let _ = backend.borrow().set_volume(volume);
                }
            }
            AppEffect::SetBackendBalance(balance) => {
                if let Some(backend) = &self.playback_backend {
                    let _ = backend.borrow().set_balance(balance);
                }
            }
            AppEffect::SetBackendEqualizer => self.sync_equalizer_to_backend(),
            AppEffect::OpenPreferences => {
                self.dispatch_store_command_and_apply_local_effects(
                    UiCommand::SetPreferencesVisible(true),
                );
                return GtkUiEffect::OpenPreferences;
            }
            AppEffect::OpenSkinBrowser => {
                self.dispatch_store_command_and_apply_local_effects(
                    UiCommand::SetSkinBrowserVisible(true),
                );
                return GtkUiEffect::OpenSkinBrowser;
            }
            AppEffect::OpenFileInfoDialog => {
                self.dispatch_store_command_and_apply_local_effects(UiCommand::SetFileInfoVisible(
                    true,
                ));
            }
            AppEffect::SaveConfig
            | AppEffect::QueueRender(_)
            | AppEffect::OpenFileDialog(_)
            | AppEffect::OpenPath(_)
            | AppEffect::OpenSkinEditor
            | AppEffect::ShowError(_)
            | AppEffect::ShowMessage(_)
            | AppEffect::StartPlayback
            | AppEffect::StartPlaybackFromCurrent => {}
        }
        GtkUiEffect::None
    }

    pub(crate) fn active_skin(&self) -> &DefaultSkin {
        &self.active_skin
    }

    pub(crate) fn active_skin_mut(&mut self) -> &mut DefaultSkin {
        &mut self.active_skin
    }

    pub(crate) fn skin_editor(&self) -> &SkinEditorState {
        &self.skin_editor
    }

    pub(crate) fn skin_editor_mut(&mut self) -> &mut SkinEditorState {
        &mut self.skin_editor
    }

    fn load_configured_skin(&mut self) -> io::Result<()> {
        self.active_skin = load_skin_from_config(&self.store.state().config)?;
        self.skin_generation = self.skin_generation.wrapping_add(1);
        Ok(())
    }

    fn set_equalizer_preset_dir(&mut self, dir: PathBuf) {
        self.equalizer.preset_dir = dir;
        if let Err(err) = self.load_equalizer_preset_stores() {
            eprintln!("xmms-rs: failed to load equalizer presets: {err}");
        }
    }

    fn load_equalizer_preset_stores(&mut self) -> io::Result<()> {
        self.equalizer.presets =
            load_preset_store(&preset_store_path(&self.equalizer.preset_dir, "eq.preset"))?;
        if self.equalizer.presets.is_empty() {
            self.equalizer.presets = default_equalizer_presets();
        }
        self.equalizer.auto_presets = load_preset_store(&preset_store_path(
            &self.equalizer.preset_dir,
            "eq.auto_preset",
        ))?;
        Ok(())
    }

    fn current_equalizer_preset(&self, name: impl Into<String>) -> EqualizerPreset {
        let config = &self.store.state().config;
        EqualizerPreset::from_positions(
            name,
            config.equalizer_preamp_pos,
            config.equalizer_band_pos,
        )
    }

    fn apply_equalizer_preset_values(&mut self, preset: &EqualizerPreset) {
        let effects = self
            .store
            .apply_equalizer_preset_positions(preset.preamp_position(), preset.band_positions())
            .effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
    }

    fn load_named_equalizer_preset(&mut self, name: &str, automatic: bool) -> bool {
        let preset = if automatic {
            find_preset(&self.equalizer.auto_presets, name)
        } else {
            find_preset(&self.equalizer.presets, name)
        }
        .cloned();
        if let Some(preset) = preset {
            self.apply_equalizer_preset_values(&preset);
            true
        } else {
            false
        }
    }

    fn load_equalizer_default_preset(&mut self) {
        self.load_named_equalizer_preset("Default", false);
    }

    fn load_equalizer_winamp_file(&mut self, path: &Path) -> io::Result<()> {
        if let Some(preset) = load_winamp_eqf_first(path)? {
            self.apply_equalizer_preset_values(&preset);
        }
        Ok(())
    }

    fn save_equalizer_winamp_file(&self, path: &Path) -> io::Result<()> {
        let path = if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("eqf"))
        {
            path.to_path_buf()
        } else {
            path.with_extension("eqf")
        };
        save_winamp_eqf(&path, &self.current_equalizer_preset("Entry1"))
    }

    pub(crate) fn scale_factor(&self) -> f64 {
        self.store.state().config.scale_factor
    }

    fn save_runtime_snapshot(&self, config_path: &Path, playlist_path: &Path) -> io::Result<()> {
        save_fallback_state(self.store.state(), config_path, playlist_path)
    }

    pub(crate) fn set_playback_backend(&mut self, backend: SharedPlaybackBackend) {
        self.output_device_groups = backend.borrow().output_device_groups();
        {
            let mut backend = backend.borrow_mut();
            let selection = self
                .store
                .state()
                .config
                .output_device
                .as_deref()
                .map(OutputDeviceSelection::System)
                .unwrap_or(OutputDeviceSelection::Automatic);
            if let Err(err) = backend.select_output_device(selection) {
                eprintln!("xmms-rs: failed to apply saved output device: {err}");
            }
            let state = self.store.state();
            let player = &state.player;
            let config = &state.config;
            let _ = backend.set_volume(player.volume());
            let _ = backend.set_balance(player.balance());
            let _ = backend.set_equalizer(EqualizerBackendState {
                active: config.equalizer_active,
                preamp_position: config.equalizer_preamp_pos,
                band_positions: config.equalizer_band_pos,
            });
            self.output_device_groups = backend.output_device_groups();
        }
        self.playback_backend = Some(backend);
    }

    fn render_state(&self) -> MainWindowRenderState {
        let state = self.store.state();
        MainWindowRenderState {
            focused: self.main_focused(),
            title: self
                .equalizer_drag_info_text()
                .unwrap_or_else(|| self.formatted_current_title()),
            title_offset_px: self.title_marquee.offset_px(),
            shaded: state.config.main_shaded,
            volume_position: volume_to_position(state.player.volume()),
            balance_position: balance_to_position(state.player.balance()),
            position_position: self.position_slider_position(),
            shaded_position_position: self.shaded_position_slider_position(),
            shaded_position_visible: self.shaded_position_slider_visible(),
            time_digits: self.time_digits(),
            shaded_time_min: self.shaded_time_min_text(),
            shaded_time_sec: self.shaded_time_sec_text(),
            bitrate_text: self.bitrate_text(),
            frequency_text: self.frequency_text(),
            shuffle_selected: state.playlist.shuffle(),
            repeat_selected: state.playlist.repeat(),
            equalizer_selected: state.config.equalizer_visible,
            playlist_selected: state.config.playlist_visible,
            pressed_push: self.pressed_push(),
            pressed_toggle: self.pressed_toggle(),
            pressed_slider: self.pressed_slider(),
            play_status: match state.player.state() {
                PlayerState::Stopped => PlayStatusValue::Stopped,
                PlayerState::Paused => PlayStatusValue::Paused,
                PlayerState::Playing => PlayStatusValue::Playing,
            },
            channels: state.player.channels(),
            visualization: self.make_visualization_render_state(),
        }
    }

    fn playlist_rows_render_state(&self) -> PlaylistRowsRenderState {
        shared_playlist_rows_render_state(
            self.store.state(),
            self.playlist_ui.scroll_offset,
            matches!(
                self.playlist_ui.pointer,
                PlaylistPointer::DraggingScrollbar { .. }
            ),
            self.playlist_ui.search.active_query().map(str::to_owned),
            self.playlist_ui.width,
            self.playlist_ui.height,
        )
    }

    fn bitrate_text(&self) -> String {
        let bitrate = self.store.state().player.bitrate();
        if bitrate <= 0 {
            return "   ".to_string();
        }
        if bitrate < 1000 {
            format!("{bitrate:>3}")
        } else {
            format!("{:>2}H", bitrate / 100)
        }
    }

    fn frequency_text(&self) -> String {
        let frequency = self.store.state().player.frequency();
        if frequency <= 0 {
            return "  ".to_string();
        }
        let khz = if frequency >= 1000 {
            (frequency + 500) / 1000
        } else {
            frequency
        };
        format!("{khz:>2}")
    }

    pub(crate) fn formatted_current_title(&self) -> String {
        shared_formatted_current_title(self.store.state())
    }

    fn equalizer_drag_info_text(&self) -> Option<String> {
        let slider = self.equalizer.pointer.dragging_slider()?;
        let config = &self.store.state().config;
        let (label, position) = match slider {
            EqualizerSlider::Preamp => ("PREAMP", config.equalizer_preamp_pos),
            EqualizerSlider::Band(0) => ("60HZ", config.equalizer_band_pos[0]),
            EqualizerSlider::Band(1) => ("170HZ", config.equalizer_band_pos[1]),
            EqualizerSlider::Band(2) => ("310HZ", config.equalizer_band_pos[2]),
            EqualizerSlider::Band(3) => ("600HZ", config.equalizer_band_pos[3]),
            EqualizerSlider::Band(4) => ("1KHZ", config.equalizer_band_pos[4]),
            EqualizerSlider::Band(5) => ("3KHZ", config.equalizer_band_pos[5]),
            EqualizerSlider::Band(6) => ("6KHZ", config.equalizer_band_pos[6]),
            EqualizerSlider::Band(7) => ("12KHZ", config.equalizer_band_pos[7]),
            EqualizerSlider::Band(8) => ("14KHZ", config.equalizer_band_pos[8]),
            EqualizerSlider::Band(9) => ("16KHZ", config.equalizer_band_pos[9]),
            EqualizerSlider::Band(_)
            | EqualizerSlider::ShadedVolume
            | EqualizerSlider::ShadedBalance => return None,
        };
        Some(format!(
            "EQ: {label}: {:+.1} DB",
            equalizer_position_to_db(position)
        ))
    }

    fn formatted_playlist_entry_title(&self, entry: &crate::playlist::PlaylistEntry) -> String {
        shared_formatted_playlist_entry_title(self.store.state(), entry)
    }

    pub(crate) fn shaded_playlist_info(&self) -> String {
        let state = self.store.state();
        let Some(position) = state.playlist.position() else {
            return String::new();
        };
        let Some(entry) = state.playlist.entries().get(position) else {
            return String::new();
        };

        let title = self.formatted_playlist_entry_title(entry);
        let prefix = if state.config.show_numbers_in_pl {
            format!("{}. ", position + 1)
        } else {
            String::new()
        };
        let suffix = if entry.length_ms >= 0 {
            format!(" {}", format_duration(entry.length_ms))
        } else {
            String::new()
        };
        let max_len = ((self.playlist_ui.width - 35) / 5)
            .saturating_sub(prefix.len() as i32)
            .saturating_sub(suffix.len() as i32)
            .max(0) as usize;
        let title = ellipsize_chars(&title, max_len);
        format!("{prefix}{title:<max_len$}{suffix}")
    }

    pub(crate) fn playlist_footer_info(&self) -> String {
        shared_playlist_footer_info(self.store.state())
    }

    fn playlist_footer_time_parts(&self) -> (String, String) {
        if self.store.state().player.state() == PlayerState::Stopped {
            return ("   ".to_string(), "  ".to_string());
        }
        let display_ms = self.display_time_ms();
        let mut seconds = (display_ms / 1000).max(0);
        if seconds > i64::from(99 * 60) {
            seconds /= 60;
        }
        let prefix = if self.store.state().config.timer_mode == TimerMode::Remaining
            && self
                .current_duration_ms()
                .is_some_and(|duration| duration > 0)
        {
            '-'
        } else {
            ' '
        };
        (
            format!("{prefix}{:02}", seconds / 60),
            format!("{:02}", seconds % 60),
        )
    }

    fn playlist_footer_time_min_text(&self) -> String {
        self.playlist_footer_time_parts().0
    }

    fn playlist_footer_time_sec_text(&self) -> String {
        self.playlist_footer_time_parts().1
    }

    fn current_duration_ms(&self) -> Option<i64> {
        self.store.state().player.duration_ms().or_else(|| {
            self.store
                .state()
                .playlist
                .position()
                .and_then(|position| self.playlist_entry_length_ms(position))
                .filter(|duration| *duration > 0)
        })
    }

    fn ensure_current_playlist_position_for_seek(&mut self) {
        if self.store.state().playlist.position().is_none()
            && !self.store.state().playlist.is_empty()
        {
            self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::SetPosition(0));
        }
    }

    fn position_slider_position(&self) -> i32 {
        let Some(duration_ms) = self.current_duration_ms().filter(|duration| *duration > 0) else {
            return 0;
        };
        let position_slider = main_slider_layout(MainSlider::Position, false);
        ((self
            .store
            .state()
            .config
            .playback_position_ms
            .clamp(0, duration_ms)
            * i64::from(position_slider.max))
            / duration_ms) as i32
    }

    fn shaded_position_slider_visible(&self) -> bool {
        self.store.state().player.state() != PlayerState::Stopped
            && self
                .current_duration_ms()
                .is_some_and(|duration| duration > 0)
    }

    fn shaded_position_slider_position(&self) -> i32 {
        let Some(duration_ms) = self.current_duration_ms().filter(|duration| *duration > 0) else {
            return 1;
        };
        (((self
            .store
            .state()
            .config
            .playback_position_ms
            .clamp(0, duration_ms)
            * 12)
            / duration_ms) as i32
            + 1)
        .clamp(1, 13)
    }

    fn display_time_ms(&self) -> i64 {
        if let Some(remaining) = self.playback_transition.eof_pause_remaining_ms() {
            return remaining.max(0);
        }
        let elapsed = self.store.state().config.playback_position_ms.max(0);
        if self.store.state().config.timer_mode == TimerMode::Remaining {
            if let Some(duration) = self.current_duration_ms().filter(|duration| *duration > 0) {
                return (duration - elapsed).max(0);
            }
        }
        elapsed
    }

    fn time_digits(&self) -> [i32; 5] {
        if self.store.state().player.state() == PlayerState::Stopped
            && self.playback_transition.eof_pause_remaining_ms().is_none()
        {
            return [NumberDisplay::BLANK; 5];
        }
        let display_ms = self.display_time_ms();
        let mut seconds = (display_ms / 1000).max(0);
        if seconds > i64::from(99 * 60) {
            seconds /= 60;
        }
        let minutes = seconds / 60;
        [
            if self.store.state().config.timer_mode == TimerMode::Remaining
                && self
                    .current_duration_ms()
                    .is_some_and(|duration| duration > 0)
            {
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

    fn shaded_time_parts(&self) -> (String, String) {
        if self.store.state().player.state() == PlayerState::Stopped {
            return ("   ".to_string(), "  ".to_string());
        }
        let display_ms = self.display_time_ms();
        let mut seconds = (display_ms / 1000).max(0);
        if seconds > i64::from(99 * 60) {
            seconds /= 60;
        }
        let prefix = if self.store.state().config.timer_mode == TimerMode::Remaining
            && self
                .current_duration_ms()
                .is_some_and(|duration| duration > 0)
        {
            '-'
        } else {
            ' '
        };
        (
            format!("{prefix}{:02}", seconds / 60),
            format!("{:02}", seconds % 60),
        )
    }

    fn shaded_time_min_text(&self) -> String {
        self.shaded_time_parts().0
    }

    fn shaded_time_sec_text(&self) -> String {
        self.shaded_time_parts().1
    }

    fn make_visualization_render_state(&self) -> VisualizationRenderState {
        VisualizationRenderState {
            mode: self.visualization.mode(),
            analyzer_style: self.visualization.analyzer_style(),
            analyzer_mode: self.visualization.analyzer_mode(),
            scope_mode: self.visualization.scope_mode(),
            peaks_enabled: self.visualization.peaks_enabled(),
            vu_mode: self.store.state().config.vis_vu_mode,
            data: *self.visualization.data(),
            peak: *self.visualization.peak(),
            milkdrop_energy: self.visualization.milkdrop_energy(),
            milkdrop_phase: self.visualization.milkdrop_phase(),
        }
    }

    fn main_focused(&self) -> bool {
        self.selected_docked_panel().is_none()
    }

    fn equalizer_focused(&self) -> bool {
        if self.is_panel_detached(PanelKind::Equalizer) {
            self.equalizer.panel.focused()
        } else {
            self.docked_focus == KeyboardFocus::Equalizer || self.equalizer.panel.dragging_title
        }
    }

    fn playlist_focused(&self) -> bool {
        if self.is_panel_detached(PanelKind::Playlist) {
            self.playlist_ui.panel.focused()
        } else {
            self.docked_focus == KeyboardFocus::Playlist || self.playlist_ui.panel.dragging_title
        }
    }

    fn select_focus_target(&mut self, target: KeyboardFocus) {
        self.docked_focus = target;
        self.equalizer.panel.focused = target == KeyboardFocus::Equalizer;
        self.playlist_ui.panel.focused = target == KeyboardFocus::Playlist;
    }

    pub(crate) fn select_docked_main(&mut self) {
        self.select_focus_target(KeyboardFocus::Main);
    }

    pub(crate) fn select_docked_panel(&mut self, kind: PanelKind) {
        if self.panel_state(kind).is_docked_visible() {
            self.select_focus_target(kind.into());
        }
    }

    pub(crate) fn cycle_visible_focus(&mut self) {
        let mut targets = vec![KeyboardFocus::Main];
        if self.panel_state(PanelKind::Equalizer) != PanelState::Hidden {
            targets.push(KeyboardFocus::Equalizer);
        }
        if self.panel_state(PanelKind::Playlist) != PanelState::Hidden {
            targets.push(KeyboardFocus::Playlist);
        }
        let current = if self.equalizer_focused() {
            KeyboardFocus::Equalizer
        } else if self.playlist_focused() {
            KeyboardFocus::Playlist
        } else {
            KeyboardFocus::Main
        };
        let position = targets
            .iter()
            .position(|target| *target == current)
            .unwrap_or(0);
        let next = targets[(position + 1) % targets.len()];
        self.select_focus_target(next);
    }

    fn current_keyboard_focus(&self) -> KeyboardFocus {
        match self.selected_docked_panel() {
            Some(PanelKind::Playlist) => KeyboardFocus::Playlist,
            Some(PanelKind::Equalizer) => KeyboardFocus::Equalizer,
            None => KeyboardFocus::Main,
        }
    }

    fn arrow_key_command(&self, focus: KeyboardFocus, arrow: ArrowKey) -> KeyCommand {
        match (focus, arrow) {
            (KeyboardFocus::Main, ArrowKey::Up) if self.is_shaded() => KeyCommand::NextTrack,
            (KeyboardFocus::Main, ArrowKey::Down) if self.is_shaded() => KeyCommand::PreviousTrack,
            (KeyboardFocus::Main, ArrowKey::Up) => KeyCommand::Volume(4),
            (KeyboardFocus::Main, ArrowKey::Down) => KeyCommand::Volume(-4),
            (KeyboardFocus::Main, ArrowKey::Left)
                if self.main_keyboard_slider == Some(MainSlider::Balance) =>
            {
                KeyCommand::Balance(-4)
            }
            (KeyboardFocus::Main, ArrowKey::Right)
                if self.main_keyboard_slider == Some(MainSlider::Balance) =>
            {
                KeyCommand::Balance(4)
            }
            (KeyboardFocus::Main, ArrowKey::Left) => KeyCommand::Seek(-4),
            (KeyboardFocus::Main, ArrowKey::Right) => KeyCommand::Seek(4),
            (KeyboardFocus::Playlist, ArrowKey::Up) => KeyCommand::PlaylistMove(-1),
            (KeyboardFocus::Playlist, ArrowKey::Down) => KeyCommand::PlaylistMove(1),
            (KeyboardFocus::Playlist, ArrowKey::Left) => KeyCommand::Seek(-4),
            (KeyboardFocus::Playlist, ArrowKey::Right) => KeyCommand::Seek(4),
            (KeyboardFocus::Equalizer, ArrowKey::Up) => KeyCommand::EqualizerAdjust(-4),
            (KeyboardFocus::Equalizer, ArrowKey::Down) => KeyCommand::EqualizerAdjust(4),
            (KeyboardFocus::Equalizer, ArrowKey::Left) if self.is_equalizer_shaded() => {
                KeyCommand::Volume(-4)
            }
            (KeyboardFocus::Equalizer, ArrowKey::Right) if self.is_equalizer_shaded() => {
                KeyCommand::Volume(4)
            }
            (KeyboardFocus::Equalizer, ArrowKey::Left)
                if self.equalizer.keyboard_slider == Some(EqualizerSlider::ShadedBalance) =>
            {
                KeyCommand::Balance(-4)
            }
            (KeyboardFocus::Equalizer, ArrowKey::Right)
                if self.equalizer.keyboard_slider == Some(EqualizerSlider::ShadedBalance) =>
            {
                KeyCommand::Balance(4)
            }
            (KeyboardFocus::Equalizer, ArrowKey::Left) => KeyCommand::Seek(-4),
            (KeyboardFocus::Equalizer, ArrowKey::Right) => KeyCommand::Seek(4),
        }
    }

    fn apply_key_command(&mut self, command: KeyCommand) -> bool {
        match command {
            KeyCommand::Volume(diff) => self.adjust_volume_by(diff),
            KeyCommand::Balance(diff) => self.adjust_balance_by(diff),
            KeyCommand::Seek(diff) => self.adjust_main_seek(diff),
            KeyCommand::PreviousTrack => {
                self.activate_push(MainPushButton::Previous);
                true
            }
            KeyCommand::NextTrack => {
                self.activate_push(MainPushButton::Next);
                true
            }
            KeyCommand::PlaylistMove(delta) => self.move_playlist_arrow_selection(delta),
            KeyCommand::EqualizerAdjust(diff) => self.adjust_selected_equalizer_slider(diff),
        }
    }

    pub(crate) fn handle_docked_vertical_arrow(&mut self, delta: isize) -> bool {
        let arrow = if delta < 0 {
            ArrowKey::Up
        } else {
            ArrowKey::Down
        };
        self.apply_key_command(self.arrow_key_command(self.current_keyboard_focus(), arrow))
    }

    pub(crate) fn handle_docked_horizontal_arrow(&mut self, diff: i32) -> bool {
        let arrow = if diff < 0 {
            ArrowKey::Left
        } else {
            ArrowKey::Right
        };
        self.apply_key_command(self.arrow_key_command(self.current_keyboard_focus(), arrow))
    }

    pub(crate) fn docked_focus_is_main(&self) -> bool {
        self.main_focused()
    }

    pub(crate) fn docked_focus_is_panel(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Equalizer => self.equalizer_focused(),
            PanelKind::Playlist => self.playlist_focused(),
        }
    }

    fn selected_docked_panel(&self) -> Option<PanelKind> {
        match self.docked_focus {
            KeyboardFocus::Main => None,
            KeyboardFocus::Equalizer => self
                .panel_state(PanelKind::Equalizer)
                .is_docked_visible()
                .then_some(PanelKind::Equalizer),
            KeyboardFocus::Playlist => self
                .panel_state(PanelKind::Playlist)
                .is_docked_visible()
                .then_some(PanelKind::Playlist),
        }
    }

    fn equalizer_render_state(&self) -> EqualizerRenderState {
        let state = self.store.state();
        let config = &state.config;
        EqualizerRenderState {
            focused: self.equalizer_focused(),
            shaded: config.equalizer_shaded,
            active: config.equalizer_active,
            automatic: config.equalizer_auto,
            pressed_control: self.equalizer.pointer.pressed_control(),
            pressed_slider: self.equalizer.pointer.dragging_slider(),
            preamp_position: config.equalizer_preamp_pos,
            band_positions: config.equalizer_band_pos,
            volume_position: volume_to_eq_shaded_position(state.player.volume()),
            balance_position: balance_to_eq_shaded_position(state.player.balance()),
        }
    }

    fn panel_state(&self, kind: PanelKind) -> PanelState {
        let config = &self.store.state().config;
        let (visible, detached, shaded) = match kind {
            PanelKind::Equalizer => (
                config.equalizer_visible,
                config.equalizer_detached,
                config.equalizer_shaded,
            ),
            PanelKind::Playlist => (
                config.playlist_visible,
                config.playlist_detached,
                config.playlist_shaded,
            ),
        };
        PanelState::from_flags(visible, detached, shaded)
    }

    fn panel_ui_mut(&mut self, kind: PanelKind) -> &mut PanelUiState {
        match kind {
            PanelKind::Equalizer => &mut self.equalizer.panel,
            PanelKind::Playlist => &mut self.playlist_ui.panel,
        }
    }

    pub(crate) fn panel_visibility(&self) -> PanelVisibility {
        PanelVisibility {
            equalizer: self.panel_state(PanelKind::Equalizer).is_detached_visible(),
            playlist: self.panel_state(PanelKind::Playlist).is_detached_visible(),
        }
    }

    pub(crate) fn docked_panel_state(&self) -> DockedPanelState {
        let config = &self.store.state().config;
        DockedPanelState {
            main_focused: self.main_focused(),
            main_shaded: config.main_shaded,
            equalizer_visible: config.equalizer_visible,
            equalizer_detached: config.equalizer_detached,
            equalizer_focused: self.equalizer_focused(),
            equalizer_shaded: config.equalizer_shaded,
            playlist_visible: config.playlist_visible,
            playlist_detached: config.playlist_detached,
            playlist_focused: self.playlist_focused(),
            playlist_shaded: config.playlist_shaded,
            playlist_width: self.playlist_ui.width,
            playlist_height: self.playlist_ui.height,
        }
    }

    fn playlist_render_key(&self) -> GtkPlaylistRenderKey {
        let state = self.store.state();
        let mut title_preferences = DefaultHasher::new();
        state.config.title_format.hash(&mut title_preferences);
        state.config.convert_underscore.hash(&mut title_preferences);
        state.config.convert_twenty.hash(&mut title_preferences);

        let mut font = DefaultHasher::new();
        state.config.playlist_font.hash(&mut font);

        let mut search = DefaultHasher::new();
        self.playlist_ui.search.active_query().hash(&mut search);

        GtkPlaylistRenderKey {
            skin_generation: self.skin_generation,
            focused: self.playlist_focused(),
            shaded: self.is_playlist_shaded(),
            width: self.playlist_ui.width,
            height: self.playlist_ui.height,
            scroll_offset: self.playlist_ui.scroll_offset,
            revisions: state.playlist.revisions(),
            title_preferences_hash: title_preferences.finish(),
            font_hash: font.finish(),
            show_numbers: state.config.show_numbers_in_pl,
            playback_time_second: self.playlist_footer_display_second(),
            scrollbar_dragging: matches!(
                self.playlist_ui.pointer,
                PlaylistPointer::DraggingScrollbar { .. }
            ),
            search_hash: search.finish(),
            menu: self.playlist_menu().map(PlaylistMenuKind::render_kind),
            menu_hover: self.playlist_menu_hover(),
        }
    }

    fn playlist_footer_display_second(&self) -> Option<i64> {
        (self.store.state().player.state() != PlayerState::Stopped)
            .then(|| (self.display_time_ms() / 1_000).max(0))
    }

    fn update_playlist_footer_redraw(&mut self, redraw: &mut GtkTickRedraw) {
        let footer_second = self.playlist_footer_display_second();
        if footer_second != self.playlist_footer_second {
            self.playlist_footer_second = footer_second;
            redraw.playlist = true;
        }
    }

    fn docked_render_key(&self) -> GtkDockedRenderKey {
        GtkDockedRenderKey {
            skin_generation: self.skin_generation,
            main: self.render_state(),
            equalizer: self
                .panel_state(PanelKind::Equalizer)
                .is_docked_visible()
                .then(|| self.equalizer_render_state()),
            playlist: self
                .panel_state(PanelKind::Playlist)
                .is_docked_visible()
                .then(|| self.playlist_render_key()),
        }
    }

    pub(crate) fn docked_panel_size(&self) -> (i32, i32) {
        docked_panel_size(self.docked_panel_state())
    }

    pub(crate) fn docked_panel_at(&self, x: i32, y: i32) -> Option<(PanelKind, i32, i32)> {
        let mut offset_y = main_window_height(self.is_shaded());
        if self.panel_state(PanelKind::Equalizer).is_docked_visible() {
            let height = equalizer_window_height(self.is_equalizer_shaded());
            if (0..EQUALIZER_WINDOW_WIDTH).contains(&x) && y >= offset_y && y < offset_y + height {
                return Some((PanelKind::Equalizer, x, y - offset_y));
            }
            offset_y += height;
        }

        if self.panel_state(PanelKind::Playlist).is_docked_visible() {
            let height = playlist_window_height(self.is_playlist_shaded(), self.playlist_ui.height);
            if x >= 0 && x < self.playlist_ui.width && y >= offset_y && y < offset_y + height {
                return Some((PanelKind::Playlist, x, y - offset_y));
            }
        }

        None
    }

    fn docked_playlist_local_y(&self, y: i32) -> Option<i32> {
        if !self.panel_state(PanelKind::Playlist).is_docked_visible() {
            return None;
        }
        let mut offset_y = main_window_height(self.is_shaded());
        if self.panel_state(PanelKind::Equalizer).is_docked_visible() {
            offset_y += equalizer_window_height(self.is_equalizer_shaded());
        }
        Some(y - offset_y)
    }

    pub(crate) fn set_panel_detached(&mut self, kind: PanelKind, detached: bool) {
        match kind {
            PanelKind::Equalizer => {
                self.dispatch_store_command_and_apply_local_effects(
                    PanelCommand::SetEqualizerDetached(detached),
                );
            }
            PanelKind::Playlist => {
                self.dispatch_store_command_and_apply_local_effects(
                    PanelCommand::SetPlaylistDetached(detached),
                );
            }
        }
    }

    pub(crate) fn is_panel_detached(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Equalizer => self.store.state().config.equalizer_detached,
            PanelKind::Playlist => self.store.state().config.playlist_detached,
        }
    }

    pub(crate) fn is_panel_visible(&self, kind: PanelKind) -> bool {
        self.panel_state(kind) != PanelState::Hidden
    }

    pub(crate) fn is_shaded(&self) -> bool {
        self.store.state().config.main_shaded
    }

    pub(crate) fn is_menu_visible(&self) -> bool {
        self.store.state().ui.main_menu_visible
    }

    pub(crate) fn set_menu_visible(&mut self, visible: bool) {
        self.dispatch_store_command_and_apply_local_effects(UiCommand::SetMainMenuVisible(visible));
    }

    pub(crate) fn is_equalizer_shaded(&self) -> bool {
        self.store.state().config.equalizer_shaded
    }

    pub(crate) fn is_playlist_shaded(&self) -> bool {
        self.store.state().config.playlist_shaded
    }

    pub(crate) fn playlist_menu(&self) -> Option<PlaylistMenuKind> {
        self.playlist_ui.menu.kind()
    }

    pub(crate) fn playlist_menu_hover(&self) -> Option<usize> {
        self.playlist_ui.menu.hover()
    }

    pub(crate) fn playlist_menu_pressed(&self) -> bool {
        self.playlist_ui.menu.pressed()
    }

    pub(crate) fn playlist_size(&self) -> (i32, i32) {
        (self.playlist_ui.width, self.playlist_ui.height)
    }

    pub(crate) fn playlist_scroll_offset(&self) -> usize {
        self.playlist_ui.scroll_offset
    }

    pub(crate) fn playlist_scrollbar_visible(&self) -> bool {
        self.playlist_scrollbar_geometry().is_some()
    }

    pub(crate) fn playlist_search_active(&self) -> bool {
        self.playlist_ui.search.is_active()
    }

    pub(crate) fn playlist_search_query(&self) -> &str {
        self.playlist_ui.search.query()
    }

    pub(crate) fn set_playlist_visible(&mut self, visible: bool) {
        self.dispatch_store_command_and_apply_local_effects(PanelCommand::SetPlaylistVisibility(
            visible,
        ));
    }

    pub(crate) fn is_preferences_visible(&self) -> bool {
        self.store.state().ui.preferences_visible
    }

    pub(crate) fn set_preferences_visible(&mut self, visible: bool) {
        self.dispatch_store_command_and_apply_local_effects(UiCommand::SetPreferencesVisible(
            visible,
        ));
    }

    pub(crate) fn preferences_page(&self) -> PreferencesPage {
        self.preferences_page
    }

    pub(crate) fn set_preferences_page(&mut self, page: PreferencesPage) {
        self.preferences_page = page;
    }

    pub(crate) fn reset_preferences_to_defaults(&mut self) {
        let effects = self
            .store
            .apply_config_from_preferences(Config::default())
            .effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
        self.playback_transition = PlaybackTransitionState::stopped_at_or_idle(
            self.store.state().config.playback_position_ms,
        );
        self.apply_visualization_preferences();
    }

    pub(crate) fn is_open_location_visible(&self) -> bool {
        self.dialogs.open_location
    }

    pub(crate) fn set_open_location_visible(&mut self, visible: bool) {
        self.dialogs.open_location = visible;
    }

    pub(crate) fn is_jump_time_visible(&self) -> bool {
        self.dialogs.jump_time
    }

    pub(crate) fn set_jump_time_visible(&mut self, visible: bool) {
        self.dialogs.jump_time = visible;
    }

    pub(crate) fn is_skin_browser_visible(&self) -> bool {
        self.store.state().ui.skin_browser_visible
    }

    pub(crate) fn set_skin_browser_visible(&mut self, visible: bool) {
        self.dispatch_store_command_and_apply_local_effects(UiCommand::SetSkinBrowserVisible(
            visible,
        ));
    }

    pub(crate) fn set_skin_editor_visible(&mut self, visible: bool) {
        self.dialogs.skin_editor = visible;
    }

    pub(crate) fn is_output_device_picker_visible(&self) -> bool {
        self.dialogs.output_device_picker
    }

    pub(crate) fn set_output_device_picker_visible(&mut self, visible: bool) {
        self.dialogs.output_device_picker = visible;
    }

    pub(crate) fn set_output_devices(&mut self, system_devices: Vec<OutputDevice>) {
        self.output_device_groups = group_output_devices(system_devices);
    }

    pub(crate) fn output_device_groups(&self) -> &OutputDeviceGroups {
        &self.output_device_groups
    }

    pub(crate) fn selected_output_device(&self) -> Option<&str> {
        self.store.state().config.output_device.as_deref()
    }

    pub(crate) fn select_output_device(&mut self, selection: OutputDeviceSelection<'_>) -> bool {
        match selection {
            OutputDeviceSelection::Automatic => {
                self.update_config_via_store(|config| config.output_device = None);
                self.output_switch_count = self.output_switch_count.saturating_add(1);
                true
            }
            OutputDeviceSelection::System(id) => {
                let found = self
                    .output_device_groups
                    .local
                    .iter()
                    .chain(self.output_device_groups.network.iter())
                    .any(|device| device.id == id);
                if !found {
                    return false;
                }
                self.update_config_via_store(|config| config.output_device = Some(id.to_string()));
                self.output_switch_count = self.output_switch_count.saturating_add(1);
                true
            }
        }
    }

    pub(crate) fn output_switch_count(&self) -> u32 {
        self.output_switch_count
    }

    pub(crate) fn mpris_root_properties(&self) -> MprisRootProperties {
        mpris_root_properties()
    }

    pub(crate) fn mpris_player_properties(&self) -> MprisPlayerProperties {
        mpris_player_properties(
            self.store.state(),
            self.store.state().config.playback_position_ms,
        )
    }

    pub(crate) fn mpris_events(&self) -> &[MprisEvent] {
        &self.mpris_events
    }

    pub(crate) fn take_mpris_events(&mut self) -> Vec<MprisEvent> {
        std::mem::take(&mut self.mpris_events)
    }

    pub(crate) fn set_mpris_volume(&mut self, volume: f64) {
        let percent = (volume * 100.0) as i32;
        self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetVolume(percent));
    }

    pub(crate) fn execute_mpris_command(&mut self, command: MprisCommand) {
        let playback_position_ms = self.store.state().config.playback_position_ms;
        match app_action_for_mpris_command(&command, playback_position_ms) {
            MprisAppAction::Raise => self.mpris_events.push(MprisEvent::Raised),
            MprisAppAction::Quit => self.mpris_events.push(MprisEvent::QuitRequested),
            MprisAppAction::Dispatch(app_command) => {
                self.dispatch_store_command_and_apply_local_effects(app_command);
                match command {
                    MprisCommand::Seek { .. } | MprisCommand::SetPosition { .. } => {
                        self.mpris_events.push(MprisEvent::Seeked(
                            self.store.state().config.playback_position_ms * 1_000,
                        ));
                    }
                    MprisCommand::Stop => {
                        self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
                        self.mpris_events.push(MprisEvent::Seeked(
                            self.store.state().config.playback_position_ms * 1_000,
                        ));
                    }
                    MprisCommand::Next
                    | MprisCommand::Previous
                    | MprisCommand::Pause
                    | MprisCommand::PlayPause
                    | MprisCommand::Play => {
                        self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
                    }
                    MprisCommand::Raise | MprisCommand::Quit | MprisCommand::OpenUri(_) => {}
                }
            }
            MprisAppAction::OpenUri(uri) => {
                self.accept_dropped_uris([uri.as_str()], true, true);
                self.mpris_events.push(MprisEvent::MetadataChanged);
                self.mpris_events.push(MprisEvent::PlaybackStatusChanged);
            }
        }
    }

    pub(crate) fn scan_skin_browser_dirs<P: AsRef<Path>>(&mut self, dirs: &[P]) -> io::Result<()> {
        self.skin_browser.entries = discover_skins_in_dirs(dirs)?;
        self.skin_browser.selected_index = self
            .store
            .state()
            .config
            .skin
            .as_deref()
            .and_then(|current| {
                self.skin_browser
                    .entries
                    .iter()
                    .position(|entry| entry.path == Path::new(current))
                    .map(|index| index + 1)
            })
            .unwrap_or(0);
        Ok(())
    }

    pub(crate) fn skin_browser_entries(&self) -> &[SkinEntry] {
        &self.skin_browser.entries
    }

    pub(crate) fn selected_skin_index(&self) -> usize {
        self.skin_browser.selected_index
    }

    pub(crate) fn selected_skin(&self) -> Option<&str> {
        self.store.state().config.skin.as_deref()
    }

    pub(crate) fn select_skin_browser_index(&mut self, index: usize) -> bool {
        let previous_skin = self.store.state().config.skin.clone();
        let previous_index = self.skin_browser.selected_index;
        let next_skin = if index == 0 {
            None
        } else {
            let Some(entry) = self.skin_browser.entries.get(index - 1) else {
                return false;
            };
            Some(entry.path.display().to_string())
        };
        self.update_config_via_store(|config| config.skin = next_skin);
        self.skin_browser.selected_index = index;

        if let Err(err) = self.reload_skin() {
            eprintln!("xmms-rs: failed to load selected skin: {err}");
            self.update_config_via_store(|config| config.skin = previous_skin);
            self.skin_browser.selected_index = previous_index;
            return false;
        }
        true
    }

    pub(crate) fn reload_skin(&mut self) -> io::Result<()> {
        self.load_configured_skin()?;
        self.skin_browser.reload_count = self.skin_browser.reload_count.saturating_add(1);
        Ok(())
    }

    pub(crate) fn skin_reload_count(&self) -> u32 {
        self.skin_browser.reload_count
    }

    pub(crate) fn clone_configured_skin_for_editor(&mut self) -> io::Result<()> {
        self.load_configured_skin()?;
        let name = self
            .store
            .state()
            .config
            .skin
            .as_deref()
            .and_then(|path| Path::new(path).file_stem())
            .and_then(|name| name.to_str())
            .map(|name| format!("{name} copy"))
            .unwrap_or_else(|| "Default Skin Copy".to_string());
        self.skin_editor.working_name = name;
        Ok(())
    }

    pub(crate) fn save_editor_skin_to_user_dir(&mut self) -> io::Result<PathBuf> {
        let user_skin_dir = user_skin_import_dir();
        fs::create_dir_all(&user_skin_dir)?;
        let name = sanitized_skin_name(&self.skin_editor.working_name);
        let destination = unique_import_destination(&user_skin_dir, std::ffi::OsStr::new(&name));
        self.active_skin.save_to_dir(&destination)?;
        self.update_config_via_store(|config| {
            config.skin = Some(destination.display().to_string())
        });
        self.reload_skin()?;
        self.scan_skin_browser_dirs(&runtime_skin_browser_dirs())?;
        Ok(destination)
    }

    pub(crate) fn export_editor_skin_wsz(&self, path: &Path) -> io::Result<()> {
        self.active_skin.export_wsz(path)
    }

    pub(crate) fn active_skin_pixel_argb(
        &self,
        kind: SkinPixmapKind,
        x: usize,
        y: usize,
    ) -> Option<u32> {
        self.active_skin
            .get(kind)
            .and_then(|image| image.pixel_argb(x, y))
    }

    pub(crate) fn toggle_sticky(&mut self) {
        let sticky = !self.store.state().config.sticky;
        self.update_config_via_store(|config| config.sticky = sticky);
    }

    pub(crate) fn sticky(&self) -> bool {
        self.store.state().config.sticky
    }

    pub(crate) fn toggle_double_size(&mut self) {
        self.double_fractional_scale();
    }

    pub(crate) fn double_fractional_scale(&mut self) {
        let scale = self.store.state().config.scale_factor * 2.0;
        self.update_config_via_store(|config| set_config_scale_factor(config, scale));
    }

    pub(crate) fn halve_fractional_scale(&mut self) {
        let scale = self.store.state().config.scale_factor / 2.0;
        self.update_config_via_store(|config| set_config_scale_factor(config, scale));
    }

    pub(crate) fn double_size(&self) -> bool {
        self.store.state().config.doublesize
    }

    pub(crate) fn toggle_easy_move(&mut self) {
        let easy_move = !self.store.state().config.easy_move;
        self.update_config_via_store(|config| config.easy_move = easy_move);
    }

    pub(crate) fn show_selected_or_current_file_info(&mut self) {
        self.last_playlist_file_info = self
            .selected_or_current_file_info_details()
            .map(|details| details.title);
    }

    pub(crate) fn selected_or_current_file_info_details(&mut self) -> Option<FileInfoDetails> {
        let details = {
            let state = self.store.state();
            self.selected_playlist_index()
                .or_else(|| state.playlist.position())
                .and_then(|index| state.playlist.entries().get(index))
                .or_else(|| state.playlist.entries().first())
                .map(file_info_details_for_entry)
        };
        self.last_playlist_file_info = details.as_ref().map(|details| details.title.clone());
        self.dispatch_store_command_and_apply_local_effects(UiCommand::SetFileInfoVisible(
            details.is_some(),
        ));
        details
    }

    pub(crate) fn is_file_info_dialog_visible(&self) -> bool {
        self.store.state().ui.file_info_visible
    }

    pub(crate) fn set_file_info_dialog_visible(&mut self, visible: bool) {
        self.dispatch_store_command_and_apply_local_effects(UiCommand::SetFileInfoVisible(visible));
    }

    pub(crate) fn select_first_playlist_entry(&mut self) -> bool {
        if self.store.state().playlist.is_empty() {
            return false;
        }
        self.select_single_playlist_entry(0);
        self.scroll_playlist_entry_into_view(0);
        true
    }

    pub(crate) fn play_first_playlist_entry(&mut self) {
        if !self.store.state().playlist.is_empty() {
            self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::SetPosition(0));
            self.dispatch_store_command_and_apply_local_effects(PlayerCommand::StartCurrentTrack);
        }
    }

    pub(crate) fn is_file_dialog_visible(&self) -> bool {
        self.dialogs.file
    }

    pub(crate) fn set_file_dialog_visible(&mut self, visible: bool) {
        self.dialogs.file = visible;
    }

    pub(crate) fn is_directory_dialog_visible(&self) -> bool {
        self.dialogs.directory
    }

    pub(crate) fn set_directory_dialog_visible(&mut self, visible: bool) {
        self.dialogs.directory = visible;
    }

    pub(crate) fn is_playlist_load_dialog_visible(&self) -> bool {
        self.dialogs.playlist_load
    }

    pub(crate) fn set_playlist_load_dialog_visible(&mut self, visible: bool) {
        self.dialogs.playlist_load = visible;
    }

    pub(crate) fn is_playlist_save_dialog_visible(&self) -> bool {
        self.dialogs.playlist_save
    }

    pub(crate) fn set_playlist_save_dialog_visible(&mut self, visible: bool) {
        self.dialogs.playlist_save = visible;
    }

    pub(crate) fn last_playlist_file_info(&self) -> Option<&str> {
        self.last_playlist_file_info.as_deref()
    }

    pub(crate) fn update_playlist_title_for_uri(&mut self, uri: &str, title: &str) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::UpdateTitleForUri {
            uri: uri.to_string(),
            title: title.to_string(),
        });
        self.last_playlist_file_info = Some(title.to_string());
    }

    pub(crate) fn playlist_options_opened(&self) -> bool {
        self.playlist_options_opened
    }

    pub(crate) fn load_playlist_file(&mut self, path: &Path) -> std::io::Result<()> {
        let playlist = Playlist::load_m3u_file(path)?;
        let effects = self.store.replace_playlist_for_file_load(playlist).effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
        self.playlist_ui.scroll_offset = 0;
        self.playlist_ui.search.stop();
        self.schedule_missing_local_playlist_durations();
        Ok(())
    }

    pub(crate) fn save_playlist_file(&self, path: &Path) -> std::io::Result<()> {
        self.store.state().playlist.save_m3u_file(path)
    }

    pub(crate) fn last_open_location(&self) -> Option<&str> {
        self.last_open_location.as_deref()
    }

    pub(crate) fn last_jump_time_ms(&self) -> Option<i64> {
        self.last_jump_time_ms
    }

    pub(crate) fn playlist_len(&self) -> usize {
        self.store.state().playlist.len()
    }

    pub(crate) fn playlist_entry_uri(&self, index: usize) -> Option<&str> {
        self.store
            .state()
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.filename.as_str())
    }

    pub(crate) fn playlist_entry_title(&self, index: usize) -> Option<&str> {
        self.store
            .state()
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.title.as_str())
    }

    pub(crate) fn playlist_entry_length_ms(&self, index: usize) -> Option<i64> {
        self.store
            .state()
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.length_ms)
    }

    pub(crate) fn playlist_entry_selected(&self, index: usize) -> Option<bool> {
        self.store
            .state()
            .playlist
            .entries()
            .get(index)
            .map(|entry| entry.selected)
    }

    pub(crate) fn visible_playlist_entry_uri(&self, row: usize) -> Option<&str> {
        self.playlist_ui
            .scroll_offset
            .checked_add(row)
            .and_then(|index| self.playlist_entry_uri(index))
    }

    pub(crate) fn visible_playlist_entry_title(&self, row: usize) -> Option<String> {
        self.playlist_ui
            .scroll_offset
            .checked_add(row)
            .and_then(|index| self.store.state().playlist.entries().get(index))
            .map(|entry| self.formatted_playlist_entry_title(entry))
    }

    pub(crate) fn playlist_position(&self) -> Option<usize> {
        self.store.state().playlist.position()
    }

    pub(crate) fn current_playlist_entry_uri(&self) -> Option<&str> {
        self.store
            .state()
            .playlist
            .position()
            .and_then(|position| self.playlist_entry_uri(position))
    }

    #[allow(dead_code)]
    fn start_current_playlist_playback(&mut self) {
        self.start_current_playlist_playback_at(
            self.playback_transition
                .play_start_position_ms(self.store.state().config.playback_position_ms),
        );
    }

    #[allow(dead_code)]
    fn start_current_playlist_playback_from_beginning(&mut self) {
        self.start_current_playlist_playback_at(0);
    }

    #[allow(dead_code)]
    fn start_current_playlist_playback_at(&mut self, position_ms: i64) {
        self.dispatch_store_command_and_apply_local_effects(PlayerCommand::StartCurrentTrack);
        if position_ms > 0 {
            self.dispatch_store_command_and_apply_local_effects(PlayerCommand::SeekToMs(
                position_ms,
            ));
        } else {
            self.playback_transition = PlaybackTransitionState::Idle;
            let result = self.store.update_playback_position_from_runtime(0);
            for effect in result.effects {
                self.apply_store_effect(effect);
            }
        }
    }

    fn load_equalizer_auto_preset_for_uri(&mut self, uri: &str) {
        if !self.store.state().config.equalizer_auto {
            return;
        }
        let Some(path) = file_uri_to_path(uri) else {
            self.load_equalizer_default_preset();
            return;
        };

        if !self.store.state().config.eqpreset_extension.is_empty() {
            let per_file = PathBuf::from(format!(
                "{}.{}",
                path.to_string_lossy(),
                self.store.state().config.eqpreset_extension
            ));
            match load_xmms_preset_file(&per_file) {
                Ok(Some(preset)) => {
                    self.apply_equalizer_preset_values(&preset);
                    return;
                }
                Ok(None) => {}
                Err(err) => eprintln!(
                    "xmms-rs: failed to load equalizer preset {}: {err}",
                    per_file.display()
                ),
            }
        }

        if !self.store.state().config.eqpreset_default_file.is_empty() {
            if let Some(parent) = path.parent() {
                let directory_preset =
                    parent.join(&self.store.state().config.eqpreset_default_file);
                match load_xmms_preset_file(&directory_preset) {
                    Ok(Some(preset)) => {
                        self.apply_equalizer_preset_values(&preset);
                        return;
                    }
                    Ok(None) => {}
                    Err(err) => eprintln!(
                        "xmms-rs: failed to load equalizer preset {}: {err}",
                        directory_preset.display()
                    ),
                }
            }
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| self.load_named_equalizer_preset(name, true))
        {
            return;
        }
        self.load_equalizer_default_preset();
    }

    fn pause_playback(&mut self) {
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().pause() {
                eprintln!("xmms-rs: failed to pause playback: {err}");
            }
        }
    }

    fn unpause_playback(&mut self) {
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().unpause() {
                eprintln!("xmms-rs: failed to resume playback: {err}");
            }
        }
    }

    #[allow(dead_code)]
    fn handle_playback_control_event(&mut self, event: PlaybackControlEvent) -> bool {
        let command = match event {
            PlaybackControlEvent::Play => PlayerCommand::Play,
            PlaybackControlEvent::Pause => PlayerCommand::Pause,
            PlaybackControlEvent::Halt => PlayerCommand::Halt,
            PlaybackControlEvent::Previous => PlayerCommand::PreviousTrack,
            PlaybackControlEvent::Next => PlayerCommand::NextTrack,
        };
        let result = self.dispatch_store_command(command);
        let changed = !result.changes.is_empty() || !result.effects.is_empty();
        for effect in result.effects {
            self.apply_store_effect(effect);
        }
        changed
    }

    fn stop_playback(&mut self) {
        self.playback_transition = PlaybackTransitionState::stop_playback();
        if let Some(backend) = &self.playback_backend {
            if let Err(err) = backend.borrow().stop() {
                eprintln!("xmms-rs: failed to stop playback: {err}");
            }
        }
        self.visualization.clear_data();
    }

    #[allow(dead_code)]
    fn request_stop_playback(&mut self) {
        if self.store.state().config.stop_with_fadeout {
            self.stop_with_fade();
        } else {
            self.stop_playback();
        }
    }

    #[allow(dead_code)]
    fn stop_with_fade(&mut self) {
        if self.store.state().player.state() == PlayerState::Stopped {
            self.stop_playback();
            return;
        }
        let start_volume = self.store.state().player.volume().max(0);
        if start_volume == 0 {
            self.stop_playback();
            return;
        }
        self.playback_transition = PlaybackTransitionState::start_fadeout(start_volume);
    }

    fn set_runtime_volume(&mut self, volume: i32) {
        let result = self.store.set_runtime_volume_for_transition(volume);
        for effect in result.effects {
            self.apply_store_effect(effect);
        }
    }

    pub(crate) fn playback_position_ms(&self) -> i64 {
        self.store.state().config.playback_position_ms
    }

    pub(crate) fn last_playback_request(&self) -> Option<&str> {
        self.last_playback_request.as_deref()
    }

    pub(crate) fn add_timed_entry(&mut self, uri: &str, title: &str, duration_ms: i64) {
        let mut playlist = self.store.state().playlist.clone();
        playlist.add_timed_uri(uri, title, duration_ms);
        let effects = self.store.replace_playlist_for_file_load(playlist).effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
    }

    pub(crate) fn add_playlist_uri(&mut self, uri: &str) {
        let effects = self
            .dispatch_store_command(PlaylistCommand::AddUris(vec![uri.to_string()]))
            .effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
    }

    pub(crate) fn set_stream_channels_for_e2e(&mut self, channels: i32) {
        let result = self.store.handle_playback_event(PlaybackEvent::StreamInfo(
            crate::player::StreamInfo {
                bitrate: None,
                frequency: None,
                channels: Some(channels),
            },
        ));
        for effect in result.effects {
            self.apply_store_effect(effect);
        }
    }

    pub(crate) fn set_visualization_data_for_e2e(
        &mut self,
        data: crate::audio_model::SpectrumData,
    ) {
        let result = self
            .store
            .handle_playback_event(PlaybackEvent::Spectrum(data));
        for effect in result.effects {
            self.apply_store_effect(effect);
        }
    }

    pub(crate) fn ensure_playing_for_e2e(&mut self) {
        if self.store.state().playlist.is_empty() {
            self.add_timed_entry("file:///xmms-renascene-e2e.ogg", "E2E", -1);
        }
        if self.store.state().player.state() != PlayerState::Playing {
            self.dispatch_store_command_and_apply_local_effects(PlayerCommand::StartCurrentTrack);
        }
    }

    pub(crate) fn save_runtime_snapshot_for_e2e(
        &mut self,
        config_path: &Path,
        playlist_path: &Path,
    ) -> io::Result<()> {
        self.save_runtime_snapshot(config_path, playlist_path)
    }

    pub(crate) fn set_playlist_entry_selected(&mut self, index: usize, selected: bool) {
        if self
            .store
            .state()
            .playlist
            .entries()
            .get(index)
            .is_some_and(|entry| entry.selected != selected)
        {
            self.dispatch_store_command_and_apply_local_effects(
                PlaylistCommand::ToggleEntrySelection(index),
            );
        }
    }

    pub(crate) fn start_playlist_search(&mut self) -> bool {
        if !self.store.state().config.vim_playlist_navigation {
            return false;
        }
        self.playlist_ui.menu.close();
        self.playlist_ui.search.start();
        true
    }

    pub(crate) fn stop_playlist_search(&mut self) {
        self.playlist_ui.search.stop();
    }

    pub(crate) fn push_playlist_search_char(&mut self, ch: char) {
        if self.playlist_ui.search.push_char(ch) {
            self.update_playlist_search_match();
        }
    }

    pub(crate) fn pop_playlist_search_char(&mut self) {
        if self.playlist_ui.search.pop_char() {
            self.update_playlist_search_match();
        }
    }

    pub(crate) fn sort_playlist_by(&mut self, key: PlaylistSortKey) {
        self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::Sort(key));
    }

    pub(crate) fn sort_selected_playlist_by(&mut self, key: PlaylistSortKey) {
        self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::SortSelected(key));
    }

    pub(crate) fn remove_selected_playlist_entries(&mut self) -> bool {
        let before = self.store.state().playlist.len();
        self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::RemoveSelected);
        self.store.state().playlist.len() != before
    }

    pub(crate) fn reverse_playlist(&mut self) {
        self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::Reverse);
    }

    pub(crate) fn randomize_playlist(&mut self) {
        self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::Randomize);
    }

    pub(crate) fn index_missing_playlist_durations_for_e2e(&mut self) {
        let mut playlist = self.store.state().playlist.clone();
        let _ = playlist.index_missing_durations_with(|item| {
            Ok::<_, std::convert::Infallible>(Some(DurationIndexResult {
                index: item.index,
                uri: item.uri.clone(),
                length_ms: ((item.index + 1) as i64) * 1_000,
                title: Some(format!("Indexed {}", item.index + 1)),
            }))
        });
        let effects = self.store.replace_playlist_for_file_load(playlist).effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
    }

    pub(crate) fn queue_playlist_duration_result_for_e2e(
        &mut self,
        index: usize,
        length_ms: i64,
        title: Option<String>,
    ) {
        let Some(uri) = self.playlist_entry_uri(index).map(ToString::to_string) else {
            return;
        };
        let _ = self.duration_index_sender.send(DurationIndexResult {
            index,
            uri,
            length_ms,
            title,
        });
    }

    fn schedule_missing_local_playlist_durations(&mut self) {
        let items = self
            .store
            .state()
            .playlist
            .missing_duration_items()
            .into_iter()
            .filter(|item| file_uri_to_path(&item.uri).is_some_and(|path| path.exists()))
            .collect::<Vec<_>>();
        if items.is_empty() {
            return;
        }

        let sender = self.duration_index_sender.clone();
        thread::spawn(move || {
            #[cfg(feature = "rodio-backend")]
            {
                use crate::playback::backend::AudioMetadataProbe as _;

                let probe = crate::playback::rodio::RodioMetadataProbe;
                for item in items {
                    match probe.probe(&item) {
                        Ok(Some(result)) => {
                            if sender.send(result).is_err() {
                                return;
                            }
                        }
                        Ok(None) => {}
                        Err(err) => eprintln!(
                            "xmms-rs: failed to probe playlist item {} with rodio: {err}",
                            item.uri
                        ),
                    }
                }
            }
            #[cfg(all(not(feature = "rodio-backend"), feature = "gstreamer-backend"))]
            {
                if let Err(err) = gstreamer::init() {
                    eprintln!(
                        "xmms-rs: failed to initialize GStreamer for playlist durations: {err}"
                    );
                    return;
                }
                let discoverer =
                    match gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(5))
                    {
                        Ok(discoverer) => discoverer,
                        Err(err) => {
                            eprintln!(
                                "xmms-rs: failed to create playlist duration discoverer: {err}"
                            );
                            return;
                        }
                    };

                for item in items {
                    let Some(path) = file_uri_to_path(&item.uri).filter(|path| path.exists())
                    else {
                        continue;
                    };
                    let info = match discoverer.discover_uri(&item.uri) {
                        Ok(info) => info,
                        Err(err) => {
                            eprintln!(
                                "xmms-rs: failed to discover playlist item {}: {err}",
                                path.display()
                            );
                            continue;
                        }
                    };
                    let length_ms = info
                        .duration()
                        .map(|duration| duration.mseconds() as i64)
                        .unwrap_or(-1);
                    if sender
                        .send(DurationIndexResult {
                            index: item.index,
                            uri: item.uri,
                            length_ms,
                            title: None,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
    }

    fn poll_duration_index_results(&mut self) -> GtkTickRedraw {
        let mut redraw = GtkTickRedraw::default();
        while let Ok(result) = self.duration_index_receiver.try_recv() {
            let dispatch = self.store.apply_duration_index_result(result);
            redraw.merge(GtkTickRedraw::from_changes(dispatch.changes));
            for effect in dispatch.effects {
                self.apply_store_effect(effect);
            }
        }
        redraw
    }

    pub(crate) fn accept_open_location(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.last_open_location = Some(text.to_string());
        let before = self.store.state().playlist.len();
        let effects = self
            .dispatch_store_command(PlaylistCommand::AddLocations(vec![text.to_string()]))
            .effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
        if self.store.state().playlist.len() > before {
            self.schedule_missing_local_playlist_durations();
            if self.store.state().playlist.position().is_none() {
                self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::SetPosition(
                    0,
                ));
            }
            self.dispatch_store_command_and_apply_local_effects(PlayerCommand::StartCurrentTrack);
        }
        self.dialogs.open_location = false;
    }

    pub(crate) fn accept_dropped_uris<I, S>(
        &mut self,
        uris: I,
        clear_first: bool,
        start_playback: bool,
    ) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let locations = uris
            .into_iter()
            .map(|location| location.as_ref().to_string())
            .filter(|location| !location.is_empty())
            .collect::<Vec<_>>();
        if locations.is_empty() {
            return false;
        }
        if clear_first {
            self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::Clear);
        }
        let before = self.store.state().playlist.len();
        let effects = self
            .dispatch_store_command(PlaylistCommand::AddLocations(locations))
            .effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
        let accepted = self.store.state().playlist.len() > before
            || (clear_first && !self.store.state().playlist.is_empty());
        if accepted && clear_first {
            self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::SetPosition(0));
        }
        if accepted {
            self.schedule_missing_local_playlist_durations();
        }
        if accepted && start_playback {
            self.dispatch_store_command_and_apply_local_effects(PlayerCommand::StartCurrentTrack);
        }
        accepted
    }

    pub(crate) fn accept_opened_uris<I, S>(&mut self, uris: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.accept_dropped_uris(uris, true, true)
    }

    pub(crate) fn accept_jump_time(&mut self, text: &str) {
        let Some(ms) = parse_time_ms(text) else {
            return;
        };
        self.last_jump_time_ms = Some(ms);
        self.dispatch_store_command_and_apply_local_effects(PlayerCommand::SeekToMs(ms));
        self.dialogs.jump_time = false;
    }

    pub(crate) fn set_playlist_size(&mut self, width: i32, height: i32) -> bool {
        let size = snap_playlist_size(width, height);
        let (width, height) = (size.width, size.height);
        let changed = self.playlist_ui.width != width || self.playlist_ui.height != height;
        self.playlist_ui.width = width;
        self.playlist_ui.height = height;
        self.clamp_playlist_scroll_offset();
        changed
    }

    pub(crate) fn set_panel_dragging(&mut self, kind: PanelKind, dragging: bool) {
        self.panel_ui_mut(kind).dragging_title = dragging;
    }

    pub(crate) fn set_panel_focused(&mut self, kind: PanelKind, focused: bool) {
        self.panel_ui_mut(kind).focused = focused;
    }

    pub(crate) fn is_panel_focused(&self, kind: PanelKind) -> bool {
        match kind {
            PanelKind::Equalizer => self.equalizer.panel.focused,
            PanelKind::Playlist => self.playlist_ui.panel.focused,
        }
    }

    pub(crate) fn equalizer_active(&self) -> bool {
        self.store.state().config.equalizer_active
    }

    pub(crate) fn equalizer_automatic(&self) -> bool {
        self.store.state().config.equalizer_auto
    }

    pub(crate) fn equalizer_preamp_position(&self) -> i32 {
        self.store.state().config.equalizer_preamp_pos
    }

    pub(crate) fn equalizer_band_position(&self, band: usize) -> Option<i32> {
        self.store
            .state()
            .config
            .equalizer_band_pos
            .get(band)
            .copied()
    }

    pub(crate) fn equalizer_preamp_db(&self) -> f64 {
        equalizer_position_to_db(self.store.state().config.equalizer_preamp_pos)
    }

    pub(crate) fn equalizer_band_db(&self, band: usize) -> Option<f64> {
        self.store
            .state()
            .config
            .equalizer_band_pos
            .get(band)
            .map(|position| equalizer_position_to_db(*position))
    }

    pub(crate) fn equalizer_gstreamer_band_db_values(&self) -> EqualizerBandDb {
        let config = &self.store.state().config;
        if config.equalizer_active {
            config.equalizer_band_pos.map(equalizer_position_to_db)
        } else {
            [0.0; crate::audio_model::EQUALIZER_BANDS]
        }
    }

    pub(crate) fn equalizer_presets_pressed(&self) -> bool {
        self.equalizer.pointer.pressed_control() == Some(EqualizerControl::Presets)
    }

    pub(crate) fn equalizer_press(&mut self, x: i32, y: i32) -> bool {
        if self.is_equalizer_shaded() {
            if let Some(slider) = equalizer_shaded_slider_at(x, y) {
                self.equalizer.keyboard_slider = Some(slider);
                self.equalizer.pointer = EqualizerPointer::DraggingSlider {
                    slider,
                    offset: self.begin_equalizer_slider_drag(slider, x, y),
                };
                return true;
            }
            return false;
        }

        if let Some(control) = equalizer_control_at(x, y) {
            self.equalizer.keyboard_slider = None;
            self.equalizer.pointer = EqualizerPointer::PressedControl {
                control,
                inside: true,
            };
            return true;
        }

        if let Some(slider) = equalizer_slider_at(x, y) {
            self.equalizer.keyboard_slider = Some(slider);
            self.equalizer.pointer = EqualizerPointer::DraggingSlider {
                slider,
                offset: self.begin_equalizer_slider_drag(slider, x, y),
            };
            return true;
        }

        false
    }

    pub(crate) fn equalizer_motion(&mut self, x: i32, y: i32) -> bool {
        match self.equalizer.pointer {
            EqualizerPointer::Idle => false,
            EqualizerPointer::PressedControl { control, inside } => {
                let next_inside = equalizer_control_at(x, y) == Some(control);
                let changed = inside != next_inside;
                self.equalizer.pointer = EqualizerPointer::PressedControl {
                    control,
                    inside: next_inside,
                };
                changed
            }
            EqualizerPointer::DraggingSlider { slider, offset } => {
                let coordinate = match slider {
                    EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => x,
                    EqualizerSlider::Preamp | EqualizerSlider::Band(_) => y,
                };
                self.set_equalizer_slider_position(slider, coordinate, offset)
            }
        }
    }

    pub(crate) fn equalizer_scroll(&mut self, x: i32, y: i32, dy: f64) -> bool {
        let slider = if self.is_equalizer_shaded() {
            equalizer_shaded_slider_at(x, y)
        } else {
            equalizer_slider_at(x, y)
        };
        let Some(slider) = slider else {
            return false;
        };
        match slider {
            EqualizerSlider::ShadedVolume => self.scroll_volume(dy),
            EqualizerSlider::ShadedBalance => self.scroll_balance(dy),
            EqualizerSlider::Preamp | EqualizerSlider::Band(_) => {
                let diff = if dy < 0.0 {
                    -4
                } else if dy > 0.0 {
                    4
                } else {
                    return false;
                };
                self.adjust_equalizer_slider(slider, diff)
            }
        }
    }

    pub(crate) fn adjust_selected_equalizer_slider(&mut self, diff: i32) -> bool {
        let slider = self
            .equalizer
            .keyboard_slider
            .unwrap_or(EqualizerSlider::Preamp);
        self.adjust_equalizer_slider(slider, diff)
    }

    fn adjust_equalizer_slider(&mut self, slider: EqualizerSlider, diff: i32) -> bool {
        match slider {
            EqualizerSlider::Preamp => {
                let current = self.store.state().config.equalizer_preamp_pos;
                let next = (current + diff).clamp(0, 100);
                let changed = current != next;
                if changed {
                    self.dispatch_store_command_and_apply_local_effects(
                        EqualizerCommand::SetPreamp(next),
                    );
                }
                changed
            }
            EqualizerSlider::Band(band) => {
                let Some(value) = self
                    .store
                    .state()
                    .config
                    .equalizer_band_pos
                    .get(band)
                    .copied()
                else {
                    return false;
                };
                let next = (value + diff).clamp(0, 100);
                let changed = value != next;
                if changed {
                    self.dispatch_store_command_and_apply_local_effects(
                        EqualizerCommand::SetBand {
                            band,
                            position: next,
                        },
                    );
                }
                changed
            }
            EqualizerSlider::ShadedVolume => self.adjust_volume_by(-diff),
            EqualizerSlider::ShadedBalance => self.adjust_balance_by(-diff),
        }
    }

    pub(crate) fn equalizer_release(&mut self, x: i32, y: i32) -> PanelAction {
        match std::mem::take(&mut self.equalizer.pointer) {
            EqualizerPointer::PressedControl { control, inside } => {
                let activated = inside && equalizer_control_at(x, y) == Some(control);
                if activated {
                    let control_name = format!("{control:?}");
                    app_log_info!(equalizer, "control activated", control_name);
                    match control {
                        EqualizerControl::On => {
                            self.dispatch_store_command_and_apply_local_effects(
                                EqualizerCommand::ToggleActive,
                            );
                        }
                        EqualizerControl::Auto => {
                            self.dispatch_store_command_and_apply_local_effects(
                                EqualizerCommand::ToggleAuto,
                            );
                        }
                        EqualizerControl::Presets => return PanelAction::ShowEqualizerPresets,
                    }
                }
                PanelAction::Changed
            }
            EqualizerPointer::DraggingSlider { .. } => PanelAction::Changed,
            EqualizerPointer::Idle => PanelAction::None,
        }
    }

    pub(crate) fn apply_equalizer_preset(&mut self, preset: i32) {
        let mut band_positions = [50; 10];
        match preset {
            1 => {
                band_positions[0] = 25;
                band_positions[1] = 30;
                band_positions[2] = 40;
            }
            2 => {
                band_positions[7] = 40;
                band_positions[8] = 30;
                band_positions[9] = 25;
            }
            3 => {
                band_positions[0] = 30;
                band_positions[1] = 35;
                band_positions[4] = 60;
                band_positions[5] = 60;
                band_positions[8] = 35;
                band_positions[9] = 30;
            }
            _ => {}
        }
        let effects = self
            .store
            .apply_equalizer_preset_positions(50, band_positions)
            .effects;
        for effect in effects {
            self.apply_store_effect(effect);
        }
    }

    fn set_equalizer_slider_position(
        &mut self,
        slider: EqualizerSlider,
        coordinate: i32,
        offset: i32,
    ) -> bool {
        let changed = match slider {
            EqualizerSlider::Preamp => {
                let position = eq_slider_pixel_to_position(
                    coordinate - equalizer_slider_layout(slider).rect.y - offset,
                );
                let changed = self.store.state().config.equalizer_preamp_pos != position;
                if changed {
                    self.dispatch_store_command_and_apply_local_effects(
                        EqualizerCommand::SetPreamp(position),
                    );
                }
                changed
            }
            EqualizerSlider::Band(band) => {
                let position = eq_slider_pixel_to_position(
                    coordinate - equalizer_slider_layout(slider).rect.y - offset,
                );
                let changed = self
                    .store
                    .state()
                    .config
                    .equalizer_band_pos
                    .get(band)
                    .is_some_and(|value| *value != position);
                if changed {
                    self.dispatch_store_command_and_apply_local_effects(
                        EqualizerCommand::SetBand { band, position },
                    );
                }
                changed
            }
            EqualizerSlider::ShadedVolume => {
                let position =
                    (coordinate - equalizer_slider_layout(slider).rect.x - offset).clamp(0, 94);
                let volume = eq_shaded_position_to_volume(position);
                let changed = self.store.state().player.volume() != volume;
                if changed {
                    self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetVolume(
                        volume,
                    ));
                }
                changed
            }
            EqualizerSlider::ShadedBalance => {
                let position =
                    (coordinate - equalizer_slider_layout(slider).rect.x - offset).clamp(0, 39);
                let balance = eq_shaded_position_to_balance(position);
                let changed = self.store.state().player.balance() != balance;
                if changed {
                    self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetBalance(
                        balance,
                    ));
                }
                changed
            }
        };
        changed
    }

    fn begin_equalizer_slider_drag(&mut self, slider: EqualizerSlider, x: i32, y: i32) -> i32 {
        let layout = equalizer_slider_layout(slider);
        match slider {
            EqualizerSlider::Preamp | EqualizerSlider::Band(_) => {
                let position = self.equalizer_slider_pixel_position(slider);
                let local_y = y - layout.rect.y;
                if local_y >= position && local_y < position + 11 {
                    local_y - position
                } else {
                    let offset = 5;
                    self.set_equalizer_slider_position(slider, y, offset);
                    offset
                }
            }
            EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => {
                let position = self.equalizer_slider_pixel_position(slider);
                let local_x = x - layout.rect.x;
                if local_x >= position && local_x < position + layout.knob_size.width {
                    local_x - position
                } else {
                    let offset = layout.knob_size.width / 2;
                    self.set_equalizer_slider_position(slider, x, offset);
                    offset
                }
            }
        }
    }

    fn equalizer_slider_pixel_position(&self, slider: EqualizerSlider) -> i32 {
        match slider {
            EqualizerSlider::Preamp => {
                eq_slider_position_to_pixel(self.store.state().config.equalizer_preamp_pos)
            }
            EqualizerSlider::Band(band) => self
                .store
                .state()
                .config
                .equalizer_band_pos
                .get(band)
                .copied()
                .map(eq_slider_position_to_pixel)
                .unwrap_or(25),
            EqualizerSlider::ShadedVolume => {
                volume_to_eq_shaded_position(self.store.state().player.volume())
            }
            EqualizerSlider::ShadedBalance => {
                balance_to_eq_shaded_position(self.store.state().player.balance())
            }
        }
    }

    fn sync_equalizer_to_backend(&self) {
        if let Some(backend) = &self.playback_backend {
            let config = &self.store.state().config;
            let _ = backend.borrow().set_equalizer(EqualizerBackendState {
                active: config.equalizer_active,
                preamp_position: config.equalizer_preamp_pos,
                band_positions: config.equalizer_band_pos,
            });
        }
    }

    pub(crate) fn panel_title_drag_region(&self, kind: PanelKind, x: i32, y: i32) -> bool {
        let title_height = match kind {
            PanelKind::Equalizer => MAIN_TITLEBAR_HEIGHT,
            PanelKind::Playlist => 20,
        };
        y >= 0
            && y < title_height
            && !self.panel_title_button_hit(kind, x, y)
            && !(kind == PanelKind::Equalizer
                && self.is_equalizer_shaded()
                && equalizer_shaded_slider_at(x, y).is_some())
    }

    pub(crate) fn main_title_drag_region(&self, x: i32, y: i32) -> bool {
        (0..MAIN_TITLEBAR_HEIGHT).contains(&y) && self.hit_test(x, y).is_none()
    }

    pub(crate) fn playlist_resize_region(&self, x: i32, y: i32) -> bool {
        !self.is_playlist_shaded()
            && x > self.playlist_ui.width - 20
            && y > self.playlist_ui.height - 20
    }

    pub(crate) fn begin_docked_playlist_resize(&mut self, local_y: i32) -> bool {
        if !self.playlist_resize_region(self.playlist_ui.width - 1, local_y) {
            return false;
        }
        self.playlist_ui.pointer = PlaylistPointer::Resizing {
            offset_y: self.playlist_ui.height - local_y,
        };
        true
    }

    pub(crate) fn docked_playlist_resize_motion(&mut self, main_y: i32) -> bool {
        let PlaylistPointer::Resizing { offset_y } = self.playlist_ui.pointer else {
            return false;
        };
        let Some(local_y) = self.docked_playlist_local_y(main_y) else {
            return false;
        };
        let height = local_y + offset_y;
        self.set_playlist_size(PLAYLIST_MIN_WIDTH, height)
    }

    pub(crate) fn end_docked_playlist_resize(&mut self) -> bool {
        if matches!(self.playlist_ui.pointer, PlaylistPointer::Resizing { .. }) {
            self.playlist_ui.pointer = PlaylistPointer::Idle;
            true
        } else {
            false
        }
    }

    pub(crate) fn is_docked_playlist_resizing(&self) -> bool {
        matches!(self.playlist_ui.pointer, PlaylistPointer::Resizing { .. })
    }

    pub(crate) fn playlist_scrollbar_press(&mut self, x: i32, y: i32) -> bool {
        let Some((thumb_y, thumb_h)) = self.playlist_scrollbar_geometry() else {
            return false;
        };
        if !self.playlist_scrollbar_region(x, y) {
            return false;
        }
        let offset = if y >= thumb_y && y < thumb_y + thumb_h {
            y - thumb_y
        } else {
            thumb_h / 2
        };
        self.playlist_ui.pointer = PlaylistPointer::DraggingScrollbar { offset };
        self.update_playlist_scroll_from_thumb_y(y - offset);
        true
    }

    pub(crate) fn playlist_scrollbar_motion(&mut self, x: i32, y: i32) -> bool {
        let PlaylistPointer::DraggingScrollbar { offset } = self.playlist_ui.pointer else {
            return false;
        };
        let old = self.playlist_ui.scroll_offset;
        let _ = x;
        self.update_playlist_scroll_from_thumb_y(y - offset);
        old != self.playlist_ui.scroll_offset
    }

    pub(crate) fn playlist_scrollbar_release(&mut self) -> bool {
        if matches!(
            self.playlist_ui.pointer,
            PlaylistPointer::DraggingScrollbar { .. }
        ) {
            self.playlist_ui.pointer = PlaylistPointer::Idle;
            true
        } else {
            false
        }
    }

    pub(crate) fn playlist_scroll(&mut self, dy: f64) -> bool {
        let rows = if dy < 0.0 {
            -3
        } else if dy > 0.0 {
            3
        } else {
            return false;
        };
        self.scroll_playlist_rows(rows)
    }

    fn scroll_playlist_rows(&mut self, rows: i32) -> bool {
        let old = self.playlist_ui.scroll_offset;
        if rows < 0 {
            self.playlist_ui.scroll_offset = self
                .playlist_ui
                .scroll_offset
                .saturating_sub(rows.unsigned_abs() as usize);
        } else {
            self.playlist_ui.scroll_offset =
                self.playlist_ui.scroll_offset.saturating_add(rows as usize);
            self.clamp_playlist_scroll_offset();
        }
        old != self.playlist_ui.scroll_offset
    }

    pub(crate) fn playlist_press(&mut self, x: i32, y: i32) -> bool {
        self.playlist_press_with_ctrl(x, y, false)
    }

    pub(crate) fn playlist_press_with_ctrl(&mut self, x: i32, y: i32, ctrl_pressed: bool) -> bool {
        if let Some(item) = self.playlist_menu_item_at(x, y) {
            return self.playlist_ui.menu.press_item(item);
        }
        if self.playlist_ui.menu.is_open() {
            return false;
        }

        let Some(index) = self.playlist_entry_at(x, y) else {
            return false;
        };
        if ctrl_pressed {
            for command in playlist_row_click_commands(index, false, true) {
                self.dispatch_store_command_and_apply_local_effects(command);
            }
            self.playlist_ui.last_click = None;
            self.playlist_ui.pending_double_click = None;
            self.playlist_ui.pointer = PlaylistPointer::Idle;
            return true;
        }

        let now = Instant::now();
        let is_double_click = self
            .playlist_ui
            .last_click
            .is_some_and(|(last_index, last_time)| {
                last_index == index && now.duration_since(last_time) <= Duration::from_millis(500)
            });

        self.playlist_ui.last_click = Some((index, now));
        self.playlist_ui.pending_double_click = is_double_click.then_some(index);
        for command in playlist_row_click_commands(index, false, false) {
            self.dispatch_store_command_and_apply_local_effects(command);
        }
        self.playlist_ui.pointer = PlaylistPointer::DraggingEntry {
            index,
            moved: false,
        };
        true
    }

    pub(crate) fn activate_playlist_entry_at(&mut self, x: i32, y: i32) -> bool {
        if self.playlist_ui.menu.is_open() {
            return false;
        }
        let Some(index) = self.playlist_entry_at(x, y) else {
            return false;
        };
        self.activate_playlist_entry(index);
        true
    }

    fn activate_playlist_entry(&mut self, index: usize) {
        self.playlist_ui.last_click = None;
        self.playlist_ui.pending_double_click = None;
        self.playlist_ui.pointer = PlaylistPointer::Idle;
        for command in playlist_row_click_commands(index, true, false) {
            self.dispatch_store_command_and_apply_local_effects(command);
        }
    }

    pub(crate) fn playlist_motion(&mut self, x: i32, y: i32) -> bool {
        if let PlaylistPointer::DraggingEntry { index: from, .. } = self.playlist_ui.pointer {
            let Some(to) = self.playlist_entry_at(x, y) else {
                return false;
            };
            if !self
                .dispatch_store_command_and_apply_local_effects(PlaylistCommand::MoveEntry {
                    from,
                    to,
                })
                .changes
                .is_empty()
            {
                self.playlist_ui.pending_double_click = None;
                self.playlist_ui.pointer = PlaylistPointer::DraggingEntry {
                    index: to,
                    moved: true,
                };
                self.scroll_playlist_entry_into_view(to);
                return true;
            }
            return false;
        }

        if !self.playlist_ui.menu.is_open() {
            return false;
        }
        let item = self.playlist_menu_item_at(x, y);
        self.playlist_ui.menu.set_hover(item)
    }

    pub(crate) fn playlist_entry_release(&mut self) -> bool {
        let PlaylistPointer::DraggingEntry { moved, .. } = self.playlist_ui.pointer else {
            return false;
        };
        self.playlist_ui.pointer = PlaylistPointer::Idle;
        if let Some(index) = self.playlist_ui.pending_double_click.take() {
            if !moved {
                self.activate_playlist_entry(index);
            }
        }
        true
    }

    pub(crate) fn playlist_release(&mut self, x: i32, y: i32) -> PanelAction {
        let menu = self.playlist_ui.menu.kind();
        let item = self.playlist_menu_item_at(x, y);
        let activated = item == self.playlist_ui.menu.hover();
        self.playlist_ui.menu.close();
        if activated {
            if let (Some(menu), Some(item)) = (menu, item) {
                self.activate_playlist_menu_item(menu, item)
            } else {
                PanelAction::Changed
            }
        } else {
            PanelAction::None
        }
    }

    fn activate_playlist_menu_item(&mut self, menu: PlaylistMenuKind, item: usize) -> PanelAction {
        let Some(command) = PlaylistMenuCommand::from_menu_item(menu, item) else {
            return PanelAction::None;
        };
        let changed = match command {
            PlaylistMenuCommand::OpenLocationWindow => return PanelAction::OpenLocationWindow,
            PlaylistMenuCommand::OpenDirectoryDialog => return PanelAction::OpenDirectoryDialog,
            PlaylistMenuCommand::OpenFileDialog => return PanelAction::OpenFileDialog,
            PlaylistMenuCommand::ShowSortMenu => return PanelAction::ShowPlaylistSortMenu,
            PlaylistMenuCommand::ShowFileInfo => return PanelAction::ShowFileInfo,
            PlaylistMenuCommand::OpenOptions => {
                self.playlist_options_opened = true;
                true
            }
            PlaylistMenuCommand::ClearList => !self
                .dispatch_store_command_and_apply_local_effects(PlaylistCommand::Clear)
                .changes
                .is_empty(),
            PlaylistMenuCommand::CropToSelection => !self
                .dispatch_store_command_and_apply_local_effects(PlaylistCommand::CropToSelection)
                .changes
                .is_empty(),
            PlaylistMenuCommand::RemoveSelectedOrCurrent => !self
                .dispatch_store_command_and_apply_local_effects(
                    PlaylistCommand::RemoveSelectedOrCurrent,
                )
                .changes
                .is_empty(),
            PlaylistMenuCommand::InvertSelection => !self
                .dispatch_store_command_and_apply_local_effects(PlaylistCommand::InvertSelection)
                .changes
                .is_empty(),
            PlaylistMenuCommand::SelectNone => !self
                .dispatch_store_command_and_apply_local_effects(PlaylistCommand::SelectNone)
                .changes
                .is_empty(),
            PlaylistMenuCommand::SelectAll => !self
                .dispatch_store_command_and_apply_local_effects(PlaylistCommand::SelectAll)
                .changes
                .is_empty(),
            PlaylistMenuCommand::SavePlaylist => return PanelAction::OpenPlaylistSaveDialog,
            PlaylistMenuCommand::LoadPlaylist => return PanelAction::OpenPlaylistLoadDialog,
        };
        if changed {
            self.clamp_playlist_scroll_offset();
            PanelAction::Changed
        } else {
            PanelAction::None
        }
    }

    pub(crate) fn activate_playlist_context_action(
        &mut self,
        action: PlaylistContextAction,
    ) -> bool {
        let command = match action {
            PlaylistContextAction::RemoveSelected => PlaylistCommand::RemoveSelectedOrCurrent,
            PlaylistContextAction::RemoveDead => PlaylistCommand::RemoveDead,
            PlaylistContextAction::PhysicallyDelete => PlaylistCommand::PhysicallyDeleteSelected,
            PlaylistContextAction::SelectAll => PlaylistCommand::SelectAll,
            PlaylistContextAction::SelectNone => PlaylistCommand::SelectNone,
            PlaylistContextAction::InvertSelection => PlaylistCommand::InvertSelection,
        };
        let changed = !self
            .dispatch_store_command_and_apply_local_effects(command)
            .changes
            .is_empty();
        if changed {
            self.clamp_playlist_scroll_offset();
        }
        changed
    }

    pub(crate) fn activate_playlist_sort_action(&mut self, action: PlaylistSortAction) -> bool {
        self.dispatch_store_command_and_apply_local_effects(action.command());
        self.clamp_playlist_scroll_offset();
        true
    }

    fn update_playlist_search_match(&mut self) {
        let query = self.playlist_ui.search.query();
        if query.is_empty() {
            return;
        }
        let total = self.store.state().playlist.len();
        if total == 0 {
            return;
        }
        let query = query.to_lowercase();
        let start = self
            .selected_playlist_index()
            .or_else(|| self.store.state().playlist.position())
            .unwrap_or(0)
            .min(total);

        let matching_index = (start..total).chain(0..start).find(|index| {
            let Some(entry) = self.store.state().playlist.entries().get(*index) else {
                return false;
            };
            let text = if entry.title.is_empty() {
                &entry.filename
            } else {
                &entry.title
            };
            text.to_lowercase().contains(&query)
        });
        if let Some(index) = matching_index {
            self.select_single_playlist_entry(index);
            self.scroll_playlist_entry_into_view(index);
        }
    }

    fn selected_playlist_index(&self) -> Option<usize> {
        self.store
            .state()
            .playlist
            .entries()
            .iter()
            .position(|entry| entry.selected)
    }

    pub(crate) fn move_playlist_selection(&mut self, delta: isize) -> bool {
        if !self.store.state().config.vim_playlist_navigation {
            return false;
        }
        self.move_playlist_selection_by(delta)
    }

    pub(crate) fn move_playlist_arrow_selection(&mut self, delta: isize) -> bool {
        self.move_playlist_selection_by(delta)
    }

    fn move_playlist_selection_by(&mut self, delta: isize) -> bool {
        let len = self.store.state().playlist.len();
        if len == 0 {
            return false;
        }
        let current = self
            .selected_playlist_index()
            .or_else(|| self.store.state().playlist.position())
            .unwrap_or(if delta < 0 { len - 1 } else { 0 });
        let next = current.saturating_add_signed(delta).min(len - 1);
        self.select_single_playlist_entry(next);
        self.scroll_playlist_entry_into_view(next);
        true
    }

    pub(crate) fn move_playlist_page(&mut self, direction: isize) -> bool {
        let visible = self.playlist_visible_entries().max(1) as isize;
        self.move_playlist_selection_by(direction.signum() * visible)
    }

    pub(crate) fn move_playlist_to_start(&mut self) -> bool {
        self.select_first_playlist_entry()
    }

    pub(crate) fn move_playlist_to_end(&mut self) -> bool {
        let Some(last) = self.store.state().playlist.len().checked_sub(1) else {
            return false;
        };
        self.select_single_playlist_entry(last);
        self.scroll_playlist_entry_into_view(last);
        true
    }

    pub(crate) fn crop_playlist_to_selected_or_current(&mut self) -> bool {
        !self
            .dispatch_store_command_and_apply_local_effects(PlaylistCommand::CropToSelection)
            .changes
            .is_empty()
    }

    pub(crate) fn toggle_queue_selected_playlist_entries(&mut self) -> bool {
        let targets = playlist_queue_target_indices(&self.store.state().playlist);
        if targets.is_empty() {
            return false;
        }
        !self
            .dispatch_store_command_and_apply_local_effects(PlaylistCommand::ToggleQueue(targets))
            .changes
            .is_empty()
    }

    pub(crate) fn clear_playlist_queue(&mut self) -> bool {
        !self
            .dispatch_store_command_and_apply_local_effects(PlaylistCommand::ClearQueue)
            .changes
            .is_empty()
    }

    pub(crate) fn open_queue_manager(&mut self) -> bool {
        self.queue_manager_opened = true;
        true
    }

    pub(crate) fn play_selected_playlist_entry(&mut self) -> bool {
        if !self.store.state().config.vim_playlist_navigation {
            return false;
        }
        self.activate_selected_or_current_playlist_entry()
    }

    pub(crate) fn activate_selected_or_current_playlist_entry(&mut self) -> bool {
        let Some(index) = self
            .selected_playlist_index()
            .or_else(|| self.store.state().playlist.position())
            .or_else(|| (!self.store.state().playlist.is_empty()).then_some(0))
        else {
            return false;
        };
        self.activate_playlist_entry(index);
        true
    }

    fn select_single_playlist_entry(&mut self, index: usize) {
        for command in playlist_row_click_commands(index, false, false) {
            self.dispatch_store_command_and_apply_local_effects(command);
        }
    }

    fn scroll_playlist_entry_into_view(&mut self, index: usize) {
        let visible = self.playlist_visible_entries();
        if visible == 0 {
            return;
        }
        if index < self.playlist_ui.scroll_offset {
            self.playlist_ui.scroll_offset = index;
        } else if index >= self.playlist_ui.scroll_offset + visible {
            self.playlist_ui.scroll_offset = index + 1 - visible;
        }
        self.clamp_playlist_scroll_offset();
    }

    fn playlist_visible_entries(&self) -> usize {
        ((self.playlist_ui.height - 58).max(0) / 11) as usize
    }

    fn playlist_entry_at(&self, x: i32, y: i32) -> Option<usize> {
        if self.is_playlist_shaded() || !(12..self.playlist_ui.width - 19).contains(&x) {
            return None;
        }
        if !(20..self.playlist_ui.height - 38).contains(&y) {
            return None;
        }
        let row = ((y - 20) / 11) as usize;
        if row >= self.playlist_visible_entries() {
            return None;
        }
        let index = self.playlist_ui.scroll_offset + row;
        (index < self.store.state().playlist.len()).then_some(index)
    }

    fn playlist_max_scroll(&self) -> usize {
        self.store
            .state()
            .playlist
            .len()
            .saturating_sub(self.playlist_visible_entries())
    }

    fn clamp_playlist_scroll_offset(&mut self) {
        self.playlist_ui.scroll_offset = self
            .playlist_ui
            .scroll_offset
            .min(self.playlist_max_scroll());
    }

    fn playlist_scrollbar_region(&self, x: i32, y: i32) -> bool {
        !self.is_playlist_shaded()
            && x >= self.playlist_ui.width - 15
            && x < self.playlist_ui.width - 7
            && y >= 20
            && y < self.playlist_ui.height - 38
    }

    fn playlist_scrollbar_geometry(&self) -> Option<(i32, i32)> {
        let visible = self.playlist_visible_entries();
        let total = self.store.state().playlist.len();
        if total <= visible || visible == 0 {
            return None;
        }
        let list_h = self.playlist_ui.height - 58;
        let thumb_h = 18;
        let max_scroll = total - visible;
        let max_thumb_pos = (list_h - thumb_h).max(0);
        let thumb_y = 20
            + ((self.playlist_ui.scroll_offset.min(max_scroll) as i32 * max_thumb_pos)
                / max_scroll.max(1) as i32);
        Some((thumb_y, thumb_h))
    }

    fn update_playlist_scroll_from_thumb_y(&mut self, thumb_y: i32) {
        let visible = self.playlist_visible_entries();
        let total = self.store.state().playlist.len();
        if total <= visible || visible == 0 {
            self.playlist_ui.scroll_offset = 0;
            return;
        }
        let list_h = self.playlist_ui.height - 58;
        let thumb_h = 18;
        let max_scroll = total - visible;
        let max_thumb_pos = (list_h - thumb_h).max(0);
        if max_thumb_pos <= 0 {
            self.playlist_ui.scroll_offset = 0;
            return;
        }
        let thumb_pos = (thumb_y - 20).clamp(0, max_thumb_pos);
        self.playlist_ui.scroll_offset = ((thumb_pos as usize * max_scroll)
            + (max_thumb_pos as usize / 2))
            / max_thumb_pos as usize;
    }

    fn playlist_menu_item_at(&self, x: i32, y: i32) -> Option<usize> {
        let menu = self.playlist_ui.menu.kind()?;
        let (menu_x, menu_y, menu_width, menu_height) =
            playlist_menu_rect(menu, self.playlist_ui.width, self.playlist_ui.height);
        if x < menu_x || x >= menu_x + menu_width || y < menu_y || y >= menu_y + menu_height {
            return None;
        }
        Some(((y - menu_y) / 18) as usize)
    }

    pub(crate) fn panel_click(&mut self, kind: PanelKind, x: i32, y: i32) -> PanelAction {
        if kind == PanelKind::Playlist {
            self.playlist_ui.menu.close();
            if matches!(
                self.playlist_ui.pointer,
                PlaylistPointer::DraggingEntry { .. }
            ) {
                self.playlist_ui.pointer = PlaylistPointer::Idle;
            }
        }

        if self.panel_title_button_hit(kind, x, y) {
            if self.panel_close_button_hit(kind, x) {
                match kind {
                    PanelKind::Equalizer => {
                        self.dispatch_store_command_and_apply_local_effects(
                            PanelCommand::SetEqualizerVisibility(false),
                        );
                    }
                    PanelKind::Playlist => {
                        self.dispatch_store_command_and_apply_local_effects(
                            PanelCommand::SetPlaylistVisibility(false),
                        );
                    }
                }
                return PanelAction::Changed;
            }

            if self.panel_shade_button_hit(kind, x) {
                self.toggle_panel_shaded(kind);
                return PanelAction::Changed;
            }
        }

        if kind == PanelKind::Playlist && !self.is_playlist_shaded() {
            if let Some(menu) =
                playlist_menu_at(x, y, self.playlist_ui.width, self.playlist_ui.height)
            {
                let menu_name = format!("{menu:?}");
                app_log_info!(playlist, "menu opened", menu_name);
                self.playlist_ui.menu.open(menu);
                return PanelAction::ShowPlaylistMenu(menu);
            }
            if let Some(button) =
                playlist_footer_button_at(x, y, self.playlist_ui.width, self.playlist_ui.height)
            {
                return self.activate_playlist_footer_button(button);
            }
        }

        PanelAction::None
    }

    fn activate_playlist_footer_button(&mut self, button: PlaylistFooterButton) -> PanelAction {
        let button_name = format!("{button:?}");
        app_log_info!(playlist, "footer button", button_name);
        match button {
            PlaylistFooterButton::Previous => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::PreviousTrack);
                PanelAction::Changed
            }
            PlaylistFooterButton::Play => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::Play);
                PanelAction::Changed
            }
            PlaylistFooterButton::Pause => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::Pause);
                PanelAction::Changed
            }
            PlaylistFooterButton::Stop => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::Halt);
                PanelAction::Changed
            }
            PlaylistFooterButton::Next => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::NextTrack);
                PanelAction::Changed
            }
            PlaylistFooterButton::Eject => PanelAction::OpenFileDialog,
            PlaylistFooterButton::ScrollUp => {
                self.scroll_playlist_rows(-1);
                PanelAction::Changed
            }
            PlaylistFooterButton::ScrollDown => {
                self.scroll_playlist_rows(1);
                PanelAction::Changed
            }
        }
    }

    fn panel_title_button_hit(&self, kind: PanelKind, x: i32, y: i32) -> bool {
        panel_title_button_at(panel_layout_kind(kind), x, y, self.playlist_ui.width).is_some()
    }

    fn panel_shade_button_hit(&self, kind: PanelKind, x: i32) -> bool {
        panel_title_button_at(panel_layout_kind(kind), x, 7, self.playlist_ui.width)
            == Some(PanelTitleButton::Shade)
    }

    fn panel_close_button_hit(&self, kind: PanelKind, x: i32) -> bool {
        panel_title_button_at(panel_layout_kind(kind), x, 7, self.playlist_ui.width)
            == Some(PanelTitleButton::Close)
    }

    pub(crate) fn player_state(&self) -> PlayerState {
        self.store.state().player.state()
    }

    pub(crate) fn shuffle(&self) -> bool {
        self.store.state().playlist.shuffle()
    }

    pub(crate) fn repeat(&self) -> bool {
        self.store.state().playlist.repeat()
    }

    pub(crate) fn no_advance(&self) -> bool {
        self.store.state().playlist.no_advance()
    }

    pub(crate) fn set_no_advance(&mut self, enabled: bool) {
        if self.store.state().playlist.no_advance() != enabled {
            self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::ToggleNoAdvance);
        }
    }

    pub(crate) fn toggle_shaded(&mut self) {
        self.dispatch_store_command_and_apply_local_effects(PanelCommand::ToggleMainShade);
    }

    pub(crate) fn toggle_selected_window_shade(&mut self) -> Option<PanelKind> {
        match self.selected_docked_panel() {
            Some(kind) => {
                self.toggle_panel_shaded(kind);
                Some(kind)
            }
            None => {
                self.toggle_shaded();
                None
            }
        }
    }

    pub(crate) fn toggle_playlist_shaded(&mut self) {
        self.toggle_panel_shaded(PanelKind::Playlist);
    }

    pub(crate) fn toggle_equalizer_shaded(&mut self) {
        self.toggle_panel_shaded(PanelKind::Equalizer);
    }

    fn toggle_panel_shaded(&mut self, kind: PanelKind) {
        match kind {
            PanelKind::Equalizer => {
                self.dispatch_store_command_and_apply_local_effects(
                    PanelCommand::ToggleEqualizerShade,
                );
            }
            PanelKind::Playlist => {
                self.dispatch_store_command_and_apply_local_effects(
                    PanelCommand::TogglePlaylistShade,
                );
            }
        }
    }

    pub(crate) fn volume(&self) -> i32 {
        self.store.state().player.volume()
    }

    pub(crate) fn balance(&self) -> i32 {
        self.store.state().player.balance()
    }

    pub(crate) fn position(&self) -> i32 {
        self.position_slider_position()
    }

    pub(crate) fn main_time_digits(&self) -> [i32; 5] {
        self.time_digits()
    }

    pub(crate) fn shaded_main_time_text(&self) -> (String, String) {
        self.shaded_time_parts()
    }

    pub(crate) fn shaded_main_position_visible(&self) -> bool {
        self.shaded_position_slider_visible()
    }

    pub(crate) fn shaded_main_position(&self) -> i32 {
        self.shaded_position_slider_position()
    }

    pub(crate) fn main_channels(&self) -> i32 {
        self.render_state().channels
    }

    pub(crate) fn set_preference_output_device(&mut self, device: Option<String>) {
        if let Some(backend) = &self.playback_backend {
            let selection = device
                .as_deref()
                .map(OutputDeviceSelection::System)
                .unwrap_or(OutputDeviceSelection::Automatic);
            let mut backend = backend.borrow_mut();
            if let Err(err) = backend.select_output_device(selection) {
                eprintln!("xmms-rs: failed to switch output device: {err}");
            }
            self.output_device_groups = backend.output_device_groups();
        }
        self.sync_equalizer_to_backend();
        self.update_config_via_store(|config| config.output_device = device);
    }

    pub(crate) fn preference_output_device(&self) -> Option<&str> {
        self.store.state().config.output_device.as_deref()
    }

    pub(crate) fn set_preference_volume(&mut self, volume: i32) {
        let volume = volume.clamp(0, 100);
        self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetVolume(volume));
    }

    pub(crate) fn set_preference_balance(&mut self, balance: i32) {
        let balance = balance.clamp(-100, 100);
        self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetBalance(balance));
    }

    pub(crate) fn set_preference_scale_factor(&mut self, scale: f64) {
        self.update_config_via_store(|config| set_config_scale_factor(config, scale));
    }

    pub(crate) fn set_preference_repeat(&mut self, enabled: bool) {
        if self.store.state().playlist.repeat() != enabled {
            self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::ToggleRepeat);
        }
    }

    pub(crate) fn set_preference_shuffle(&mut self, enabled: bool) {
        if self.store.state().playlist.shuffle() != enabled {
            self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::ToggleShuffle);
        }
    }

    pub(crate) fn set_preference_no_playlist_advance(&mut self, enabled: bool) {
        if self.store.state().playlist.no_advance() != enabled {
            self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::ToggleNoAdvance);
        }
    }

    pub(crate) fn preference_no_playlist_advance(&self) -> bool {
        self.store.state().playlist.no_advance()
    }

    pub(crate) fn set_preference_pause_between_songs(&mut self, enabled: bool) {
        self.update_config_via_store(|config| config.pause_between_songs = enabled);
        if !enabled {
            self.playback_transition = PlaybackTransitionState::stop_playback();
        }
    }

    pub(crate) fn preference_pause_between_songs(&self) -> bool {
        self.store.state().config.pause_between_songs
    }

    pub(crate) fn set_preference_stop_with_fadeout(&mut self, enabled: bool) {
        self.update_config_via_store(|config| config.stop_with_fadeout = enabled);
    }

    pub(crate) fn preference_stop_with_fadeout(&self) -> bool {
        self.store.state().config.stop_with_fadeout
    }

    pub(crate) fn set_preference_pause_between_songs_time(&mut self, seconds: i32) {
        self.update_config_via_store(|config| {
            config.pause_between_songs_time = seconds.clamp(0, 1000);
        });
    }

    pub(crate) fn preference_pause_between_songs_time(&self) -> i32 {
        self.store.state().config.pause_between_songs_time
    }

    pub(crate) fn set_preference_mouse_wheel_change(&mut self, percent: i32) {
        self.update_config_via_store(|config| {
            config.mouse_wheel_change = percent.clamp(1, 100);
        });
    }

    pub(crate) fn preference_mouse_wheel_change(&self) -> i32 {
        self.store.state().config.mouse_wheel_change
    }

    pub(crate) fn set_preference_timer_remaining(&mut self, enabled: bool) {
        self.update_config_via_store(|config| {
            config.timer_mode = if enabled {
                TimerMode::Remaining
            } else {
                TimerMode::Elapsed
            };
        });
    }

    pub(crate) fn preference_timer_remaining(&self) -> bool {
        self.store.state().config.timer_mode == TimerMode::Remaining
    }

    pub(crate) fn set_preference_playlist_docked(&mut self, docked: bool) {
        self.dispatch_store_command_and_apply_local_effects(PanelCommand::SetPlaylistDetached(
            !docked,
        ));
        if docked {
            self.playlist_ui.width = PLAYLIST_MIN_WIDTH;
            self.clamp_playlist_scroll_offset();
        }
    }

    pub(crate) fn set_preference_equalizer_docked(&mut self, docked: bool) {
        self.dispatch_store_command_and_apply_local_effects(PanelCommand::SetEqualizerDetached(
            !docked,
        ));
    }

    pub(crate) fn set_preference_convert_underscore(&mut self, enabled: bool) {
        self.update_config_via_store(|config| config.convert_underscore = enabled);
    }

    pub(crate) fn preference_convert_underscore(&self) -> bool {
        self.store.state().config.convert_underscore
    }

    pub(crate) fn set_preference_convert_twenty(&mut self, enabled: bool) {
        self.update_config_via_store(|config| config.convert_twenty = enabled);
    }

    pub(crate) fn preference_convert_twenty(&self) -> bool {
        self.store.state().config.convert_twenty
    }

    pub(crate) fn set_preference_show_numbers_in_playlist(&mut self, enabled: bool) {
        self.update_config_via_store(|config| config.show_numbers_in_pl = enabled);
    }

    pub(crate) fn preference_show_numbers_in_playlist(&self) -> bool {
        self.store.state().config.show_numbers_in_pl
    }

    pub(crate) fn set_preference_vim_playlist_navigation(&mut self, enabled: bool) {
        self.update_config_via_store(|config| config.vim_playlist_navigation = enabled);
    }

    pub(crate) fn preference_vim_playlist_navigation(&self) -> bool {
        self.store.state().config.vim_playlist_navigation
    }

    pub(crate) fn set_preference_playlist_font(&mut self, font: &str) {
        self.set_preference_playlist_font_size(playlist_font_size_from_descriptor(font));
    }

    pub(crate) fn preference_playlist_font(&self) -> &str {
        &self.store.state().config.playlist_font
    }

    pub(crate) fn set_preference_playlist_font_size(&mut self, size: f64) {
        self.update_config_via_store(|config| {
            config.playlist_font = playlist_font_descriptor_for_size(size);
        });
    }

    pub(crate) fn preference_playlist_font_size(&self) -> f64 {
        playlist_font_size_from_descriptor(&self.store.state().config.playlist_font)
    }

    pub(crate) fn set_preference_title_format(&mut self, format: &str) {
        self.update_config_via_store(|config| {
            config.title_format = normalize_title_format(format);
        });
    }

    pub(crate) fn preference_title_format(&self) -> &str {
        &self.store.state().config.title_format
    }

    pub(crate) fn set_visualization_mode(&mut self, mode: VisMode) {
        self.update_config_via_store(|config| config.vis_mode = mode);
        self.apply_visualization_preferences();
    }

    pub(crate) fn visualization_mode(&self) -> VisMode {
        self.visualization.mode()
    }

    pub(crate) fn set_visualization_analyzer_style(&mut self, style: VisAnalyzerStyle) {
        self.update_config_via_store(|config| config.vis_analyzer_style = style);
        self.apply_visualization_preferences();
    }

    pub(crate) fn visualization_analyzer_style(&self) -> VisAnalyzerStyle {
        self.visualization.analyzer_style()
    }

    pub(crate) fn set_visualization_analyzer_mode(&mut self, mode: VisAnalyzerMode) {
        self.update_config_via_store(|config| config.vis_analyzer_mode = mode);
        self.apply_visualization_preferences();
    }

    pub(crate) fn visualization_analyzer_mode(&self) -> VisAnalyzerMode {
        self.visualization.analyzer_mode()
    }

    pub(crate) fn set_visualization_scope_mode(&mut self, mode: VisScopeMode) {
        self.update_config_via_store(|config| config.vis_scope_mode = mode);
        self.apply_visualization_preferences();
    }

    pub(crate) fn visualization_scope_mode(&self) -> VisScopeMode {
        self.visualization.scope_mode()
    }

    pub(crate) fn set_visualization_peaks_enabled(&mut self, enabled: bool) {
        self.update_config_via_store(|config| config.vis_peaks_enabled = enabled);
        self.apply_visualization_preferences();
    }

    pub(crate) fn visualization_peaks_enabled(&self) -> bool {
        self.visualization.peaks_enabled()
    }

    pub(crate) fn visualization_analyzer_falloff(&self) -> VisFalloffSpeed {
        self.store.state().config.vis_analyzer_falloff
    }

    pub(crate) fn visualization_peaks_falloff(&self) -> VisFalloffSpeed {
        self.store.state().config.vis_peaks_falloff
    }

    pub(crate) fn set_visualization_falloff(
        &mut self,
        analyzer: VisFalloffSpeed,
        peaks: VisFalloffSpeed,
    ) {
        self.update_config_via_store(|config| {
            config.vis_analyzer_falloff = analyzer;
            config.vis_peaks_falloff = peaks;
        });
        self.apply_visualization_preferences();
    }

    pub(crate) fn set_visualization_vu_mode(&mut self, mode: VisVuMode) {
        self.update_config_via_store(|config| config.vis_vu_mode = mode);
    }

    pub(crate) fn visualization_vu_mode(&self) -> VisVuMode {
        self.store.state().config.vis_vu_mode
    }

    pub(crate) fn set_visualization_refresh_divisor(&mut self, divisor: i32) {
        self.update_config_via_store(|config| {
            config.vis_refresh_divisor = divisor.clamp(1, 8);
        });
    }

    pub(crate) fn visualization_refresh_divisor(&self) -> i32 {
        self.store.state().config.vis_refresh_divisor.clamp(1, 8)
    }

    pub(crate) fn visualization_render_state(&self) -> VisualizationRenderState {
        self.make_visualization_render_state()
    }

    fn apply_visualization_preferences(&mut self) {
        let config = self.store.state().config.clone();
        self.visualization.set_mode(config.vis_mode);
        self.visualization
            .set_analyzer_mode(config.vis_analyzer_mode);
        self.visualization
            .set_analyzer_style(config.vis_analyzer_style);
        self.visualization.set_scope_mode(config.vis_scope_mode);
        self.visualization
            .set_peaks_enabled(config.vis_peaks_enabled);
        self.visualization
            .set_falloff(config.vis_analyzer_falloff, config.vis_peaks_falloff);
    }

    #[cfg(test)]
    fn set_playback_position_ms(&mut self, position_ms: i64) {
        self.ensure_current_playlist_position_for_seek();
        let result = self
            .store
            .update_playback_position_from_runtime(position_ms);
        for effect in result.effects {
            self.apply_store_effect(effect);
        }
        let position_ms = self.store.state().config.playback_position_ms;
        if self.store.state().player.state() == PlayerState::Stopped {
            self.playback_transition = PlaybackTransitionState::stopped_at_or_idle(position_ms);
            return;
        }
        if self.playback_transition.pending_backend_seek_ms().is_some() {
            self.playback_transition = PlaybackTransitionState::request_backend_seek(position_ms);
            return;
        }
        self.playback_transition = PlaybackTransitionState::request_backend_seek(position_ms);
        if let Some(backend) = &self.playback_backend {
            match backend.borrow().seek(position_ms) {
                Ok(()) => {
                    self.playback_transition =
                        PlaybackTransitionState::await_backend_seek(position_ms);
                }
                Err(err) => {
                    eprintln!("xmms-rs: failed to seek playback: {err}");
                }
            }
        }
    }

    pub(crate) fn update_timer_tick(&mut self, elapsed_ms: u32) -> bool {
        self.update_timer_tick_targets(elapsed_ms).any()
    }

    fn update_timer_tick_targets(&mut self, elapsed_ms: u32) -> GtkTickRedraw {
        let mut redraw = self.poll_duration_index_results();
        redraw.merge(self.poll_playback_backend());
        let fading = self.update_stop_fade(elapsed_ms);
        let eof_waiting = self.update_pending_eof_advance(elapsed_ms);
        let title = self
            .equalizer_drag_info_text()
            .unwrap_or_else(|| self.formatted_current_title());
        let marquee_changed = self.title_marquee.update(
            &title,
            crate::render::MAIN_TITLE_TEXT_WIDTH,
            self.store.state().player.state(),
            !self.is_shaded(),
            Duration::from_millis(u64::from(elapsed_ms)),
        );
        redraw.main |= fading || marquee_changed;
        if eof_waiting {
            redraw.main = true;
            redraw.playlist = true;
        }
        if self.store.state().player.state() != PlayerState::Playing {
            self.visualization_tick_counter = 0;
            self.update_playlist_footer_redraw(&mut redraw);
            return redraw;
        }

        if self.playback_backend.is_none() {
            let pending_visualization_data = self
                .store
                .state()
                .player
                .visualization_data_valid()
                .then(|| *self.store.state().player.visualization_data());
            let result = self.store.tick_playback_position(i64::from(elapsed_ms));
            redraw.merge(GtkTickRedraw::from_changes(result.changes));
            for effect in result.effects {
                self.apply_store_effect(effect);
            }
            if let Some(data) = pending_visualization_data {
                let result = self
                    .store
                    .handle_playback_event(PlaybackEvent::Spectrum(data));
                redraw.merge(GtkTickRedraw::from_changes(result.changes));
                for effect in result.effects {
                    self.apply_store_effect(effect);
                }
            }
        }
        self.visualization_tick_counter += 1;
        if self.visualization_tick_counter >= self.visualization_refresh_divisor() {
            self.visualization_tick_counter = 0;
            let data = self
                .store
                .state()
                .player
                .visualization_data_valid()
                .then(|| *self.store.state().player.visualization_data());
            self.visualization.tick_with_steps(
                data.as_ref().map(|data| data.as_slice()),
                self.visualization_refresh_divisor() as usize,
            );
        }
        redraw.main = true;
        self.update_playlist_footer_redraw(&mut redraw);
        redraw
    }

    fn runtime_tick_interval(&self) -> Duration {
        if self.playback_transition.fadeout().is_some()
            || self.playback_transition.eof_pause_remaining_ms().is_some()
            || self.playback_transition.pending_backend_seek_ms().is_some()
            || self
                .playback_transition
                .awaiting_backend_seek_ms()
                .is_some()
        {
            return GTK_TRANSITION_TICK;
        }

        match self.store.state().player.state() {
            PlayerState::Playing if self.visualization.mode() != VisMode::Off => {
                GTK_TRANSITION_TICK
            }
            PlayerState::Playing
                if self
                    .title_marquee
                    .is_scrolling(self.store.state().player.state(), !self.is_shaded()) =>
            {
                GTK_MARQUEE_TICK
            }
            PlayerState::Playing => GTK_PLAYBACK_TICK,
            PlayerState::Paused => GTK_PAUSED_TICK,
            PlayerState::Stopped => GTK_IDLE_TICK,
        }
    }

    fn update_stop_fade(&mut self, elapsed_ms: u32) -> bool {
        let Some((next_transition, volume)) = self.playback_transition.tick_fadeout(elapsed_ms)
        else {
            return false;
        };
        if next_transition
            .fadeout()
            .is_some_and(|(remaining_ms, _)| remaining_ms == 0)
        {
            let restore_volume = next_transition
                .fadeout()
                .map(|(_, start_volume)| start_volume)
                .unwrap_or_default();
            let result = self.store.complete_stop_fade(restore_volume);
            for effect in result.effects {
                self.apply_store_effect(effect);
            }
            self.playback_transition = PlaybackTransitionState::stop_playback();
            self.visualization.clear_data();
            return true;
        }
        self.playback_transition = next_transition;
        self.set_runtime_volume(volume);
        true
    }

    fn update_pending_eof_advance(&mut self, elapsed_ms: u32) -> bool {
        let Some((next_transition, should_advance)) =
            self.playback_transition.tick_eof_pause(elapsed_ms)
        else {
            return false;
        };
        self.playback_transition = next_transition;
        if !should_advance {
            return true;
        }
        self.advance_playlist_after_eof();
        true
    }

    fn poll_playback_backend(&mut self) -> GtkTickRedraw {
        let mut redraw = GtkTickRedraw::default();
        let Some(backend) = self.playback_backend.as_ref().map(Rc::clone) else {
            return redraw;
        };
        let spectrum_layout = if self.visualization.mode() == VisMode::Analyzer
            && self.visualization.analyzer_style() == VisAnalyzerStyle::Bars
        {
            SpectrumLayout::AnalyzerBars
        } else {
            SpectrumLayout::Lines
        };
        let mut applied_pending_seek = false;
        let backend_ref = backend.borrow();
        backend_ref.set_spectrum_layout(spectrum_layout);
        let events = backend_ref.poll_events();
        drop(backend_ref);
        match events {
            Ok(events) => {
                let mut end_of_stream = false;
                let mut backend_ready = false;
                for event in events {
                    if matches!(event, PlaybackEvent::EndOfStream) {
                        end_of_stream = true;
                    }
                    if matches!(
                        event,
                        PlaybackEvent::AsyncDone | PlaybackEvent::DurationChanged(_)
                    ) {
                        backend_ready = true;
                    }
                    let result = self.store.handle_playback_event(event);
                    redraw.merge(GtkTickRedraw::from_changes(result.changes));
                    for effect in result.effects {
                        self.apply_store_effect(effect);
                    }
                }
                if backend_ready {
                    applied_pending_seek |= self.apply_pending_backend_seek(&backend, false);
                }
                if end_of_stream {
                    self.playlist_eof_reached();
                    redraw.main = true;
                    redraw.playlist = true;
                }
            }
            Err(err) => eprintln!("xmms-rs: failed to poll playback backend: {err}"),
        }
        let (stream_info, duration_ms) = {
            let backend = backend.borrow();
            (backend.stream_info(), backend.duration_ms())
        };
        let result = self
            .store
            .handle_playback_event(PlaybackEvent::StreamInfo(stream_info));
        redraw.merge(GtkTickRedraw::from_changes(result.changes));
        for effect in result.effects {
            self.apply_store_effect(effect);
        }
        if let Some(duration_ms) = duration_ms {
            let result =
                self.store
                    .handle_playback_event(crate::player::PlaybackEvent::DurationChanged(Some(
                        duration_ms,
                    )));
            redraw.merge(GtkTickRedraw::from_changes(result.changes));
            for effect in result.effects {
                self.apply_store_effect(effect);
            }
            applied_pending_seek |= self.apply_pending_backend_seek(&backend, true);
        }
        let position_ms = { backend.borrow().position_ms() };
        if let Some(position_ms) = position_ms {
            if let Some(target_ms) = self.playback_transition.awaiting_backend_seek_ms() {
                if position_ms.saturating_sub(target_ms).abs() <= 250 {
                    self.playback_transition = PlaybackTransitionState::Idle;
                    let result = self
                        .store
                        .update_playback_position_from_runtime(position_ms.max(target_ms));
                    redraw.merge(GtkTickRedraw::from_changes(result.changes));
                    for effect in result.effects {
                        self.apply_store_effect(effect);
                    }
                }
            } else if self.should_sync_backend_position(applied_pending_seek) {
                let result = self
                    .store
                    .update_playback_position_from_runtime(position_ms);
                redraw.merge(GtkTickRedraw::from_changes(result.changes));
                for effect in result.effects {
                    self.apply_store_effect(effect);
                }
            }
        }
        redraw
    }

    fn should_sync_backend_position(&self, applied_pending_seek: bool) -> bool {
        !applied_pending_seek
            && self.playback_transition.pending_backend_seek_ms().is_none()
            && self
                .playback_transition
                .awaiting_backend_seek_ms()
                .is_none()
            && self.playback_transition.eof_pause_remaining_ms().is_none()
    }

    fn apply_pending_backend_seek(
        &mut self,
        backend: &SharedPlaybackBackend,
        log_failure: bool,
    ) -> bool {
        let Some(position_ms) = self.playback_transition.pending_backend_seek_ms() else {
            return false;
        };
        match backend.borrow().seek(position_ms) {
            Ok(()) => {
                app_log_info!(backend, "gtk applied pending start seek", position_ms);
                self.playback_transition = PlaybackTransitionState::await_backend_seek(position_ms);
                true
            }
            Err(err) => {
                if log_failure {
                    eprintln!("xmms-rs: failed to seek playback: {err}");
                    self.playback_transition = PlaybackTransitionState::Idle;
                }
                false
            }
        }
    }

    pub(crate) fn playlist_eof_reached(&mut self) {
        let result = self.store.update_playback_position_from_runtime(0);
        for effect in result.effects {
            self.apply_store_effect(effect);
        }
        if self.store.state().config.pause_between_songs
            && self.store.state().config.pause_between_songs_time > 0
        {
            self.playback_transition = PlaybackTransitionState::wait_between_songs(
                i64::from(self.store.state().config.pause_between_songs_time) * 1_000,
            );
            return;
        }
        self.advance_playlist_after_eof();
    }

    fn advance_playlist_after_eof(&mut self) {
        let result = self.store.handle_playlist_eof();
        for effect in result.effects {
            self.apply_store_effect(effect);
        }
    }

    pub(crate) fn click(&mut self, x: i32, y: i32) -> UiAction {
        self.press(x, y);
        self.release(x, y)
    }

    pub(crate) fn press(&mut self, x: i32, y: i32) {
        let Some(control) = self.hit_test(x, y) else {
            self.main_pointer = MainPointer::Idle;
            return;
        };

        self.main_pointer = if let MainControl::Slider(slider) = control {
            self.main_keyboard_slider = Some(slider);
            let offset = self.begin_slider_drag(slider, x);
            MainPointer::DraggingSlider {
                slider,
                offset,
                position: self.slider_position(slider),
            }
        } else {
            MainPointer::PressedButton {
                control,
                inside: true,
            }
        };
    }

    pub(crate) fn motion(&mut self, x: i32, y: i32) -> bool {
        match self.main_pointer {
            MainPointer::Idle => false,
            MainPointer::PressedButton { control, inside } => {
                let next_inside = self.control_rect(control).contains(x, y);
                let changed = inside != next_inside;
                self.main_pointer = MainPointer::PressedButton {
                    control,
                    inside: next_inside,
                };
                changed
            }
            MainPointer::DraggingSlider {
                slider,
                offset,
                position,
            } => {
                let next_position = (x - self.slider_rect(slider).x - offset)
                    .clamp(self.slider_min(slider), self.slider_max(slider));
                if next_position == position {
                    return false;
                }
                self.main_pointer = MainPointer::DraggingSlider {
                    slider,
                    offset,
                    position: next_position,
                };
                self.set_slider_position(slider, next_position)
            }
        }
    }

    pub(crate) fn release(&mut self, x: i32, y: i32) -> UiAction {
        match std::mem::take(&mut self.main_pointer) {
            MainPointer::Idle => UiAction::None,
            MainPointer::PressedButton { control, inside } => {
                let activated = inside && self.control_rect(control).contains(x, y);
                match control {
                    MainControl::Push(button) if activated => self.activate_push(button),
                    MainControl::Toggle(toggle) if activated => {
                        self.activate_toggle(toggle);
                        UiAction::None
                    }
                    _ => UiAction::None,
                }
            }
            MainPointer::DraggingSlider {
                slider,
                offset,
                position,
            } => {
                let next_position = (x - self.slider_rect(slider).x - offset)
                    .clamp(self.slider_min(slider), self.slider_max(slider));
                if next_position != position {
                    self.set_slider_position(slider, next_position);
                }
                UiAction::None
            }
        }
    }

    pub(crate) fn scroll_main(&mut self, x: i32, y: i32, dy: f64) -> bool {
        if let Some((kind, panel_x, panel_y)) = self.docked_panel_at(x, y) {
            return match kind {
                PanelKind::Equalizer => self.equalizer_scroll(panel_x, panel_y, dy),
                PanelKind::Playlist => self.playlist_scroll(dy),
            };
        }
        if let Some(MainControl::Slider(slider)) = self.hit_test(x, y) {
            return self.scroll_slider(slider, dy);
        }
        self.scroll_volume(dy)
    }

    fn scroll_slider(&mut self, slider: MainSlider, dy: f64) -> bool {
        match slider {
            MainSlider::Volume => self.scroll_volume(dy),
            MainSlider::Balance => self.scroll_balance(dy),
            MainSlider::Position => self.scroll_position_slider(dy),
        }
    }

    pub(crate) fn adjust_main_seek(&mut self, diff: i32) -> bool {
        self.scroll_position_slider(f64::from(diff))
    }

    fn scroll_volume(&mut self, dy: f64) -> bool {
        let step = self.store.state().config.mouse_wheel_change.clamp(1, 100);
        let diff = if dy < 0.0 {
            step
        } else if dy > 0.0 {
            -step
        } else {
            return false;
        };
        self.adjust_volume_by(diff)
    }

    fn adjust_volume_by(&mut self, diff: i32) -> bool {
        let volume = (self.store.state().player.volume() + diff).clamp(0, 100);
        if volume == self.store.state().player.volume() {
            return false;
        }
        self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetVolume(volume));
        true
    }

    fn scroll_balance(&mut self, dy: f64) -> bool {
        let step = self.store.state().config.mouse_wheel_change.clamp(1, 100);
        let diff = if dy < 0.0 {
            step
        } else if dy > 0.0 {
            -step
        } else {
            return false;
        };
        self.adjust_balance_by(diff)
    }

    fn adjust_balance_by(&mut self, diff: i32) -> bool {
        let balance = (self.store.state().player.balance() + diff).clamp(-100, 100);
        if balance == self.store.state().player.balance() {
            return false;
        }
        self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetBalance(balance));
        true
    }

    fn scroll_position_slider(&mut self, dy: f64) -> bool {
        self.ensure_current_playlist_position_for_seek();
        let Some(duration_ms) = self.current_duration_ms().filter(|duration| *duration > 0) else {
            return false;
        };
        let step_ms = (duration_ms / 100).max(1_000);
        let old_position = self.store.state().config.playback_position_ms;
        let position_ms = if dy < 0.0 {
            old_position - step_ms
        } else if dy > 0.0 {
            old_position + step_ms
        } else {
            return false;
        };
        self.dispatch_store_command_and_apply_local_effects(PlayerCommand::SeekToMs(position_ms));
        self.store.state().config.playback_position_ms != old_position
    }

    fn hit_test(&self, x: i32, y: i32) -> Option<MainControl> {
        let mut controls = vec![
            MainControl::Push(MainPushButton::Close),
            MainControl::Push(MainPushButton::Shade),
            MainControl::Push(MainPushButton::Minimize),
            MainControl::Push(MainPushButton::Menu),
        ];
        if !self.is_shaded() {
            controls.extend([
                MainControl::Toggle(MainToggleButton::Playlist),
                MainControl::Toggle(MainToggleButton::Equalizer),
                MainControl::Toggle(MainToggleButton::Repeat),
                MainControl::Toggle(MainToggleButton::Shuffle),
                MainControl::Slider(MainSlider::Position),
                MainControl::Slider(MainSlider::Balance),
                MainControl::Slider(MainSlider::Volume),
                MainControl::Push(MainPushButton::Eject),
                MainControl::Push(MainPushButton::Next),
                MainControl::Push(MainPushButton::Stop),
                MainControl::Push(MainPushButton::Pause),
                MainControl::Push(MainPushButton::Play),
                MainControl::Push(MainPushButton::Previous),
            ]);
        } else {
            controls.extend([
                MainControl::Slider(MainSlider::Position),
                MainControl::Push(MainPushButton::Eject),
                MainControl::Push(MainPushButton::Next),
                MainControl::Push(MainPushButton::Stop),
                MainControl::Push(MainPushButton::Pause),
                MainControl::Push(MainPushButton::Play),
                MainControl::Push(MainPushButton::Previous),
            ]);
        }

        controls
            .into_iter()
            .filter(|control| match control {
                MainControl::Slider(MainSlider::Position) if self.is_shaded() => {
                    self.shaded_position_slider_visible()
                }
                _ => true,
            })
            .find(|control| self.control_rect(*control).contains(x, y))
    }

    pub(crate) fn activate_push(&mut self, button: MainPushButton) -> UiAction {
        match button {
            MainPushButton::Close => UiAction::Quit,
            MainPushButton::Minimize => UiAction::Minimize,
            MainPushButton::Menu => {
                self.dispatch_store_command_and_apply_local_effects(UiCommand::SetMainMenuVisible(
                    true,
                ));
                UiAction::ShowMenu
            }
            MainPushButton::Shade => {
                self.dispatch_store_command_and_apply_local_effects(PanelCommand::ToggleMainShade);
                UiAction::Resize
            }
            MainPushButton::Play => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::Play);
                UiAction::None
            }
            MainPushButton::Pause => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::Pause);
                UiAction::None
            }
            MainPushButton::Stop => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::Halt);
                UiAction::None
            }
            MainPushButton::Previous => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::PreviousTrack);
                UiAction::None
            }
            MainPushButton::Next => {
                self.dispatch_store_command_and_apply_local_effects(PlayerCommand::NextTrack);
                UiAction::None
            }
            MainPushButton::Eject => UiAction::OpenFileDialog,
        }
    }

    pub(crate) fn activate_toggle(&mut self, toggle: MainToggleButton) {
        match toggle {
            MainToggleButton::Shuffle => {
                self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::ToggleShuffle);
            }
            MainToggleButton::Repeat => {
                self.dispatch_store_command_and_apply_local_effects(PlaylistCommand::ToggleRepeat);
            }
            MainToggleButton::Equalizer => {
                self.dispatch_store_command_and_apply_local_effects(
                    PanelCommand::ToggleEqualizerVisibility,
                );
            }
            MainToggleButton::Playlist => {
                self.dispatch_store_command_and_apply_local_effects(
                    PanelCommand::TogglePlaylistVisibility,
                );
            }
        }
    }

    fn begin_slider_drag(&mut self, slider: MainSlider, x: i32) -> i32 {
        let rect = self.slider_rect(slider);
        let knob_width = self.slider_knob_width(slider);
        let position = self.slider_position(slider);
        let knob_x = rect.x + position;
        if x >= knob_x && x < knob_x + knob_width {
            x - knob_x
        } else {
            let offset = knob_width / 2;
            self.set_slider_position(slider, x - rect.x - offset);
            offset
        }
    }

    fn set_slider_position(&mut self, slider: MainSlider, position: i32) -> bool {
        if slider == MainSlider::Position {
            self.ensure_current_playlist_position_for_seek();
        }
        let position = position.clamp(self.slider_min(slider), self.slider_max(slider));
        let old_position = self.slider_position(slider);
        if old_position == position {
            return false;
        }

        let slider_name = format!("{slider:?}");
        app_log_info!(player, "slider changed", slider_name, position);

        match slider {
            MainSlider::Volume => {
                let volume = position_to_volume(position);
                self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetVolume(
                    volume,
                ));
            }
            MainSlider::Balance => {
                let balance = position_to_balance(position);
                self.dispatch_store_command_and_apply_local_effects(AudioCommand::SetBalance(
                    balance,
                ));
            }
            MainSlider::Position => {
                if let Some(duration_ms) =
                    self.current_duration_ms().filter(|duration| *duration > 0)
                {
                    let position_ms = if self.is_shaded() {
                        (duration_ms * i64::from(position - 1)) / 12
                    } else {
                        let position_slider = main_slider_layout(MainSlider::Position, false);
                        (duration_ms * i64::from(position)) / i64::from(position_slider.max)
                    };
                    self.dispatch_store_command_and_apply_local_effects(PlayerCommand::SeekToMs(
                        position_ms,
                    ));
                }
            }
        }
        true
    }

    fn slider_position(&self, slider: MainSlider) -> i32 {
        match slider {
            MainSlider::Volume => volume_to_position(self.store.state().player.volume()),
            MainSlider::Balance => balance_to_position(self.store.state().player.balance()),
            MainSlider::Position if self.is_shaded() => self.shaded_position_slider_position(),
            MainSlider::Position => self.position_slider_position(),
        }
    }

    fn slider_min(&self, slider: MainSlider) -> i32 {
        main_slider_layout(slider, self.is_shaded()).min
    }

    fn slider_max(&self, slider: MainSlider) -> i32 {
        main_slider_layout(slider, self.is_shaded()).max
    }

    fn slider_knob_width(&self, slider: MainSlider) -> i32 {
        main_slider_layout(slider, self.is_shaded()).knob_size.width
    }

    fn pressed_push(&self) -> Option<MainPushButton> {
        match self.main_pointer.pressed_control() {
            Some(MainControl::Push(button)) => Some(button),
            _ => None,
        }
    }

    fn pressed_toggle(&self) -> Option<MainToggleButton> {
        match self.main_pointer.pressed_control() {
            Some(MainControl::Toggle(toggle)) => Some(toggle),
            _ => None,
        }
    }

    fn pressed_slider(&self) -> Option<MainSlider> {
        self.main_pointer.pressed_slider()
    }

    fn control_rect(&self, control: MainControl) -> ControlRect {
        match control {
            MainControl::Push(button) => main_push_button_rect(button, self.is_shaded()),
            MainControl::Toggle(toggle) => main_toggle_button_rect(toggle),
            MainControl::Slider(slider) => self.slider_rect(slider),
        }
    }

    fn slider_rect(&self, slider: MainSlider) -> ControlRect {
        main_slider_layout(slider, self.is_shaded()).rect
    }
}

type ControlRect = SkinRect;

fn panel_layout_kind(kind: PanelKind) -> LayoutPanel {
    match kind {
        PanelKind::Equalizer => LayoutPanel::Equalizer,
        PanelKind::Playlist => LayoutPanel::Playlist,
    }
}

fn event_to_base_coords(
    area: &gtk::DrawingArea,
    state: &MainWindowUiState,
    x: f64,
    y: f64,
) -> (i32, i32) {
    let width = area.allocated_width().max(1) as f64;
    let height = area.allocated_height().max(1) as f64;
    let (base_width, base_height) = state.docked_panel_size();
    scale_event_coords(width, height, base_width, base_height, x, y)
}

fn apply_ui_action(
    action: UiAction,
    app: &gtk::Application,
    window: &gtk::ApplicationWindow,
    drawing_area: &gtk::DrawingArea,
    menu_popover: &gtk::Popover,
    state: &Rc<RefCell<MainWindowUiState>>,
) {
    match action {
        UiAction::None => {}
        UiAction::Quit => app.quit(),
        UiAction::Minimize => window.minimize(),
        UiAction::Resize => {
            let (height, scale) = {
                let state = state.borrow();
                (
                    if state.is_shaded() {
                        MAIN_TITLEBAR_HEIGHT
                    } else {
                        MAIN_WINDOW_HEIGHT
                    },
                    state.scale_factor(),
                )
            };
            drawing_area.set_content_height(scale_dim(height, scale));
            window.set_default_size(
                scale_dim(MAIN_WINDOW_WIDTH, scale),
                scale_dim(height, scale),
            );
        }
        UiAction::ShowMenu => {
            show_main_menu(menu_popover, drawing_area, &state.borrow());
        }
        UiAction::OpenFileDialog => {
            state.borrow_mut().set_file_dialog_visible(true);
            show_open_file_dialog(window, Rc::clone(state));
        }
    }
}

fn show_open_file_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Open Files"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    dialog.set_select_multiple(true);
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_file_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                let uris = files_from_list_model(dialog.files());
                state.accept_opened_uris(uris);
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_open_directory_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Open Directory"),
        Some(parent),
        gtk::FileChooserAction::SelectFolder,
        Some("Open"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_directory_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                let uri = dialog.file().map(|file| file.uri().to_string());
                state.accept_opened_uris(uri);
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_playlist_add_file_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
    playlist_area: gtk::DrawingArea,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Add Files"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    dialog.set_select_multiple(true);
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_file_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                let uris = files_from_list_model(dialog.files());
                state.accept_dropped_uris(uris, false, false);
            }
        }
        playlist_area.queue_draw();
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_playlist_add_directory_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
    playlist_area: gtk::DrawingArea,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Add Directory"),
        Some(parent),
        gtk::FileChooserAction::SelectFolder,
        Some("Open"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_directory_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                let uri = dialog.file().map(|file| file.uri().to_string());
                state.accept_dropped_uris(uri, false, false);
            }
        }
        playlist_area.queue_draw();
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_playlist_load_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
    playlist_area: gtk::DrawingArea,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Load Playlist"),
        Some(parent),
        gtk::FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_playlist_load_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    if let Err(err) = state.load_playlist_file(&path) {
                        eprintln!("xmms-rs: failed to load playlist {}: {err}", path.display());
                    }
                }
            }
        }
        playlist_area.queue_draw();
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn show_playlist_save_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let dialog = gtk::FileChooserNative::new(
        Some("Save Playlist"),
        Some(parent),
        gtk::FileChooserAction::Save,
        Some("Save"),
        Some("Cancel"),
    );
    let dialog_for_response = dialog.clone();
    dialog.connect_response(move |dialog, response| {
        {
            let mut state = main_state.borrow_mut();
            state.set_playlist_save_dialog_visible(false);
            if response == gtk::ResponseType::Accept {
                if let Some(path) = dialog.file().and_then(|file| file.path()) {
                    if let Err(err) = state.save_playlist_file(&path) {
                        eprintln!("xmms-rs: failed to save playlist {}: {err}", path.display());
                    }
                }
            }
        }
        dialog_for_response.destroy();
    });
    dialog.show();
}

fn files_from_list_model(files: gtk::gio::ListModel) -> Vec<String> {
    (0..files.n_items())
        .filter_map(|idx| files.item(idx))
        .filter_map(|object| object.downcast::<gtk::gio::File>().ok())
        .map(|file| file.uri().to_string())
        .collect()
}

fn show_main_menu(
    menu_popover: &gtk::Popover,
    drawing_area: &gtk::DrawingArea,
    state: &MainWindowUiState,
) {
    let (base_width, base_height) = state.docked_panel_size();
    let rect = main_menu_anchor_rect(
        drawing_area.allocated_width(),
        drawing_area.allocated_height(),
        base_width,
        base_height,
    );
    menu_popover.set_position(gtk::PositionType::Bottom);
    menu_popover.set_pointing_to(Some(&rect));
    menu_popover.popup();
}

fn main_menu_anchor_rect(
    allocated_width: i32,
    allocated_height: i32,
    base_width: i32,
    base_height: i32,
) -> gtk::gdk::Rectangle {
    let scale_x = allocated_width.max(1) as f64 / f64::from(base_width.max(1));
    let scale_y = allocated_height.max(1) as f64 / f64::from(base_height.max(1));
    let rect = main_push_button_rect(MainPushButton::Menu, false);
    gtk::gdk::Rectangle::new(
        (f64::from(rect.x) * scale_x) as i32,
        (f64::from(rect.y) * scale_y) as i32,
        (f64::from(rect.width) * scale_x).max(1.0) as i32,
        (f64::from(rect.height) * scale_y).max(1.0) as i32,
    )
}

fn shortcut_matches(key: gtk::gdk::Key, state: gtk::gdk::ModifierType, accelerator: &str) -> bool {
    let Some((shortcut_key, shortcut_mods)) = gtk::accelerator_parse(accelerator) else {
        return false;
    };
    let relevant_mods = state
        & (gtk::gdk::ModifierType::CONTROL_MASK
            | gtk::gdk::ModifierType::SHIFT_MASK
            | gtk::gdk::ModifierType::ALT_MASK);
    key == shortcut_key && relevant_mods == shortcut_mods
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_gtk_for_tests() -> std::sync::MutexGuard<'static, ()> {
        static GTK_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = GTK_TEST_LOCK.lock().unwrap();
        gtk::init().expect("GTK should initialize for widget tests");
        guard
    }

    fn find_named_widget<W: IsA<gtk::Widget> + Clone + 'static>(
        root: &impl IsA<gtk::Widget>,
        name: &str,
    ) -> Option<W> {
        let root = root.as_ref();
        if root.widget_name() == name {
            if let Ok(widget) = root.clone().downcast::<W>() {
                return Some(widget);
            }
        }

        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Some(found) = find_named_widget::<W>(&widget, name) {
                return Some(found);
            }
            child = widget.next_sibling();
        }
        None
    }

    fn collect_label_text(root: &impl IsA<gtk::Widget>, labels: &mut Vec<String>) {
        let root = root.as_ref();
        if let Ok(label) = root.clone().downcast::<gtk::Label>() {
            labels.push(label.text().to_string());
        }

        let mut child = root.first_child();
        while let Some(widget) = child {
            collect_label_text(&widget, labels);
            child = widget.next_sibling();
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        PathBuf::from("target")
            .join("test-artifacts")
            .join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn skin_browser_list_labels(list: &gtk::ListBox) -> Vec<String> {
        let mut labels = Vec::new();
        let mut index = 0;
        while let Some(row) = list.row_at_index(index) {
            let label = row
                .child()
                .and_then(|child| child.downcast::<gtk::Label>().ok())
                .expect("skin browser rows should contain labels");
            labels.push(label.text().to_string());
            index += 1;
        }
        labels
    }

    #[test]
    fn mouse_wheel_uses_configured_volume_step() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            volume: 50,
            mouse_wheel_change: 12,
            ..Config::default()
        }));

        assert!(state.scroll_main(1, 1, -1.0));
        assert_eq!(state.volume(), 62);
        assert!(state.scroll_main(1, 1, 1.0));
        assert_eq!(state.volume(), 50);
    }

    #[test]
    fn mouse_wheel_over_main_sliders_changes_each_slider() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            volume: 50,
            mouse_wheel_change: 12,
            ..Config::default()
        }));
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.store.state_mut().playlist.set_position(0);

        assert!(state.scroll_main(108, 58, -1.0));
        assert_eq!(state.volume(), 62);
        assert!(state.scroll_main(178, 58, -1.0));
        assert_eq!(state.store.state().player.balance(), 12);
        assert!(state.scroll_main(140, 73, 1.0));
        let forward_position = state.playback_position_ms();
        assert!(forward_position > 0);
        assert!(state.scroll_main(140, 73, -1.0));
        assert!(state.playback_position_ms() < forward_position);
    }

    #[test]
    fn mouse_wheel_seeking_selects_first_entry_before_initial_playback() {
        let mut state = MainWindowUiState::default();
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);

        assert_eq!(state.store.state().playlist.position(), None);
        assert!(state.scroll_main(140, 73, 1.0));

        assert_eq!(state.store.state().playlist.position(), Some(0));
        assert!(state.playback_position_ms() > 0);
        assert_eq!(state.store.state().player.state(), PlayerState::Stopped);
    }

    #[test]
    fn mouse_wheel_over_shaded_sliders_changes_each_slider() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            volume: 50,
            mouse_wheel_change: 12,
            ..Config::default()
        }));
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.store.state_mut().playlist.set_position(0);
        state.store.state_mut().player.mark_playing();
        state.dispatch_store_command(PanelCommand::SetMainShade(true));
        state.toggle_equalizer_shaded();

        assert!(state.scroll_main(227, 5, 1.0));
        let forward_position = state.playback_position_ms();
        assert!(forward_position > 0);
        assert!(state.scroll_main(227, 5, -1.0));
        assert!(state.playback_position_ms() < forward_position);
        assert!(state.equalizer_scroll(62, 5, -1.0));
        assert_eq!(state.volume(), 62);
        assert!(state.equalizer_scroll(165, 5, -1.0));
        assert_eq!(state.store.state().player.balance(), 12);
    }

    #[test]
    fn wheel_event_coordinates_are_scaled_to_skin_space() {
        assert_eq!(
            scale_event_coords(
                f64::from(MAIN_WINDOW_WIDTH * 2),
                f64::from(MAIN_WINDOW_HEIGHT * 2),
                MAIN_WINDOW_WIDTH,
                MAIN_WINDOW_HEIGHT,
                216.0,
                116.0,
            ),
            (108, 58)
        );
        assert_eq!(
            scale_event_coords(
                f64::from(MAIN_WINDOW_WIDTH * 3),
                f64::from(MAIN_WINDOW_HEIGHT * 3),
                MAIN_WINDOW_WIDTH,
                MAIN_WINDOW_HEIGHT,
                534.0,
                174.0,
            ),
            (178, 58)
        );
    }

    #[test]
    fn playlist_mouse_wheel_scrolls_three_rows() {
        let mut state = MainWindowUiState::default();
        for index in 0..20 {
            state
                .store
                .state_mut()
                .playlist
                .add_uri(format!("file:///tmp/song{index}.mp3"));
        }

        assert!(state.playlist_scroll(1.0));
        assert_eq!(state.playlist_scroll_offset(), 3);
        assert!(state.playlist_scroll(-1.0));
        assert_eq!(state.playlist_scroll_offset(), 0);

        state.toggle_playlist_shaded();
        assert!(state.playlist_scroll(1.0));
        assert_eq!(state.playlist_scroll_offset(), 3);
    }

    #[test]
    fn gtk_runtime_tick_cadence_tracks_visible_work() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            vis_mode: VisMode::Off,
            ..Config::default()
        }));
        assert_eq!(state.runtime_tick_interval(), GTK_IDLE_TICK);

        state.store.state_mut().player.mark_playing();
        assert_eq!(state.runtime_tick_interval(), GTK_PLAYBACK_TICK);

        state.set_visualization_mode(VisMode::Analyzer);
        state.set_visualization_refresh_divisor(3);
        assert_eq!(state.runtime_tick_interval(), GTK_TRANSITION_TICK);

        state.playback_transition = PlaybackTransitionState::start_fadeout(100);
        assert_eq!(state.runtime_tick_interval(), GTK_TRANSITION_TICK);

        state.playback_transition = PlaybackTransitionState::Idle;
        state.store.state_mut().player.pause();
        assert_eq!(state.runtime_tick_interval(), GTK_PAUSED_TICK);
    }

    #[test]
    fn gtk_playback_tick_dirties_only_main_render_target() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            vis_mode: VisMode::Off,
            ..Config::default()
        }));
        state.store.state_mut().player.mark_playing();
        state.update_timer_tick_targets(250);

        assert_eq!(
            state.update_timer_tick_targets(250),
            GtkTickRedraw {
                main: true,
                playlist: false,
                equalizer: false,
            }
        );
        assert!(state.update_timer_tick_targets(500).playlist);
    }

    #[test]
    fn gtk_render_surface_cache_reuses_unchanged_composition() {
        let target = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, 20, 20).unwrap();
        let cr = gtk::cairo::Context::new(&target).unwrap();
        let mut cache = GtkRenderSurfaceCache::default();

        render_scaled_to_gtk_cached(&mut cache, 1_u64, &cr, 20, 20, 10, 10, |_cr, _pass| Ok(()))
            .unwrap();
        render_scaled_to_gtk_cached(&mut cache, 1_u64, &cr, 20, 20, 10, 10, |_cr, _pass| Ok(()))
            .unwrap();
        assert_eq!(cache.render_count, 1);

        render_scaled_to_gtk_cached(&mut cache, 2_u64, &cr, 20, 20, 10, 10, |_cr, _pass| Ok(()))
            .unwrap();
        assert_eq!(cache.render_count, 2);
    }

    #[test]
    fn pause_between_songs_delays_eof_advance() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            pause_between_songs: true,
            pause_between_songs_time: 2,
            ..Config::default()
        }));
        state
            .store
            .state_mut()
            .playlist
            .add_uri("file:///tmp/one.mp3");
        state
            .store
            .state_mut()
            .playlist
            .add_uri("file:///tmp/two.mp3");
        state.store.state_mut().playlist.set_position(0);

        state.playlist_eof_reached();
        assert_eq!(state.store.state().playlist.position(), Some(0));
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::WaitingBetweenSongs {
                remaining_ms: 2_000
            }
        );
        assert_eq!(state.playback_position_ms(), 0);

        assert!(state.update_timer_tick(1_000));
        assert_eq!(state.store.state().playlist.position(), Some(0));
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::WaitingBetweenSongs {
                remaining_ms: 1_000
            }
        );
        assert_eq!(state.playback_position_ms(), 0);

        assert!(state.update_timer_tick(1_000));
        assert_eq!(state.store.state().playlist.position(), Some(1));
        assert_eq!(state.playback_transition, PlaybackTransitionState::Idle);
    }

    #[test]
    fn play_during_pause_between_songs_wait_starts_from_beginning() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            pause_between_songs: true,
            pause_between_songs_time: 2,
            ..Config::default()
        }));
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.store.state_mut().playlist.set_position(0);

        state.playlist_eof_reached();
        assert!(state.update_timer_tick(1_000));
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::WaitingBetweenSongs {
                remaining_ms: 1_000
            }
        );
        assert_eq!(state.playback_position_ms(), 0);

        state.start_current_playlist_playback();

        assert_eq!(state.playback_transition, PlaybackTransitionState::Idle);
        assert_eq!(state.playback_position_ms(), 0);
        assert_eq!(state.playback_position_ms(), 0);
        assert_eq!(state.store.state().player.state(), PlayerState::Playing);
    }

    #[test]
    fn eof_pause_blocks_stale_backend_position_sync() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            pause_between_songs: true,
            pause_between_songs_time: 2,
            ..Config::default()
        }));
        state
            .store
            .state_mut()
            .playlist
            .add_uri("file:///tmp/one.mp3");
        state
            .store
            .state_mut()
            .playlist
            .add_uri("file:///tmp/two.mp3");
        state.store.state_mut().playlist.set_position(0);

        assert!(state.should_sync_backend_position(false));

        state.playlist_eof_reached();

        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::WaitingBetweenSongs {
                remaining_ms: 2_000
            }
        );
        assert_eq!(state.playback_position_ms(), 0);
        assert!(!state.should_sync_backend_position(false));
        assert!(!state.should_sync_backend_position(true));
    }

    #[test]
    fn halt_stops_and_resets_without_fading() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            volume: 80,
            stop_with_fadeout: true,
            playback_position_ms: 42_000,
            ..Config::default()
        }));
        state.store.state_mut().player.mark_playing();

        state.activate_push(MainPushButton::Stop);
        assert_eq!(state.playback_position_ms(), 0);
        assert_eq!(state.store.state().player.state(), PlayerState::Stopped);
        assert_eq!(state.volume(), 80);
        assert_eq!(state.playback_transition, PlaybackTransitionState::Idle);
    }

    #[test]
    fn playback_control_event_handles_play_pause_transitions() {
        let mut state = MainWindowUiState::default();
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);

        assert!(state.handle_playback_control_event(PlaybackControlEvent::Play));
        assert_eq!(state.store.state().player.state(), PlayerState::Playing);

        assert!(state.handle_playback_control_event(PlaybackControlEvent::Pause));
        assert_eq!(state.store.state().player.state(), PlayerState::Paused);

        assert!(!state.handle_playback_control_event(PlaybackControlEvent::Pause));
        assert_eq!(state.store.state().player.state(), PlayerState::Paused);

        assert!(state.handle_playback_control_event(PlaybackControlEvent::Play));
        assert_eq!(state.store.state().player.state(), PlayerState::Playing);

        assert!(!state.handle_playback_control_event(PlaybackControlEvent::Play));
        assert_eq!(state.store.state().player.state(), PlayerState::Playing);
    }

    #[test]
    fn panel_state_maps_visibility_detach_and_shade_flags() {
        let mut state = MainWindowUiState::from_state(AppState::from_config(Config {
            equalizer_visible: true,
            equalizer_detached: false,
            playlist_visible: true,
            playlist_detached: true,
            ..Config::default()
        }));
        state.toggle_equalizer_shaded();

        assert_eq!(
            state.panel_state(PanelKind::Equalizer),
            PanelState::Docked { shaded: true }
        );
        assert_eq!(
            state.panel_state(PanelKind::Playlist),
            PanelState::Detached { shaded: false }
        );

        state.set_playlist_visible(false);
        assert_eq!(state.panel_state(PanelKind::Playlist), PanelState::Hidden);
    }

    #[test]
    fn gtk_store_revision_is_monotonic_across_commands_and_runtime_events() {
        let mut state = MainWindowUiState::default();
        let mut revisions = Vec::new();

        revisions.push(
            state
                .dispatch_store_command(PanelCommand::SetMainShade(true))
                .revision,
        );
        state.set_preference_scale_factor(1.5);
        revisions.push(state.store.revision());
        state.apply_equalizer_preset_values(&EqualizerPreset::from_positions("GTK", 25, [40; 10]));
        revisions.push(state.store.revision());
        state.set_stream_channels_for_e2e(2);
        revisions.push(state.store.revision());
        state.store.update_playback_position_from_runtime(250);
        revisions.push(state.store.revision());

        assert!(revisions.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn gtk_store_dispatch_separates_domain_changes_from_effect_execution() {
        let mut state = MainWindowUiState::default();

        let result = state.dispatch_store_command(AudioCommand::SetVolume(37));

        assert_eq!(state.store.state().player.volume(), 37);
        assert!(result
            .changes
            .contains(crate::app::store::StateChangeSet::PLAYER));
        assert!(result.effects.contains(&AppEffect::SetOutputVolume(37)));
        assert!(state.last_playback_request.is_none());

        for effect in result.effects {
            state.apply_store_effect(effect);
        }
        assert!(state.last_playback_request.is_none());
    }

    #[test]
    fn gtk_playlist_queue_shortcuts_use_the_shared_store_queue() {
        let mut state = MainWindowUiState::default();
        for uri in [
            "file:///music/one.ogg",
            "file:///music/two.ogg",
            "file:///music/three.ogg",
        ] {
            state.add_playlist_uri(uri);
        }
        state.set_playlist_entry_selected(0, true);
        state.set_playlist_entry_selected(2, true);

        assert!(state.toggle_queue_selected_playlist_entries());
        assert_eq!(state.store.playlist_queue(), vec![0, 2]);

        state.reverse_playlist();
        assert_eq!(state.store.playlist_queue(), vec![2, 0]);

        assert!(state.clear_playlist_queue());
        assert!(state.store.playlist_queue().is_empty());
        assert!(!state.clear_playlist_queue());
    }

    #[test]
    fn gtk_playback_request_observability_keeps_only_latest_uri() {
        let mut state = MainWindowUiState::default();

        state.start_backend_playback_uri("file:///music/one.ogg", 0);
        state.start_backend_playback_uri("file:///music/two.ogg", 0);

        assert_eq!(state.last_playback_request(), Some("file:///music/two.ogg"));
    }

    #[test]
    fn gtk_mpris_quit_event_invokes_application_shutdown() {
        let quit_count = std::cell::Cell::new(0);

        assert!(!handle_mpris_quit_request(&[MprisEvent::Raised], || {
            quit_count.set(quit_count.get() + 1);
        }));
        assert!(handle_mpris_quit_request(
            &[MprisEvent::QuitRequested],
            || {
                quit_count.set(quit_count.get() + 1);
            }
        ));
        assert_eq!(quit_count.get(), 1);
    }

    #[test]
    fn unrelated_preference_updates_preserve_runtime_owned_values() {
        let mut state = MainWindowUiState::default();
        state.set_preference_volume(73);
        state.set_preference_repeat(true);

        state.set_preference_scale_factor(1.5);

        assert_eq!(state.volume(), 73);
        assert!(state.repeat());
        let snapshot = state.store.state().persistence_snapshot();
        assert_eq!(snapshot.config.volume, 73);
        assert!(snapshot.config.repeat);
        assert_eq!(snapshot.config.scale_factor, 1.5);
    }

    #[test]
    fn equalizer_preset_file_round_trip_updates_the_store_atomically() {
        let dir = unique_temp_dir("xmms-rs-equalizer-round-trip");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preset.eqf");
        let mut bands = [50; 10];
        bands[0] = 25;
        bands[9] = 73;
        let mut state = MainWindowUiState::default();
        state.apply_equalizer_preset_values(&EqualizerPreset::from_positions("Entry1", 30, bands));
        state.save_equalizer_winamp_file(&path).unwrap();
        state
            .apply_equalizer_preset_values(&EqualizerPreset::from_positions("Reset", 50, [50; 10]));
        let revision_before_load = state.store.revision();

        state.load_equalizer_winamp_file(&path).unwrap();

        assert_eq!(state.store.revision(), revision_before_load + 1);
        assert_eq!(state.equalizer_preamp_position(), 30);
        assert_eq!(state.equalizer_band_position(0), Some(25));
        assert!(state
            .equalizer_band_position(9)
            .is_some_and(|position| (position - 73).abs() <= 2));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn playlist_menu_command_maps_menu_indices() {
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Add, 0),
            Some(PlaylistMenuCommand::OpenLocationWindow)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Add, 2),
            Some(PlaylistMenuCommand::OpenFileDialog)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Remove, 3),
            Some(PlaylistMenuCommand::RemoveSelectedOrCurrent)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::List, 1),
            Some(PlaylistMenuCommand::SavePlaylist)
        );
        assert_eq!(
            PlaylistMenuCommand::from_menu_item(PlaylistMenuKind::Misc, 99),
            None
        );
    }

    #[test]
    fn play_from_stopped_preserves_selected_position() {
        let mut state = MainWindowUiState::default();
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.set_playback_position_ms(42_000);

        state.press(40, 90);
        assert_eq!(state.release(40, 90), UiAction::None);

        assert_eq!(state.store.state().player.state(), PlayerState::Playing);
        assert_eq!(state.playback_position_ms(), 42_000);
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::PendingBackendSeek(42_000)
        );
    }

    #[test]
    fn seeking_while_playing_waits_for_backend_position_confirmation() {
        let mut state = MainWindowUiState::default();
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 120_000);
        state.press(40, 90);
        assert_eq!(state.release(40, 90), UiAction::None);

        state.set_playback_position_ms(42_000);

        assert_eq!(state.playback_position_ms(), 42_000);
        assert_eq!(
            state.playback_transition,
            PlaybackTransitionState::PendingBackendSeek(42_000)
        );
        assert!(!state.should_sync_backend_position(false));
    }

    #[test]
    fn changing_to_next_track_starts_from_beginning() {
        let mut state = MainWindowUiState::default();
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/one.ogg", "One", 120_000);
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/two.ogg", "Two", 120_000);
        state.store.state_mut().playlist.set_position(0);
        state.set_playback_position_ms(42_000);

        state.press(109, 90);
        assert_eq!(state.release(109, 90), UiAction::None);

        assert_eq!(state.store.state().playlist.position(), Some(1));
        assert_eq!(state.playback_position_ms(), 0);
        assert_eq!(state.playback_position_ms(), 0);
    }

    #[test]
    fn skin_browser_content_and_refresh_match_original_selector() {
        let _gtk = init_gtk_for_tests();

        let add = gtk::Button::with_label("Add...");
        add.set_widget_name(SKIN_BROWSER_ADD_WIDGET);
        let close = gtk::Button::with_label("Close");
        close.set_widget_name(SKIN_BROWSER_CLOSE_WIDGET);
        let (content, _content_list) = build_skin_browser_content(&add, &close);

        let header = find_named_widget::<gtk::Label>(&content, SKIN_BROWSER_HEADER_WIDGET)
            .expect("skin browser should have a Skins header");
        assert_eq!(header.text(), "Skins");

        let list = find_named_widget::<gtk::ListBox>(&content, SKIN_BROWSER_LIST_WIDGET)
            .expect("skin browser should have a selectable skin list");
        assert_eq!(list.selection_mode(), gtk::SelectionMode::Single);

        let add = find_named_widget::<gtk::Button>(&content, SKIN_BROWSER_ADD_WIDGET)
            .expect("skin browser should have an Add button");
        assert_eq!(add.label().as_deref(), Some("Add..."));

        let close = find_named_widget::<gtk::Button>(&content, SKIN_BROWSER_CLOSE_WIDGET)
            .expect("skin browser should have a Close button");
        assert_eq!(close.label().as_deref(), Some("Close"));

        let mut labels = Vec::new();
        collect_label_text(&content, &mut labels);
        assert!(!labels
            .iter()
            .any(|label| label.contains("placeholder for the Rust port")));

        let tmp = unique_temp_dir("xmms-rs-skin-browser-refresh");
        let skins = tmp.join("Skins");
        let broken = skins.join("Broken");
        let classic = skins.join("Classic");
        fs::create_dir_all(&broken).unwrap();
        fs::create_dir_all(&classic).unwrap();
        fs::write(broken.join("main.xpm"), b"not an xpm").unwrap();
        fs::write(skins.join("Blue.wsz"), b"archive").unwrap();

        let mut state = MainWindowUiState::default();
        refresh_skin_browser_list(&list, &mut state, std::slice::from_ref(&skins)).unwrap();

        assert_eq!(
            skin_browser_list_labels(&list),
            ["default", "Blue", "Broken", "Classic"]
        );
        assert_eq!(list.selected_row().map(|row| row.index()), Some(0));

        state.store.state_mut().config.skin = Some(classic.display().to_string());
        fs::create_dir_all(skins.join("Zed")).unwrap();
        refresh_skin_browser_list(&list, &mut state, std::slice::from_ref(&skins)).unwrap();

        assert_eq!(
            skin_browser_list_labels(&list),
            ["default", "Blue", "Broken", "Classic", "Zed"]
        );
        assert_eq!(list.selected_row().map(|row| row.index()), Some(3));

        fs::write(
            classic.join("main.xpm"),
            r#"/* XPM */
static char * main_xpm[] = {
"1 1 1 1",
". c #010203",
"."};
"#,
        )
        .unwrap();
        let main_state = Rc::new(RefCell::new(state));
        let main_area = gtk::DrawingArea::new();
        let equalizer_area = gtk::DrawingArea::new();
        let playlist_area = gtk::DrawingArea::new();
        let populating = Rc::new(Cell::new(false));
        connect_skin_browser_selection(
            &list,
            &main_state,
            &main_area,
            &equalizer_area,
            &playlist_area,
            &populating,
        );
        list.select_row(list.row_at_index(0).as_ref());
        list.select_row(list.row_at_index(3).as_ref());

        let state = main_state.borrow();
        let classic_path = classic.display().to_string();
        assert_eq!(state.selected_skin(), Some(classic_path.as_str()));
        assert_eq!(
            state
                .active_skin()
                .get(SkinPixmapKind::Main)
                .unwrap()
                .pixel_argb(0, 0),
            Some(0xff010203)
        );
        drop(state);

        list.select_row(list.row_at_index(2).as_ref());
        let state = main_state.borrow();
        assert_eq!(state.selected_skin(), Some(classic_path.as_str()));
        assert_eq!(state.selected_skin_index(), 3);
        assert_eq!(list.selected_row().map(|row| row.index()), Some(3));
        assert_eq!(
            state
                .active_skin()
                .get(SkinPixmapKind::Main)
                .unwrap()
                .pixel_argb(0, 0),
            Some(0xff010203)
        );

        fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn skin_browser_import_copies_archives_and_directories_to_user_skin_dir() {
        let tmp = unique_temp_dir("xmms-rs-skin-browser-import");
        let source = tmp.join("source");
        let user_skins = tmp.join("user-skins");
        fs::create_dir_all(&source).unwrap();

        let archive = source.join("Blue.wsz");
        fs::write(&archive, b"archive").unwrap();
        let imported_archive = import_skin_to_user_dir(&archive, &user_skins).unwrap();
        assert_eq!(imported_archive, user_skins.join("Blue.wsz"));
        assert_eq!(fs::read(&imported_archive).unwrap(), b"archive");

        let duplicate_archive = import_skin_to_user_dir(&archive, &user_skins).unwrap();
        assert_eq!(duplicate_archive, user_skins.join("Blue 1.wsz"));

        let dir_skin = source.join("Classic");
        fs::create_dir_all(dir_skin.join("nested")).unwrap();
        fs::write(dir_skin.join("main.xpm"), b"main").unwrap();
        fs::write(dir_skin.join("nested").join("eqmain.xpm"), b"eq").unwrap();
        let imported_dir = import_skin_to_user_dir(&dir_skin, &user_skins).unwrap();
        assert_eq!(fs::read(imported_dir.join("main.xpm")).unwrap(), b"main");
        assert_eq!(
            fs::read(imported_dir.join("nested").join("eqmain.xpm")).unwrap(),
            b"eq"
        );

        let unsupported = source.join("notes.txt");
        fs::write(&unsupported, b"not a skin").unwrap();
        assert!(import_skin_to_user_dir(&unsupported, &user_skins).is_err());

        fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn main_menu_anchor_uses_full_menu_button_rect() {
        let rect = main_menu_anchor_rect(
            MAIN_WINDOW_WIDTH * 2,
            MAIN_WINDOW_HEIGHT * 2,
            MAIN_WINDOW_WIDTH,
            MAIN_WINDOW_HEIGHT,
        );

        assert_eq!(rect.x(), 12);
        assert_eq!(rect.y(), 6);
        assert_eq!(rect.width(), 18);
        assert_eq!(rect.height(), 18);

        let docked_height = MAIN_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT;
        let rect = main_menu_anchor_rect(
            MAIN_WINDOW_WIDTH * 2,
            docked_height * 2,
            MAIN_WINDOW_WIDTH,
            docked_height,
        );

        assert_eq!(rect.x(), 12);
        assert_eq!(rect.y(), 6);
        assert_eq!(rect.width(), 18);
        assert_eq!(rect.height(), 18);
    }

    #[test]
    fn main_window_buttons_update_player_and_toggle_state() {
        let mut state = MainWindowUiState::default();

        state.press(40, 90);
        assert_eq!(state.release(40, 90), UiAction::None);
        assert_eq!(state.store.state().player.state(), PlayerState::Stopped);

        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.ogg", "Test", 10_000);
        state.press(40, 90);
        assert_eq!(state.release(40, 90), UiAction::None);
        assert_eq!(state.store.state().player.state(), PlayerState::Playing);

        state.press(63, 90);
        assert_eq!(state.release(63, 90), UiAction::None);
        assert_eq!(state.store.state().player.state(), PlayerState::Paused);

        state.press(165, 90);
        assert_eq!(state.release(165, 90), UiAction::None);
        assert!(state.store.state().playlist.shuffle());

        state.press(243, 59);
        assert_eq!(state.release(243, 59), UiAction::None);
        assert!(state.store.state().config.playlist_visible);
    }

    #[test]
    fn main_render_state_formats_stream_info_like_xmms() {
        let mut state = MainWindowUiState::default();

        assert_eq!(state.render_state().bitrate_text, "   ");
        assert_eq!(state.render_state().frequency_text, "  ");

        state
            .store
            .handle_playback_event(PlaybackEvent::StreamInfo(crate::player::StreamInfo {
                bitrate: Some(192),
                frequency: Some(44_100),
                channels: Some(2),
            }));
        assert_eq!(state.render_state().bitrate_text, "192");
        assert_eq!(state.render_state().frequency_text, "44");

        state
            .store
            .handle_playback_event(PlaybackEvent::StreamInfo(crate::player::StreamInfo {
                bitrate: Some(1280),
                frequency: Some(48),
                channels: Some(2),
            }));
        assert_eq!(state.render_state().bitrate_text, "12H");
        assert_eq!(state.render_state().frequency_text, "48");
    }

    #[test]
    fn main_window_sliders_update_runtime_values() {
        let mut state = MainWindowUiState::default();

        state.press(107, 58);
        state.motion(107, 58);
        assert_eq!(state.release(107, 58), UiAction::None);
        assert_eq!(state.store.state().player.volume(), 0);

        state.press(214, 58);
        assert_eq!(state.release(214, 58), UiAction::None);
        assert!(state.store.state().player.balance() > 70);

        state.press(263, 73);
        assert_eq!(state.release(263, 73), UiAction::None);
        assert_eq!(state.position(), 0);
    }

    #[test]
    fn held_position_slider_does_not_seek_again_when_playback_advances() {
        let mut state = MainWindowUiState::default();
        state
            .store
            .state_mut()
            .playlist
            .add_timed_uri("file:///tmp/test.wav", "Test", 120_000);
        state.store.state_mut().player.mark_playing();

        state.press(20, 73);
        assert!(state.motion(140, 73));
        let dragged_position_ms = state.playback_position_ms();
        state
            .store
            .update_playback_position_from_runtime(dragged_position_ms + 1_000);
        let revision_before_release = state.store.revision();

        assert_eq!(state.release(140, 73), UiAction::None);
        assert_eq!(state.store.revision(), revision_before_release);
        assert_eq!(state.playback_position_ms(), dragged_position_ms + 1_000);
    }

    #[test]
    fn zoom_scale_helpers_resize_from_preferences_scale_factor() {
        let mut state = MainWindowUiState::default();
        state.set_preference_scale_factor(1.7);

        assert_eq!(scale_dim(MAIN_WINDOW_WIDTH, state.scale_factor()), 468);
        assert_eq!(unscale_dim(468, state.scale_factor()), MAIN_WINDOW_WIDTH);
    }

    #[test]
    fn shade_and_close_titlebar_buttons_return_window_actions() {
        let mut state = MainWindowUiState::default();

        state.press(255, 4);
        assert_eq!(state.release(255, 4), UiAction::Resize);
        assert!(state.is_shaded());

        state.press(265, 4);
        assert_eq!(state.release(265, 4), UiAction::Quit);
    }

    #[test]
    fn main_titlebar_drag_region_excludes_title_buttons() {
        let state = MainWindowUiState::default();

        assert!(state.main_title_drag_region(40, 7));
        assert!(!state.main_title_drag_region(6, 4));
        assert!(!state.main_title_drag_region(244, 4));
        assert!(!state.main_title_drag_region(254, 4));
        assert!(!state.main_title_drag_region(264, 4));
        assert!(!state.main_title_drag_region(40, MAIN_TITLEBAR_HEIGHT));
    }

    #[test]
    fn shaded_equalizer_sliders_are_not_titlebar_drag_regions() {
        let mut state = MainWindowUiState::default();
        state.toggle_equalizer_shaded();

        assert!(!state.panel_title_drag_region(PanelKind::Equalizer, 61, 7));
        assert!(!state.panel_title_drag_region(PanelKind::Equalizer, 164, 7));
        assert!(state.panel_title_drag_region(PanelKind::Equalizer, 40, 7));
    }

    #[test]
    fn parse_prompt_time_accepts_seconds_and_minutes_seconds() {
        assert_eq!(parse_time_ms("42"), Some(42_000));
        assert_eq!(parse_time_ms("1:23"), Some(83_000));
        assert_eq!(parse_time_ms(""), None);
        assert_eq!(parse_time_ms("1:2:3"), None);
        assert_eq!(parse_time_ms("not-time"), None);
    }
}
