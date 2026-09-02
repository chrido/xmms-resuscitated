"""Full GTK/egui skinned-control E2E coverage with screenshots and console-log assertions."""

from __future__ import annotations

import subprocess
import time
from importlib import import_module
from pathlib import Path
from typing import Any

from conftest import assert_app_log_contains, read_process_log
from gui import (
    EQUALIZER_CONTROL_RECTS,
    PLAYLIST_FOOTER_RECTS,
    PLAYLIST_MENU_RECTS,
    EqualizerControl,
    EqualizerSlider,
    MainButton,
    MainSlider,
    MainToggleButton,
    MAIN_SLIDER_RECTS,
    MainWindow,
    PANEL_CLOSE_RECT,
    PANEL_SHADE_RECT,
    PlaylistFooterButton,
    PlaylistMenuButton,
    click_skin_rect,
    drag_playlist_scrollbar_to_bottom,
    drag_skin_rect,
    equalizer_slider_rect,
    offset_rect,
    open_panel,
    run_xdotool,
    screenshot_screen,
    screenshot_tool_available,
    window_geometry,
)

pytest: Any = import_module("pytest")

EQUALIZER_WINDOW_TITLE = "XMMS Renascene Rust Equalizer"
PLAYLIST_WINDOW_TITLE = "XMMS Renascene Rust Playlist"
EQUALIZER_SLIDERS = [
    EqualizerSlider.PREAMP,
    EqualizerSlider.BAND_0,
    EqualizerSlider.BAND_1,
    EqualizerSlider.BAND_2,
    EqualizerSlider.BAND_3,
    EqualizerSlider.BAND_4,
    EqualizerSlider.BAND_5,
    EqualizerSlider.BAND_6,
    EqualizerSlider.BAND_7,
    EqualizerSlider.BAND_8,
    EqualizerSlider.BAND_9,
]


def assert_screenshot(path: Path) -> None:
    assert path.is_file()
    assert path.stat().st_size > 0


def require_screenshots() -> None:
    if not screenshot_tool_available():
        pytest.skip("Install ImageMagick 'import' or xwd to capture E2E screenshots")


def capture(test_output: Any) -> Path:
    path = screenshot_screen(test_output.screenshot_path())
    assert_screenshot(path)
    return path

def test_gui_player_transport_toggles_and_sliders_with_tracks_screenshots_and_logs(
    gui_tracked_main_window: MainWindow,
    gui_app_with_tracks: subprocess.Popen[bytes],
    test_output: Any,
) -> None:
    """Click core player controls/sliders on ffmpeg tracks and confirm app logs."""
    require_screenshots()

    capture(test_output)
    gui_tracked_main_window.focus_main_window()

    for toggle in [MainToggleButton.SHUFFLE, MainToggleButton.REPEAT]:
        gui_tracked_main_window.click_main_toggle(toggle)
        time.sleep(0.2)
        capture(test_output)

    for button in [
        MainButton.PLAY,
        MainButton.PAUSE,
        MainButton.STOP,
        MainButton.NEXT,
        MainButton.PREVIOUS,
    ]:
        gui_tracked_main_window.click_main_button(button)
        time.sleep(0.4)
        capture(test_output)

    gui_tracked_main_window.click_main_button(MainButton.PLAY)
    time.sleep(0.8)
    capture(test_output)

    gui_tracked_main_window.drag_main_slider(MainSlider.VOLUME, 0.85)
    time.sleep(0.2)
    capture(test_output)

    gui_tracked_main_window.drag_main_slider(MainSlider.BALANCE, 0.85)
    time.sleep(0.2)
    capture(test_output)

    gui_tracked_main_window.drag_main_slider(MainSlider.POSITION, 0.55)
    time.sleep(0.4)
    capture(test_output)

    assert_app_log_contains(
        gui_app_with_tracks,
        "command Playlist(ToggleShuffle)",
        "command Playlist(ToggleRepeat)",
        "command Player(Play)",
        "StartPlaybackUri",
        "xmms-e2e-track-",
        "command Player(Pause)",
        "command Player(Halt)",
        "command Player(NextTrack)",
        "command Player(PreviousTrack)",
        "command Audio(SetVolume",
        "command Audio(SetBalance",
        "player: slider changed, slider_name=Volume",
        "player: slider changed, slider_name=Balance",
        "player: slider changed, slider_name=Position",
    )


