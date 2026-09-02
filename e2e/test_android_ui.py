"""Android emulator UI and screenshot E2E coverage."""

from __future__ import annotations

import re
import time
import wave
import zipfile
from io import BytesIO
from importlib import import_module
from pathlib import Path
from typing import Any, Callable

from PIL import Image

from android import ANDROID_AUTO_PROBE_ACTIVITY, ANDROID_PACKAGE, AndroidDevice
from gui import (
    BASE_MAIN_WIDTH,
    EQUALIZER_CONTROL_RECTS,
    MAIN_BUTTON_RECTS,
    MAIN_PLAYER_BASE_HEIGHT,
    MAIN_TOGGLE_RECTS,
    EqualizerControl,
    MainButton,
    MainToggleButton,
    offset_rect,
)

pytest: Any = import_module("pytest")

pytestmark = pytest.mark.android

ANDROID_ACTIVITY_STATES = ("stopped", "foreground", "background", "closed")


def _prepare_widget_lifecycle_track(android_device: AndroidDevice) -> None:
    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 180)
    android_device.write_private_bytes(
        "files/imports/widget-lifecycle.wav",
        audio.getvalue(),
    )
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:180,Widget Lifecycle\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/widget-lifecycle.wav\n",
    )


def _enter_android_activity_state(
    android_device: AndroidDevice,
    state: str,
) -> None:
    if state == "stopped":
        android_device.force_stop()
    elif state == "foreground":
        android_device.start_activity()
        android_device.main_player_bounds()
    elif state == "background":
        android_device.start_activity()
        android_device.main_player_bounds()
        android_device.go_home()
    elif state == "closed":
        android_device.start_activity()
        android_device.main_player_bounds()
        android_device.close_activity()
    else:
        raise ValueError(f"Unknown Android activity state: {state}")


def test_android_checkpoints_background_playback_position(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    audio_path = "files/imports/checkpoint.wav"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()

    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 20)
    android_device.write_private_bytes(audio_path, audio.getvalue())
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:20,Position Checkpoint\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/checkpoint.wav\n",
    )

    android_device.restart_app()
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    android_device.assert_log_contains(
        "player: button activated, button_name=Play",
    )
    android_device.wait_for_service("XmmsPlaybackService")
    android_device.go_home()
    position_ms = android_device.wait_for_private_file_int_at_least(
        config_path,
        "playback_position_ms",
        8_000,
        timeout=15.0,
    )
    assert position_ms < 20_000


def test_android_widget_cold_starts_playback_without_activity(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    audio_path = "files/imports/widget-cold-start.wav"
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()

    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 20)
    android_device.write_private_bytes(audio_path, audio.getvalue())
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:20,Widget Cold Start\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/widget-cold-start.wav\n",
    )

    android_device.start_widget_control()

    android_device.wait_for_service("XmmsPlaybackService")
    position_ms = android_device.wait_for_private_file_int_at_least(
        config_path,
        "playback_position_ms",
        500,
        timeout=15.0,
    )
    assert position_ms < 20_000


def test_android_widget_playback_attaches_foreground_analyzer_backend(
    android_device: AndroidDevice,
) -> None:
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    _prepare_widget_lifecycle_track(android_device)

    android_device.start_widget_control()
    android_device.wait_for_service("XmmsPlaybackService")
    android_device.wait_for_media_session_position_at_least(500, timeout=10.0)

    android_device.open_player_from_info_widget()

    android_device.assert_log_contains(
        "egui attached existing Android playback backend",
    )
    android_device.main_player_bounds()
    android_device.assert_no_app_crash()


@pytest.mark.parametrize(
    ("process_fate", "resume_trigger"),
    [
        pytest.param("alive", "widget", id="cached-widget-play"),
        pytest.param("killed", "widget", id="killed-widget-play"),
        pytest.param("killed", "app", id="killed-app-open"),
    ],
)
def test_android_resumes_cached_idle_process(
    android_device: AndroidDevice,
    test_output: Any,
    process_fate: str,
    resume_trigger: str,
) -> None:
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    _prepare_widget_lifecycle_track(android_device)

    android_device.start_activity()
    android_device.main_player_bounds()
    cached_pid = android_device.cache_process()
    if process_fate == "killed":
        android_device.kill_background_process()

    android_device.clear_logcat()
    if resume_trigger == "widget":
        android_device.start_widget_control()
    android_device.start_activity()
    resumed_pid = android_device.app_pid()
    if process_fate == "alive":
        assert resumed_pid == cached_pid
    else:
        assert resumed_pid and resumed_pid != cached_pid
    if resume_trigger == "widget":
        android_device.wait_for_service("XmmsPlaybackService")
        android_device.wait_for_media_session_position_at_least(500, timeout=10.0)
    android_device.main_player_bounds()
    screenshot = android_device.screenshot(test_output.screenshot_path())
    android_device.assert_player_rendered(screenshot)
    android_device.assert_no_app_crash()


@pytest.mark.parametrize(
    "initial_activity_state",
    ANDROID_ACTIVITY_STATES,
    ids=lambda state: f"from-{state}",
)
def test_android_info_widget_open_renders_after_activity_lifecycle(
    android_device: AndroidDevice,
    test_output: Any,
    initial_activity_state: str,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    _enter_android_activity_state(android_device, initial_activity_state)

    android_device.open_player_from_info_widget()

    screenshot = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        timeout=10.0,
    )
    android_device.assert_player_rendered(screenshot)
    android_device.assert_no_app_crash()


def test_android_info_widget_repeated_background_returns_render_player(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)

    for _ in range(3):
        android_device.go_home()
        android_device.open_player_from_info_widget()
        screenshot = android_device.wait_for_rendered_screenshot(
            test_output.screenshot_path(),
            timeout=10.0,
        )
        android_device.assert_player_rendered(screenshot)
        android_device.assert_no_app_crash()