def test_gui_holding_position_slider_does_not_repeat_seek(
    gui_tracked_main_window: MainWindow,
    gui_app_with_tracks: subprocess.Popen[bytes],
) -> None:
    """A stationary held seek knob must emit only the initial position change."""
    marker = "player: slider changed, slider_name=Position"
    rect = MAIN_SLIDER_RECTS[MainSlider.POSITION]
    geometry = window_geometry(gui_tracked_main_window.window_id)
    scale = geometry.width / 275
    start_x = round((rect.x + 1) * scale)
    end_x = round((rect.x + rect.width * 0.55) * scale)
    y = round((rect.y + rect.height / 2) * scale)
    baseline = read_process_log(gui_app_with_tracks).count(marker)

    run_xdotool(
        "windowactivate",
        "--sync",
        gui_tracked_main_window.window_id,
        check=False,
    )
    run_xdotool(
        "mousemove",
        "--window",
        gui_tracked_main_window.window_id,
        str(start_x),
        str(y),
    )
    run_xdotool("mousedown", "1")
    try:
        run_xdotool(
            "mousemove",
            "--window",
            gui_tracked_main_window.window_id,
            str(end_x),
            str(y),
        )
        deadline = time.monotonic() + 5.0
        while time.monotonic() < deadline:
            changed_count = read_process_log(gui_app_with_tracks).count(marker)
            if changed_count > baseline:
                break
            time.sleep(0.05)
        else:
            raise AssertionError("position-slider drag did not emit an initial change")

        time.sleep(0.8)
        assert read_process_log(gui_app_with_tracks).count(marker) == changed_count
    finally:
        run_xdotool("mouseup", "1", check=False)

    time.sleep(0.2)
    assert read_process_log(gui_app_with_tracks).count(marker) == changed_count


def test_gui_equalizer_controls_and_sliders_screenshots_and_logs(
    gui_tracked_main_window: MainWindow,
    gui_app_with_tracks: subprocess.Popen[bytes],
    test_output: Any,
) -> None:
    """Click every equalizer button and every equalizer slider."""
    require_screenshots()

    equalizer_window, equalizer_y = open_panel(
        gui_tracked_main_window,
        MainToggleButton.EQUALIZER,
        EQUALIZER_WINDOW_TITLE,
        settle_delay=0.3,
        on_open=lambda: capture(test_output),
    )

    for control in [EqualizerControl.ON, EqualizerControl.AUTO, EqualizerControl.PRESETS]:
        click_skin_rect(equalizer_window, offset_rect(EQUALIZER_CONTROL_RECTS[control], equalizer_y))
        time.sleep(0.25)
        capture(test_output)
        if control is EqualizerControl.PRESETS:
            run_xdotool("key", "Escape", check=False)
            time.sleep(0.1)

    for index, slider in enumerate(EQUALIZER_SLIDERS):
        drag_skin_rect(
            equalizer_window,
            offset_rect(equalizer_slider_rect(slider), equalizer_y),
            end_fraction=0.2 if index % 2 == 0 else 0.8,
            horizontal=False,
        )
        time.sleep(0.15)
        capture(test_output)

    click_skin_rect(equalizer_window, offset_rect(PANEL_SHADE_RECT, equalizer_y))
    time.sleep(0.25)
    capture(test_output)
    click_skin_rect(equalizer_window, offset_rect(PANEL_SHADE_RECT, equalizer_y))
    time.sleep(0.25)
    capture(test_output)
    click_skin_rect(equalizer_window, offset_rect(PANEL_CLOSE_RECT, equalizer_y))
    time.sleep(0.25)
    capture(test_output)

    assert_app_log_contains(
        gui_app_with_tracks,
        "equalizer: control activated, control_name=On",
        "equalizer: control activated, control_name=Auto",
        "equalizer: control activated, control_name=Presets",
        "command Equalizer(SetPreamp",
        "command Equalizer(SetBand { band: 0",
        "command Equalizer(SetBand { band: 9",
        "command Panel(ToggleEqualizerShade)",
        "command Panel(SetEqualizerVisibility(false))",
    )