@pytest.mark.parametrize(
    "initial_activity_state",
    ANDROID_ACTIVITY_STATES,
    ids=lambda state: f"initial-{state}",
)
@pytest.mark.parametrize(
    "repeated_activity_state",
    ANDROID_ACTIVITY_STATES,
    ids=lambda state: f"repeat-{state}",
)
def test_android_widget_playback_survives_activity_lifecycle_matrix(
    android_device: AndroidDevice,
    test_output: Any,
    initial_activity_state: str,
    repeated_activity_state: str,
) -> None:
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    _prepare_widget_lifecycle_track(android_device)

    _enter_android_activity_state(android_device, initial_activity_state)
    android_device.clear_logcat()
    android_device.start_widget_control()
    android_device.start_activity()
    android_device.wait_for_service("XmmsPlaybackService")
    android_device.wait_for_media_session_position_at_least(
        500,
        timeout=10.0,
    )
    android_device.assert_no_app_crash()

    android_device.main_player_bounds()
    initial_screenshot = android_device.screenshot(test_output.screenshot_path())
    android_device.assert_player_rendered(initial_screenshot)
    android_device.assert_no_app_crash()

    baseline_position = android_device.wait_for_media_session_position_at_least(500)
    _enter_android_activity_state(android_device, repeated_activity_state)
    android_device.clear_logcat()
    android_device.start_widget_control()
    android_device.start_activity()
    android_device.wait_for_service("XmmsPlaybackService")
    minimum_position = (
        500
        if repeated_activity_state == "stopped"
        else baseline_position + 500
    )
    repeated_position = android_device.wait_for_media_session_position_at_least(
        minimum_position,
        timeout=10.0,
    )
    android_device.assert_no_app_crash()

    android_device.main_player_bounds()
    repeated_screenshot = android_device.screenshot(test_output.screenshot_path())
    android_device.assert_player_rendered(repeated_screenshot)
    android_device.assert_no_app_crash()
    assert repeated_position < 180_000


def test_android_redraws_after_background_playback_resume(
    android_device: AndroidDevice,
) -> None:
    audio_path = "files/imports/background-resume.wav"
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()

    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 60)
    android_device.write_private_bytes(audio_path, audio.getvalue())
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:60,Background Resume\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/background-resume.wav\n",
    )

    android_device.start_activity()
    player_bounds = android_device.main_player_bounds()
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY], player_bounds)
    android_device.wait_for_service("XmmsPlaybackService")
    original_pid = android_device.app_pid()
    android_device.go_home()
    assert android_device.app_pid() == original_pid
    time.sleep(1.0)

    android_device.start_activity()

    assert android_device.app_pid() == original_pid
    android_device.main_player_bounds()


def test_android_stop_then_play_stays_synchronized(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    audio_path = "files/imports/stop-restart.wav"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()

    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 180)
    android_device.write_private_bytes(audio_path, audio.getvalue())
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:180,Stop Restart\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/stop-restart.wav\n",
    )

    android_device.restart_app()
    player_bounds = android_device.main_player_bounds()
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY], player_bounds)
    android_device.wait_for_service("XmmsPlaybackService")
    android_device.wait_for_media_session_position_at_least(1_500, timeout=10.0)
    playing = android_device.screenshot(test_output.screenshot_path())

    _tap_skin_rect_until_log(
        android_device,
        MAIN_BUTTON_RECTS[MainButton.STOP],
        "player: button activated, button_name=Stop",
    )
    android_device.wait_for_private_file_contains(
        config_path,
        "playback_position_ms=0",
    )
    stopped = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=playing,
        minimum_changed_fraction=0.0001,
    )
    android_device.wait_for_service_absent("XmmsPlaybackService")
    time.sleep(1.2)
    assert _private_config_int(android_device, "playback_position_ms") == 0

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY], player_bounds)
    android_device.wait_for_service("XmmsPlaybackService")
    restarted_position = android_device.wait_for_media_session_position_at_least(
        500,
        timeout=10.0,
    )
    restarted = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=stopped,
        stable_for=0,
        minimum_changed_fraction=0.0001,
    )

    android_device.assert_player_rendered(playing)
    android_device.assert_player_rendered(stopped)
    android_device.assert_player_rendered(restarted)
    assert restarted_position < 3_000
    assert not android_device.rendered_screens_match(playing, stopped)
    assert not android_device.rendered_screens_match(stopped, restarted)


def test_android_shows_playlist_by_default(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    geometry = android_device.display_geometry()
    player_bounds = android_device.main_player_bounds()
    assert player_bounds[1] <= geometry.top_inset + 4
    assert player_bounds[3] <= geometry.height - geometry.bottom_inset + 1
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.PLAYLIST],
        player_bounds,
    )

    android_device.assert_log_contains(
        "player: toggle activated, toggle_name=Playlist",
    )
    android_device.wait_for_private_file_contains(
        config_path,
        "playlist_visible=false",
    )

    android_device.restart_app()
    android_device.tap_skin_rect(MAIN_TOGGLE_RECTS[MainToggleButton.PLAYLIST])
    android_device.wait_for_private_file_contains(
        config_path,
        "playlist_visible=true",
    )


def test_android_portrait_player_controls_and_panels(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    initial = android_device.screenshot(test_output.screenshot_path())

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    android_device.tap_skin_rect(MAIN_TOGGLE_RECTS[MainToggleButton.EQUALIZER])
    equalizer = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_skin_rect(MAIN_TOGGLE_RECTS[MainToggleButton.PLAYLIST])
    playlist = android_device.screenshot(test_output.screenshot_path())

    android_device.assert_log_contains(
        "player: button activated, button_name=Play",
        "player: toggle activated, toggle_name=Equalizer",
        "player: toggle activated, toggle_name=Playlist",
    )
    assert initial.read_bytes() != equalizer.read_bytes()
    assert equalizer.read_bytes() != playlist.read_bytes()


def test_android_preferences_use_touch_layout_and_back_navigation(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    original_pid = android_device.app_pid()

    player = android_device.framebuffer_png()
    android_device.clear_logcat()
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    android_device.assert_log_contains("command Ui(SetPreferencesVisible(true))")
    categories = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=player,
    )

    android_device.tap_usable_fraction(0.5, 0.22)
    player_page = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=categories,
    )
    android_device.swipe_usable_fraction(0.20, 0.5, 0.70, 0.5)
    categories_after_back = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=player_page,
    )
    android_device.tap_usable_fraction(0.5, 0.463)
    skins_page = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=categories_after_back,
    )
    android_device.swipe_usable_fraction(0.20, 0.5, 0.70, 0.5)
    categories_after_skin_back = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=skins_page,
    )
    android_device.tap_usable_fraction(0.13, 0.045)
    android_device.assert_log_contains("command Ui(SetPreferencesVisible(false))")
    closed = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=categories_after_skin_back,
    )

    android_device.assert_log_contains(
        "command Ui(SetPreferencesVisible(true))",
        "command Ui(SetPreferencesVisible(false))",
    )
    assert android_device.app_pid() == original_pid
    assert not android_device.rendered_screens_match(categories, player_page)
    assert android_device.rendered_screens_match(categories, categories_after_back)
    assert not android_device.rendered_screens_match(categories, skins_page)
    assert android_device.rendered_screens_match(
        categories,
        categories_after_skin_back,
    )
    assert not android_device.rendered_screens_match(categories, closed)


def test_android_equalizer_presets_menu_saves_winamp_eqf(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    output_path = "/sdcard/Download/preset.eqf"
    android_device.shell("rm", "-f", output_path, check=False)
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player_bounds = android_device.main_player_bounds()
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.EQUALIZER],
        player_bounds,
    )
    player_bounds = android_device.main_player_bounds()
    android_device.tap_skin_rect(
        offset_rect(
            EQUALIZER_CONTROL_RECTS[EqualizerControl.PRESETS],
            MAIN_PLAYER_BASE_HEIGHT,
        ),
        player_bounds,
    )
    menu = android_device.screenshot(test_output.screenshot_path())

    android_device.tap_usable_fraction(0.477, 0.405)
    android_device.wait_for_focus("com.google.android.documentsui")
    picker = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_ui_text("Save")
    android_device.wait_for_focus(ANDROID_PACKAGE)
    android_device.wait_for_external_file(output_path)
    header = android_device.command(
        "exec-out",
        "head",
        "-c",
        "31",
        output_path,
    ).stdout

    assert menu.read_bytes() != picker.read_bytes()
    assert header == "Winamp EQ library file v1.1\x1a!--"
    android_device.shell("rm", "-f", output_path, check=False)


def test_android_menu_button_opens_preferences_directly(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player_bounds = android_device.main_player_bounds()

    player = android_device.framebuffer_png()
    android_device.clear_logcat()
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU], player_bounds)
    android_device.assert_log_contains("command Ui(SetPreferencesVisible(true))")
    preferences = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=player,
    )
    android_device.tap_usable_fraction(0.13, 0.045)
    android_device.assert_log_contains("command Ui(SetPreferencesVisible(false))")
    player_after_close = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=preferences,
    )

    android_device.assert_log_contains(
        "command Ui(SetPreferencesVisible(true))",
        "command Ui(SetPreferencesVisible(false))",
    )
    assert not android_device.rendered_screens_match(preferences, player_after_close)


def test_android_landscape_uses_full_height_and_accepts_skin_taps(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.restart_app(reset_data=True)
    android_device.main_player_bounds()
    android_device.set_landscape()
    android_device.wait_for_app()
    geometry = android_device.display_geometry()
    try:
        scale = android_device.main_player_scale()
    except AssertionError:
        android_device.restart_app()
        android_device.set_landscape()
        android_device.wait_for_app()
        geometry = android_device.display_geometry()
        scale = android_device.main_player_scale()
    player_bounds = android_device.main_player_bounds()
    playlist_bounds = android_device.landscape_playlist_bounds()

    assert geometry.width > geometry.height
    default_player_column_height = 116 * 2
    assert scale * default_player_column_height >= geometry.usable_height * 0.85
    assert playlist_bounds[0] >= player_bounds[2] - 4
    assert playlist_bounds[1] <= player_bounds[1] + 8
    safe_left = geometry.left_inset
    safe_right = geometry.width - geometry.right_inset
    assert player_bounds[0] >= safe_left - 2
    assert player_bounds[2] <= safe_right + 2
    assert playlist_bounds[0] >= safe_left - 2
    assert playlist_bounds[2] <= safe_right + 2

    before = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_skin_rect(MAIN_TOGGLE_RECTS[MainToggleButton.REPEAT])
    android_device.assert_log_contains(
        "player: toggle activated, toggle_name=Repeat",
    )
    time.sleep(0.2)
    after = android_device.screenshot(test_output.screenshot_path())
    assert before.read_bytes() != after.read_bytes()

    android_device.set_portrait()
    android_device.wait_for_app()
    portrait_geometry = android_device.display_geometry()
    portrait_stack = android_device.portrait_docked_stack_bounds()
    portrait_safe_right = portrait_geometry.width - portrait_geometry.right_inset
    assert abs(portrait_stack[0] - portrait_geometry.left_inset) <= 4
    assert abs(portrait_stack[2] - portrait_safe_right) <= 4
    assert portrait_stack[1] >= portrait_geometry.top_inset - 2
    assert portrait_stack[3] <= portrait_geometry.height - portrait_geometry.bottom_inset + 2
    portrait_scale = (portrait_stack[2] - portrait_stack[0]) / BASE_MAIN_WIDTH
    player_and_equalizer_height = 2 * MAIN_PLAYER_BASE_HEIGHT * portrait_scale
    assert portrait_stack[3] - portrait_stack[1] > player_and_equalizer_height + 20


def test_android_orientation_round_trip_reflows_without_restart(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_sensor_landscape()
    android_device.restart_app(reset_data=True)
    original_pid = android_device.app_pid()

    first_landscape_geometry = android_device.display_geometry()
    first_landscape_player = android_device.main_player_bounds()
    first_landscape_playlist = android_device.landscape_playlist_bounds()
    first_landscape = android_device.screenshot(test_output.screenshot_path())
    assert first_landscape_geometry.width > first_landscape_geometry.height
    assert first_landscape_playlist[0] >= first_landscape_player[2] - 4

    android_device.set_sensor_portrait()
    android_device.wait_for_app()
    portrait_geometry = android_device.display_geometry()
    portrait_stack = android_device.portrait_docked_stack_bounds()
    portrait = android_device.screenshot(test_output.screenshot_path())
    assert android_device.app_pid() == original_pid
    assert portrait_geometry.width < portrait_geometry.height
    assert portrait_stack[2] <= portrait_geometry.width - portrait_geometry.right_inset + 4
    assert not android_device.rendered_screens_match(first_landscape, portrait)

    android_device.set_sensor_landscape()
    android_device.wait_for_app()
    second_landscape_geometry = android_device.display_geometry()
    second_landscape_player = android_device.main_player_bounds()
    second_landscape_playlist = android_device.landscape_playlist_bounds()
    second_landscape = android_device.screenshot(test_output.screenshot_path())
    assert android_device.app_pid() == original_pid
    assert second_landscape_geometry.width > second_landscape_geometry.height
    assert second_landscape_playlist[0] >= second_landscape_player[2] - 4
    assert not android_device.rendered_screens_match(portrait, second_landscape)


def test_android_persists_player_configuration(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )
    android_device.wait_for_private_file_contains(config_path, "shuffle=true")

    android_device.restart_app()
    player_bounds = android_device.main_player_bounds()
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )

    android_device.assert_log_contains(
        "player: toggle activated, toggle_name=Shuffle",
    )
    android_device.wait_for_private_file_contains(config_path, "shuffle=false")


def test_android_flushes_recent_state_before_backgrounding(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )
    android_device.wait_for_private_file_contains(config_path, "shuffle=true")

    android_device.clear_logcat()
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.REPEAT],
        player_bounds,
    )
    android_device.assert_log_contains(
        "player: toggle activated, toggle_name=Repeat",
    )
    android_device.go_home()
    android_device.force_stop()

    config = android_device.read_private_file(config_path)
    assert "repeat=true" in config