def test_gui_playlist_buttons_menus_and_scrollbar_screenshots_and_logs(
    gui_tracked_main_window: MainWindow,
    gui_app_with_tracks: subprocess.Popen[bytes],
    test_output: Any,
) -> None:
    """Click every playlist bottom/menu button and drag the playlist scrollbar."""
    require_screenshots()

    playlist_window, playlist_y = open_panel(
        gui_tracked_main_window,
        MainToggleButton.PLAYLIST,
        PLAYLIST_WINDOW_TITLE,
        settle_delay=0.3,
        on_open=lambda: capture(test_output),
    )

    for menu in [
        PlaylistMenuButton.ADD,
        PlaylistMenuButton.REMOVE,
        PlaylistMenuButton.SELECT,
        PlaylistMenuButton.MISC,
        PlaylistMenuButton.LIST,
    ]:
        click_skin_rect(playlist_window, offset_rect(PLAYLIST_MENU_RECTS[menu], playlist_y))
        time.sleep(0.25)
        capture(test_output)
        run_xdotool("key", "Escape", check=False)
        time.sleep(0.1)

    for button in [
        PlaylistFooterButton.PREVIOUS,
        PlaylistFooterButton.PLAY,
        PlaylistFooterButton.PAUSE,
        PlaylistFooterButton.STOP,
        PlaylistFooterButton.NEXT,
        PlaylistFooterButton.SCROLL_DOWN,
        PlaylistFooterButton.SCROLL_UP,
    ]:
        click_skin_rect(playlist_window, offset_rect(PLAYLIST_FOOTER_RECTS[button], playlist_y))
        time.sleep(0.3)
        capture(test_output)

    drag_playlist_scrollbar_to_bottom(playlist_window, playlist_y)
    time.sleep(0.3)
    capture(test_output)
    run_xdotool("key", "Escape", check=False)
    time.sleep(0.2)

    click_skin_rect(playlist_window, offset_rect(PANEL_SHADE_RECT, playlist_y))
    time.sleep(0.25)
    capture(test_output)
    click_skin_rect(playlist_window, offset_rect(PANEL_SHADE_RECT, playlist_y))
    time.sleep(0.25)
    capture(test_output)
    click_skin_rect(playlist_window, offset_rect(PANEL_CLOSE_RECT, playlist_y))
    time.sleep(0.25)
    capture(test_output)

    assert_app_log_contains(
        gui_app_with_tracks,
        "playlist: menu opened, menu_name=Add",
        "playlist: menu opened, menu_name=Remove",
        "playlist: menu opened, menu_name=Select",
        "playlist: menu opened, menu_name=Misc",
        "playlist: menu opened, menu_name=List",
        "playlist: footer button, button_name=Previous",
        "playlist: footer button, button_name=Play",
        "playlist: footer button, button_name=Pause",
        "playlist: footer button, button_name=Stop",
        "playlist: footer button, button_name=Next",
        "playlist: footer button, button_name=ScrollDown",
        "playlist: footer button, button_name=ScrollUp",
        "command Player(Play)",
        "command Player(Pause)",
        "command Player(Halt)",
        "command Player(NextTrack)",
        "command Player(PreviousTrack)",
        "command Panel(TogglePlaylistShade)",
        "command Panel(SetPlaylistVisibility(false))",
    )