def test_android_restores_panels_settings_and_playlist_after_relaunch(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    playlist_path = "files/config/xmms-renascene/playlist.m3u"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    android_device.write_private_file(
        playlist_path,
        "#EXTM3U\n"
        "#EXTINF:42,Restored Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/restored.wav\n",
    )
    android_device.restart_app()
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.EQUALIZER],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.REPEAT],
        player_bounds,
    )
    android_device.tap_skin_rect(
        offset_rect(
            EQUALIZER_CONTROL_RECTS[EqualizerControl.ON],
            MAIN_PLAYER_BASE_HEIGHT,
        ),
        player_bounds,
    )
    for setting in (
        "playlist_visible=true",
        "equalizer_visible=true",
        "equalizer_active=false",
        "shuffle=true",
        "repeat=true",
    ):
        android_device.wait_for_private_file_contains(config_path, setting)

    original_pid = android_device.close_activity()
    android_device.start_activity()
    assert android_device.app_pid() == original_pid
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(
        offset_rect(
            EQUALIZER_CONTROL_RECTS[EqualizerControl.ON],
            MAIN_PLAYER_BASE_HEIGHT,
        ),
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.PLAYLIST],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.EQUALIZER],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.REPEAT],
        player_bounds,
    )
    for setting in (
        "playlist_visible=false",
        "equalizer_visible=false",
        "equalizer_active=true",
        "shuffle=false",
        "repeat=false",
    ):
        android_device.wait_for_private_file_contains(config_path, setting)
    android_device.wait_for_private_file_contains(playlist_path, "Restored Track")


def test_android_managed_playlists_save_load_and_delete_from_settings(
    android_device: AndroidDevice,
) -> None:
    playlist_path = "files/config/xmms-renascene/playlist.m3u"
    managed_path = "files/config/playlists/playlist"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    android_device.write_private_file(
        playlist_path,
        "#EXTM3U\n"
        "#EXTINF:42,Managed Original\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/original.wav\n",
    )
    android_device.restart_app()

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    android_device.tap_usable_fraction(0.5, 0.544)
    android_device.tap_usable_fraction(0.5, 0.30)
    android_device.wait_for_private_file_contains(managed_path, "Managed Original")
    assert not android_device.private_file_exists(f"{managed_path}.m3u8")

    android_device.write_private_file(
        managed_path,
        "#EXTM3U\n"
        "#EXTINF:42,Managed Loaded\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/loaded.wav\n",
    )
    android_device.tap_horizontal_button_group(0, button_count=3)
    android_device.close_activity()
    android_device.wait_for_private_file_contains(playlist_path, "Managed Loaded")
    android_device.start_activity()

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    android_device.tap_usable_fraction(0.5, 0.544)
    android_device.tap_horizontal_button_group(2, button_count=3)
    android_device.wait_for_private_file_absent(managed_path)


def test_android_playlist_save_and_load_menu_items_open_managed_dialog(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player = android_device.screenshot(test_output.screenshot_path())
    left, top, right, bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    list_x = round(left + 240.5 * scale)

    def tap_list_y(offset_from_bottom: float) -> None:
        android_device.shell(
            "input",
            "tap",
            str(list_x),
            str(round(bottom - offset_from_bottom * scale)),
        )
        time.sleep(0.4)

    tap_list_y(20)
    tap_list_y(39)
    save_dialog = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_usable_fraction(0.13, 0.043)

    tap_list_y(20)
    tap_list_y(21)
    load_dialog = android_device.screenshot(test_output.screenshot_path())

    def image_pixels(path: Path) -> bytes:
        with Image.open(path) as image:
            return image.convert("RGB").tobytes()

    assert image_pixels(player) != image_pixels(save_dialog)
    assert image_pixels(player) != image_pixels(load_dialog)


def _playlist_row_point(
    android_device: AndroidDevice,
    index: int,
) -> tuple[int, int]:
    left, top, right, _bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    rows_top = top + round((MAIN_PLAYER_BASE_HEIGHT + 20) * scale)
    return (
        left + round(120 * scale),
        rows_top + round((index * 11 + 5.5) * scale),
    )


def _run_playlist_swipe_until_log(
    android_device: AndroidDevice,
    *,
    points: Callable[[], tuple[int, int, int, int]],
    duration_ms: int,
    expected_log: str,
) -> str:
    android_device.clear_logcat()
    last_error: AssertionError | None = None
    for attempt in range(3):
        android_device.wait_for_app()
        start_x, start_y, end_x, end_y = points()
        android_device.shell(
            "input",
            "swipe",
            str(start_x),
            str(start_y),
            str(end_x),
            str(end_y),
            str(duration_ms),
        )
        try:
            return android_device.assert_log_contains(expected_log, timeout=6.0)
        except AssertionError as error:
            last_error = error
            if attempt < 2:
                android_device.recover_input_dispatch()
    assert last_error is not None
    raise last_error


def _tap_skin_rect_until_log(
    android_device: AndroidDevice,
    rect: Any,
    expected_log: str,
) -> str:
    android_device.clear_logcat()
    last_error: AssertionError | None = None
    for attempt in range(3):
        android_device.tap_skin_rect(rect)
        try:
            return android_device.assert_log_contains(expected_log, timeout=6.0)
        except AssertionError as error:
            last_error = error
            if attempt < 2:
                android_device.recover_input_dispatch()
    assert last_error is not None
    raise last_error


def _swipe_playlist_row_horizontally(
    android_device: AndroidDevice,
    index: int,
    *,
    selected: bool,
) -> None:
    def points() -> tuple[int, int, int, int]:
        left, _top, right, _bottom = android_device.main_player_bounds()
        scale = (right - left) / 275
        _row_x, row_y = _playlist_row_point(android_device, index)
        start_x = left + round((40 if selected else 200) * scale)
        end_x = left + round((200 if selected else 40) * scale)
        return start_x, row_y, end_x, row_y

    _run_playlist_swipe_until_log(
        android_device,
        points=points,
        duration_ms=300,
        expected_log=(
            f"playlist: swipe selection applied, swiped_index={index}, "
            f"selected={str(selected).lower()}"
        ),
    )


def _swipe_playlist_up(
    android_device: AndroidDevice,
    touched_index: int,
    *,
    selected_index: int,
) -> str:
    def points() -> tuple[int, int, int, int]:
        start_x, start_y = _playlist_row_point(android_device, touched_index)
        scale = android_device.main_player_scale()
        return start_x, start_y, start_x, start_y - round(120 * scale)

    return _run_playlist_swipe_until_log(
        android_device,
        points=points,
        duration_ms=300,
        expected_log=f"playlist: swipe playback started, selected_index={selected_index}",
    )


def _swipe_playlist_down(
    android_device: AndroidDevice,
    touched_index: int,
) -> str:
    def points() -> tuple[int, int, int, int]:
        start_x, start_y = _playlist_row_point(android_device, touched_index)
        scale = android_device.main_player_scale()
        return start_x, start_y, start_x, start_y + round(360 * scale)

    return _run_playlist_swipe_until_log(
        android_device,
        points=points,
        duration_ms=900,
        expected_log="playlist: swipe playback paused",
    )


def _private_config_int(
    android_device: AndroidDevice,
    key: str,
) -> int:
    config = android_device.read_private_file("files/config/xmms-renascene/config")
    match = re.search(rf"^{re.escape(key)}=(-?\d+)$", config, re.MULTILINE)
    if match is None:
        raise AssertionError(f"Android config did not contain {key!r}:\n{config}")
    return int(match.group(1))


def _prepare_android_swipe_playlist(android_device: AndroidDevice) -> None:
    playlist_path = "files/config/xmms-renascene/playlist.m3u"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    for name in ("first", "second", "third"):
        audio = BytesIO()
        with wave.open(audio, "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(1)
            wav.setframerate(8_000)
            wav.writeframes(bytes([128]) * 8_000 * 180)
        android_device.write_private_bytes(
            f"files/imports/{name}.wav",
            audio.getvalue(),
        )
    android_device.write_private_file(
        playlist_path,
        "#EXTM3U\n"
        "#EXTINF:180,Swipe First\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/first.wav\n"
        "#EXTINF:180,Swipe Second\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/second.wav\n"
        "#EXTINF:180,Swipe Third\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/third.wav\n",
    )
    android_device.restart_app()


def test_android_playlist_swipes_select_right_and_deselect_left(
    android_device: AndroidDevice,
) -> None:
    _prepare_android_swipe_playlist(android_device)
    left, top, right, _bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    row_top = top + round((MAIN_PLAYER_BASE_HEIGHT + 20) * scale)
    row_bottom = row_top + round(11 * scale)

    def row_pixels() -> bytes:
        with Image.open(BytesIO(android_device.framebuffer_png())) as screenshot:
            return screenshot.convert("RGB").crop(
                (left, row_top, right, row_bottom)
            ).tobytes()

    def wait_for_row_change(previous: bytes) -> bytes:
        deadline = time.monotonic() + 5.0
        current = previous
        while time.monotonic() < deadline:
            current = row_pixels()
            if current != previous:
                return current
            time.sleep(0.2)
        raise AssertionError("playlist row did not redraw after swipe selection")

    before = row_pixels()
    _swipe_playlist_row_horizontally(android_device, 0, selected=True)
    selected = wait_for_row_change(before)
    _swipe_playlist_row_horizontally(android_device, 0, selected=False)
    deselected = wait_for_row_change(selected)

    assert before != selected
    assert selected != deselected


def test_android_playlist_swipe_up_starts_only_selected_item(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    _prepare_android_swipe_playlist(android_device)
    _swipe_playlist_row_horizontally(android_device, 1, selected=True)

    _swipe_playlist_up(android_device, touched_index=0, selected_index=1)

    android_device.assert_log_contains(
        "playlist: swipe playback started, selected_index=1",
        "backend: egui play_uri, "
        "uri=file:///data/user/0/org.xmms.renascene/files/imports/second.wav"
    )
    android_device.wait_for_service("XmmsPlaybackService")
    android_device.wait_for_private_file_contains(
        config_path,
        "playlist_position=1",
    )


def test_android_playlist_swipe_up_starts_first_selected_item_in_playlist_order(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    _prepare_android_swipe_playlist(android_device)
    _swipe_playlist_row_horizontally(android_device, 0, selected=True)
    _swipe_playlist_row_horizontally(android_device, 2, selected=True)

    _swipe_playlist_up(android_device, touched_index=2, selected_index=0)

    log = android_device.assert_log_contains(
        "playlist: swipe playback started, selected_index=0",
        "backend: egui play_uri, "
        "uri=file:///data/user/0/org.xmms.renascene/files/imports/first.wav"
    )
    assert (
        "uri=file:///data/user/0/org.xmms.renascene/files/imports/third.wav"
        not in log
    )
    android_device.wait_for_service("XmmsPlaybackService")
    android_device.wait_for_private_file_contains(
        config_path,
        "playlist_position=0",
    )


def test_android_playlist_swipe_down_pauses_and_does_not_resume(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    _prepare_android_swipe_playlist(android_device)
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    android_device.assert_log_contains(
        "backend: egui play_uri, "
        "uri=file:///data/user/0/org.xmms.renascene/files/imports/first.wav"
    )
    android_device.wait_for_service("XmmsPlaybackService")
    time.sleep(1.5)

    _swipe_playlist_down(android_device, touched_index=0)
    android_device.go_home()
    paused_position = android_device.wait_for_private_file_int_at_least(
        config_path,
        "playback_position_ms",
        500,
        timeout=5.0,
    )
    android_device.start_activity()
    android_device.main_player_bounds()

    second_swipe_log = _swipe_playlist_down(android_device, touched_index=0)
    assert "backend: egui play_uri" not in second_swipe_log
    android_device.go_home()
    time.sleep(1.0)
    still_paused_position = _private_config_int(
        android_device,
        "playback_position_ms",
    )
    assert still_paused_position - paused_position <= 250


def test_android_playlist_swipe_up_resumes_selected_paused_track(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    _prepare_android_swipe_playlist(android_device)
    _swipe_playlist_row_horizontally(android_device, 0, selected=True)
    _swipe_playlist_up(android_device, touched_index=1, selected_index=0)
    android_device.assert_log_contains(
        "playlist: swipe playback started, selected_index=0",
        "backend: egui play_uri, "
        "uri=file:///data/user/0/org.xmms.renascene/files/imports/first.wav",
    )
    android_device.wait_for_service("XmmsPlaybackService")
    time.sleep(3.0)

    _swipe_playlist_down(android_device, touched_index=1)
    android_device.go_home()
    paused_position = android_device.wait_for_private_file_int_at_least(
        config_path,
        "playback_position_ms",
        1_500,
        timeout=5.0,
    )
    android_device.start_activity()
    android_device.main_player_bounds()

    resume_log = _swipe_playlist_up(
        android_device,
        touched_index=1,
        selected_index=0,
    )
    assert "backend: egui play_uri" not in resume_log
    time.sleep(1.0)
    android_device.go_home()
    resumed_position = android_device.wait_for_private_file_int_at_least(
        config_path,
        "playback_position_ms",
        paused_position + 500,
        timeout=5.0,
    )
    assert resumed_position > paused_position


def test_android_misc_popup_dismisses_outside_and_file_info_uses_full_screen(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:42,File Info Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/info.wav\n",
    )
    android_device.restart_app()
    player = android_device.screenshot(test_output.screenshot_path())
    left, _top, right, bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    misc_x = round(left + 111.5 * scale)
    menu_y = round(bottom - 20 * scale)

    def open_misc() -> None:
        android_device.shell("input", "tap", str(misc_x), str(menu_y))
        time.sleep(0.5)

    open_misc()
    popup = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_usable_fraction(0.1, 0.5)
    dismissed = android_device.screenshot(test_output.screenshot_path())

    open_misc()
    android_device.tap_usable_fraction(0.67, 0.85)
    file_info = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=dismissed,
    )
    android_device.tap_usable_fraction(0.13, 0.043)
    returned = android_device.wait_for_rendered_screenshot(
        test_output.screenshot_path(),
        changed_from=file_info,
    )

    def image_pixels(path: Path) -> bytes:
        with Image.open(path) as image:
            return image.convert("RGB").tobytes()

    assert image_pixels(player) != image_pixels(popup)
    assert image_pixels(popup) != image_pixels(dismissed)
    assert image_pixels(dismissed) != image_pixels(file_info)
    assert image_pixels(file_info) != image_pixels(returned)


def test_android_clear_list_stops_playback_and_resets_playlist(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    playlist_path = "files/config/xmms-renascene/playlist.m3u"
    audio_path = "files/imports/clear.wav"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()

    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 8)
    android_device.write_private_bytes(audio_path, audio.getvalue())
    android_device.write_private_file(
        playlist_path,
        "#EXTM3U\n"
        "#EXTINF:8,Clear Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/clear.wav\n",
    )
    android_device.restart_app()
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    android_device.wait_for_service("XmmsPlaybackService")

    left, _top, right, bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    list_x = round(left + 240.5 * scale)
    android_device.clear_logcat()
    menu_error: AssertionError | None = None
    for _attempt in range(6):
        android_device.shell(
            "input",
            "tap",
            str(list_x),
            str(round(bottom - 20 * scale)),
        )
        try:
            android_device.assert_log_contains(
                "playlist: menu opened, menu_name=List",
                timeout=2.0,
            )
            break
        except AssertionError as error:
            menu_error = error
    else:
        assert menu_error is not None
        raise menu_error

    clear_error: AssertionError | None = None
    for _attempt in range(6):
        android_device.shell(
            "input",
            "tap",
            str(list_x),
            str(round(bottom - 57 * scale)),
        )
        try:
            android_device.wait_for_private_file_not_contains(
                playlist_path,
                "Clear Track",
                timeout=2.0,
            )
            break
        except AssertionError as error:
            clear_error = error
    else:
        assert clear_error is not None
        raise clear_error

    android_device.wait_for_service_absent("XmmsPlaybackService")
    android_device.wait_for_private_file_contains(config_path, "playback_position_ms=0")


def test_android_auto_media_browser_surface(
    android_device: AndroidDevice,
) -> None:
    manifest = android_device.apk_xmltree("AndroidManifest.xml")
    automotive = android_device.apk_xmltree("res/xml/automotive_app_desc.xml")
    assert "com.google.android.gms.car.application" in manifest
    assert "android.media.browse.MediaBrowserService" in manifest
    assert 'A: name="media"' in automotive

    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:42,Android Auto Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/auto.wav\n",
    )
    android_device.clear_logcat()
    android_device.shell(
        "am",
        "start",
        "-n",
        ANDROID_AUTO_PROBE_ACTIVITY,
    )
    android_device.assert_log_contains(
        "connected root=xmms-root",
        "children parent=xmms-root count=1",
        "children parent=xmms-playlist count=1",
        "first title=Android Auto Track",
    )


def test_android_external_media_volume_source_bridge() -> None:
    root = Path(__file__).resolve().parents[1]
    activity = (
        root / "android/java/org/xmms/renascene/XmmsActivity.java"
    ).read_text()
    bridge = (root / "src/ui/egui/android/jni.rs").read_text()
    events = (root / "src/ui/egui/android_events.rs").read_text()
    app = (root / "src/ui/egui/app.rs").read_text()
    app_state = (root / "src/app_state.rs").read_text()
    store = (root / "src/app/store.rs").read_text()

    assert "new ContentObserver(MAIN_HANDLER)" in activity
    assert "Settings.System.CONTENT_URI, true, mediaVolumeObserver" in activity
    assert "registerMediaVolumeObserver();" in activity
    assert "unregisterMediaVolumeObserver();" in activity
    assert "nativeOnMediaVolumeChanged(volumePercent);" in activity
    assert "volumePercent == lastReportedMediaVolumePercent" in activity
    assert "pendingAppMediaVolumePercent.set(clampedPercent)" in activity
    assert "pendingAppMediaVolumePercent.getAndSet(null)" in activity

    assert "ExternalVolumeChanged(i32)" in events
    assert (
        "Java_org_xmms_renascene_XmmsActivity_nativeOnMediaVolumeChanged"
        in bridge
    )
    assert "volume_percent.clamp(0, 100)" in bridge
    assert "AndroidPlatformEvent::ExternalVolumeChanged(" in bridge
    assert "AndroidPlatformEvent::ExternalVolumeChanged(_) => self" in events
    assert "retain(|queued|" in events
    assert "events::request_registered_repaint();" in bridge

    assert "sync_external_output_volume(volume)" in app
    assert "self.android.mark_persistence();" in app
    platform_poll = app.split("fn poll_android_platform_events", 1)[1].split(
        "\n}\n\nimpl EguiFrontendState", 1
    )[0]
    assert "AndroidPlatformEvent::ExternalVolumeChanged(volume)" in platform_poll
    assert "AudioCommand::SetVolume" not in platform_poll
    assert "set_media_volume_percent" not in platform_poll

    store_sync = store.split("pub fn sync_external_output_volume", 1)[1].split(
        "pub fn complete_stop_fade", 1
    )[0]
    assert "state.player.set_volume(volume)" in store_sync
    assert "state.config.volume = volume" not in store_sync
    assert "config.volume = self.player.volume();" in app_state
    assert "SetOutputVolume" not in store_sync
    assert "SetBackendVolume" not in store_sync


def test_android_player_widget_is_packaged(
    android_device: AndroidDevice,
) -> None:
    manifest = android_device.apk_xmltree("AndroidManifest.xml")
    widget_info = android_device.apk_xmltree("res/xml/player_widget_info.xml")
    info_widget_info = android_device.apk_xmltree(
        "res/xml/player_info_widget_info.xml"
    )
    widget_layout = android_device.apk_xmltree("res/layout/widget_player.xml")
    info_widget_layout = android_device.apk_xmltree(
        "res/layout/widget_player_info.xml"
    )
    widget_source = (
        Path(__file__).resolve().parents[1]
        / "android/java/org/xmms/renascene/XmmsPlayerWidget.java"
    ).read_text()
    info_widget_source = (
        Path(__file__).resolve().parents[1]
        / "android/java/org/xmms/renascene/XmmsPlayerInfoWidget.java"
    ).read_text()
    widget_support_source = (
        Path(__file__).resolve().parents[1]
        / "android/java/org/xmms/renascene/XmmsWidgetSupport.java"
    ).read_text()
    widget_layout_source = (
        Path(__file__).resolve().parents[1]
        / "android/res/layout/widget_player.xml"
    ).read_text()
    info_widget_layout_source = (
        Path(__file__).resolve().parents[1]
        / "android/res/layout/widget_player_info.xml"
    ).read_text()
    info_widget_preview_source = (
        Path(__file__).resolve().parents[1]
        / "android/res/layout/widget_player_info_preview.xml"
    ).read_text()
    info_widget_metadata_source = (
        Path(__file__).resolve().parents[1]
        / "android/res/xml/player_info_widget_info.xml"
    ).read_text()
    info_widget_preview_path = (
        Path(__file__).resolve().parents[1]
        / "android/res/drawable-nodpi/widget_player_info_preview.png"
    )
    native_widget_source = (
        Path(__file__).resolve().parents[1] / "src/ui/egui/android/widgets.rs"
    ).read_text()

    assert ".XmmsPlayerWidget" in manifest
    assert ".XmmsPlayerInfoWidget" in manifest
    assert "android.appwidget.action.APPWIDGET_UPDATE" in manifest
    assert "android:initialLayout" in widget_info
    assert "android:initialLayout" in info_widget_info
    assert "android:previewImage" in widget_info
    assert "android:previewLayout" in widget_info
    assert "android:previewImage" in info_widget_info
    assert "android:previewLayout" in info_widget_info
    assert manifest.count("A: android:icon") >= 3
    assert manifest.count("A: android:label") >= 3
    assert 'android:id="@+id/widget_player_container"' in widget_layout_source
    assert "E: ImageView" in widget_layout
    assert widget_layout.count("E: ImageButton") == 5
    assert "E: TextView" not in widget_layout
    assert "E: ImageView" in info_widget_layout
    assert info_widget_layout.count("E: ImageButton") == 1
    assert "E: TextView" not in info_widget_layout
    assert 'android:id="@android:id/background"' in info_widget_layout_source
    assert 'android:clipToOutline="true"' in info_widget_layout_source
    assert 'android:outlineProvider="none"' in info_widget_layout_source
    assert 'android:id="@+id/widget_player_info_content"' in info_widget_layout_source
    assert 'android:background="@android:color/black"' in info_widget_layout_source
    assert 'android:padding="2dp"' in info_widget_layout_source
    assert 'android:id="@android:id/background"' in info_widget_preview_source
    assert 'android:clipToOutline="true"' in info_widget_preview_source
    assert 'android:outlineProvider="none"' in info_widget_preview_source
    assert 'android:minWidth="168dp"' in info_widget_metadata_source
    assert 'android:minResizeWidth="168dp"' in info_widget_metadata_source
    assert 'android:minHeight="41dp"' in info_widget_metadata_source
    assert 'android:minResizeHeight="41dp"' in info_widget_metadata_source
    with Image.open(info_widget_preview_path) as info_widget_preview:
        assert info_widget_preview.size == (672, 164)
    assert "PLAYER_WIDTH = 114" in widget_source
    assert "PLAYER_HEIGHT = 18" in widget_source
    assert "onAppWidgetOptionsChanged" in widget_source
    assert "getAppWidgetOptions(widgetId)" in widget_source
    assert "views.setViewPadding(" in widget_source
    assert "XmmsWidgetSupport.proportionalPadding(" in widget_source
    assert re.search(
        r"contentHeight\s*=\s*Math\.round\(\(float\) width"
        r"\s*\*\s*nativeHeight\s*/\s*nativeWidth\)",
        widget_support_source,
    )
    assert not re.search(
        r"updateAppWidget\s*\(\s*widgetIds\s*,",
        widget_source,
    )
    pressed_duration = re.search(
        r"PRESSED_DURATION_MS\s*=\s*(\d+)",
        widget_source,
    )
    assert pressed_duration is not None
    assert 100 <= int(pressed_duration.group(1)) <= 200
    assert "showPressedControl(context, control);" in widget_source
    assert re.search(
        r"nativeRenderPlayerWidget\s*\([^;]+activePressedControl\s*\)",
        widget_source,
        re.DOTALL,
    )
    assert "long generation = ++pressedGeneration;" in widget_source
    assert "if (generation != pressedGeneration)" in widget_source
    assert "PRESSED_HANDLER.removeCallbacks(restorePressedRunnable)" in widget_source
    assert "pressedControl = NO_PRESSED_CONTROL;" in widget_source
    assert (
        "PRESSED_HANDLER.postDelayed(restorePressedRunnable, PRESSED_DURATION_MS)"
        in widget_source
    )
    for control, button in (
        (1, "Pause"),
        (2, "Play"),
        (3, "Next"),
        (4, "Previous"),
        (6, "Stop"),
    ):
        assert (
            f"{control} => Some(MainPushButton::{button})"
            in native_widget_source
        )
    assert (
        "render_transport_buttons_color_image(skin, pressed)"
        in native_widget_source
    )
    assert "INFO_WIDTH = 164" in info_widget_source
    assert "INFO_HEIGHT = 37" in info_widget_source
    assert "FRAME_WIDTH = INFO_WIDTH + 4" in info_widget_source
    assert "FRAME_HEIGHT = INFO_HEIGHT + 4" in info_widget_source
    assert "OPEN_PLAYER_REQUEST_CODE = 1000" in info_widget_source
    assert "PendingIntent.getActivity(" in info_widget_source
    assert "new Intent(context, XmmsActivity.class)" in info_widget_source
    assert "onAppWidgetOptionsChanged" in info_widget_source
    assert "getAppWidgetOptions(widgetId)" in info_widget_source
    assert "XmmsWidgetSupport.proportionalPadding(" in info_widget_source
    assert "manager.updateAppWidget(widgetId, views)" in info_widget_source
    assert "widget_player_info_content" in info_widget_source
    assert "views.setOnClickPendingIntent(resources.open" in info_widget_source
    assert "nativeUpdateTitleMarquee(" in info_widget_source
    assert "MARQUEE_HANDLER.postDelayed(this, MARQUEE_TICK_MS)" in info_widget_source
    assert "titleOffsetPx" in info_widget_source
    assert "TextView" not in info_widget_source

    controls = re.findall(
        r'contentDescription[^=]*="([^"]+)"',
        widget_layout,
    )
    assert controls == ["Previous", "Play", "Pause", "Stop", "Next"]

    for view_id, media_action in (
        ("previous", "CONTROL_PREVIOUS"),
        ("play", "CONTROL_PLAY"),
        ("pause", "CONTROL_PAUSE"),
        ("stop", "CONTROL_STOP"),
        ("next", "CONTROL_NEXT"),
    ):
        assert re.search(
            rf"setOnClickPendingIntent\s*\(\s*resources\.{view_id}\s*,"
            rf"\s*controlPendingIntent\s*\(\s*context\s*,"
            rf"\s*XmmsPlaybackService\.{media_action}\s*\)\s*\)",
            widget_source,
        )

    apk = Path(__file__).resolve().parents[1] / "target/debug/apk/xmms-renascene.apk"
    with zipfile.ZipFile(apk) as package:
        packaged_resources = set(package.namelist())
        assert "res/drawable/widget_icon.png" in packaged_resources
        assert (
            "res/drawable-nodpi-v4/widget_player_preview.png"
            in packaged_resources
        )
        assert (
            "res/drawable-nodpi-v4/widget_player_info_preview.png"
            in packaged_resources
        )
        assert "res/layout/widget_player_preview.xml" in packaged_resources
        assert "res/layout/widget_player_info_preview.xml" in packaged_resources
        native_library = next(
            name
            for name in package.namelist()
            if name.startswith("lib/") and name.endswith("/libxmms_renascene.so")
        )
        library_bytes = package.read(native_library)
    assert (
        b"Java_org_xmms_renascene_XmmsPlayerWidget_nativeRenderPlayerWidget"
        in library_bytes
    )
    assert (
        b"Java_org_xmms_renascene_XmmsPlayerInfoWidget_nativeRenderPlayerInfoWidget"
        in library_bytes
    )
    assert (
        b"Java_org_xmms_renascene_XmmsPlayerInfoWidget_nativeUpdateTitleMarquee"
        in library_bytes
    )
