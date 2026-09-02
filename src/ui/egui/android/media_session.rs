//! Playback-service and media-browser boundary.
//!
//! `MEDIA_PLAYLIST` explicitly records whether its playlist is the foreground
//! Activity's read-only mirror or the paused/absent Activity's authoritative
//! service state. Actual Activity lifecycle callbacks drive that transition;
//! repaint registration is deliberately not consulted as a liveness proxy.
//! `PLAYBACK_BACKEND` is process-wide so Activity and service callbacks use one
//! backend. Service transport JNI mutates the backend and playlist only in
//! authoritative mode. Foreground controls are queued for egui, while paused
//! controls execute immediately and are tagged `backend_executed` so a resumed
//! Activity applies only the matching domain transition.
//! Media-browser JNI queries are synchronous read-only endpoints required by
//! `MediaBrowserService`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use jni::objects::{JObject, JValue};
use jni::JNIEnv;

use crate::playback::backend::PlaybackBackend;
use crate::playback::model::{PlaybackEvent, PlayerAction, PlayerTransition};
use crate::playback::rodio::RodioBackend;
use crate::playlist::{Playlist, TrackDirection};
use crate::session::{fallback_state_paths, load_saved_state};

use super::super::android_media::{
    AndroidActivityGeneration, AndroidAuthoritativeMediaPlaylist, AndroidMediaPlaylist,
    AndroidMediaPlaylistAuthority, AndroidMediaPlaylistState,
};
use super::activity;
use super::events::{
    self, AndroidMediaControl, AndroidMediaControlEvent, AndroidPlatformEvent, AndroidPlaybackState,
};
use super::persistence;

#[derive(Debug, Clone, PartialEq, Eq)]
struct AndroidMediaNotification {
    state: AndroidPlaybackState,
    title: String,
    bitrate: i32,
    frequency: i32,
    channels: i32,
    duration_ms: i64,
    current_index: i64,
    playlist_len: i32,
    queue_version: u64,
    has_previous: bool,
    has_next: bool,
}

static MEDIA_NOTIFICATION: OnceLock<Mutex<Option<AndroidMediaNotification>>> = OnceLock::new();
static MEDIA_PLAYLIST: OnceLock<Mutex<AndroidMediaPlaylistState>> = OnceLock::new();
static PLAYBACK_BACKEND: OnceLock<Mutex<Option<RodioBackend>>> = OnceLock::new();
static LAST_POSITION_PUBLICATION: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

const POSITION_PUBLICATION_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) fn reset_notification() {
    *MEDIA_NOTIFICATION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = None;
    *LAST_POSITION_PUBLICATION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = None;
}

pub fn shared_playback_backend() -> Result<RodioBackend, String> {
    let mut backend = PLAYBACK_BACKEND
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(backend) = backend.as_ref() {
        return Ok(backend.clone());
    }
    let created = RodioBackend::new()?;
    *backend = Some(created.clone());
    Ok(created)
}

pub(crate) fn existing_playback_backend() -> Option<RodioBackend> {
    PLAYBACK_BACKEND
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

pub(crate) fn replace_activity(activity: AndroidActivityGeneration, resumed: bool) {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .replace_activity(activity, resumed);
}

pub(crate) fn activity_resumed(activity: AndroidActivityGeneration) {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .activity_resumed(activity);
}

pub(crate) fn activity_paused_or_exited(activity: AndroidActivityGeneration) {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .activity_paused_or_exited(activity);
}

pub(crate) fn persist_playback_position_now() {
    let Some(backend) = existing_playback_backend() else {
        return;
    };
    if let Err(err) =
        persistence::persist_playback_position_now(&backend, current_media_item_index())
    {
        eprintln!("xmms-rs: failed to persist Android lifecycle position: {err}");
    }
}

pub(crate) fn is_foreground_mirror(activity: AndroidActivityGeneration) -> bool {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .is_mirror_for(activity)
}

pub fn sync_media_playlist(
    activity: AndroidActivityGeneration,
    playlist: &Playlist,
    titles: Vec<String>,
) -> bool {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .sync_mirror(
            activity,
            AndroidMediaPlaylist::new(playlist.clone(), titles),
        )
}

pub fn sync_media_playlist_position(
    activity: AndroidActivityGeneration,
    position: Option<usize>,
) -> bool {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .sync_mirror_position(activity, position)
}

#[allow(clippy::too_many_arguments)]
pub fn update_playback_notification(
    activity: AndroidActivityGeneration,
    state: AndroidPlaybackState,
    title: &str,
    bitrate: i32,
    frequency: i32,
    channels: i32,
    duration_ms: i64,
    position_ms: i64,
    current_index: i64,
    playlist_len: i32,
    queue_version: u64,
    has_previous: bool,
    has_next: bool,
) -> Result<bool, String> {
    if !is_foreground_mirror(activity) {
        return Ok(false);
    }
    let Some(context) = activity::context_for_generation(activity) else {
        return Ok(false);
    };
    let Some(context) = context.as_ref().filter(|context| context.resumed) else {
        return Ok(false);
    };
    let notification = AndroidMediaNotification {
        state,
        title: title.to_string(),
        bitrate,
        frequency,
        channels,
        duration_ms,
        current_index,
        playlist_len,
        queue_version,
        has_previous,
        has_next,
    };
    let mut previous = MEDIA_NOTIFICATION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if previous.as_ref() == Some(&notification) {
        return Ok(true);
    }

    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android media-control thread: {err}"))?;
    let title = env
        .new_string(title)
        .map_err(|err| format!("failed to create Android media title: {err}"))?;
    env.call_method(
        context.activity.as_obj(),
        "updatePlaybackNotification",
        "(ILjava/lang/String;IIIJJJIZZ)V",
        &[
            JValue::Int(state as i32),
            JValue::Object(&title),
            JValue::Int(bitrate),
            JValue::Int(frequency),
            JValue::Int(channels),
            JValue::Long(duration_ms),
            JValue::Long(position_ms),
            JValue::Long(current_index),
            JValue::Int(playlist_len),
            JValue::Bool(has_previous.into()),
            JValue::Bool(has_next.into()),
        ],
    )
    .map_err(|err| format!("failed to update Android playback notification: {err}"))?;
    *previous = Some(notification);
    Ok(true)
}

pub fn complete_media_control(activity: AndroidActivityGeneration) -> Result<(), String> {
    let Some(context) = activity::context_for_generation(activity) else {
        return Ok(());
    };
    let Some(context) = context.as_ref() else {
        return Ok(());
    };
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android media-control thread: {err}"))?;
    env.call_method(
        context.activity.as_obj(),
        "completeMediaControl",
        "()V",
        &[],
    )
    .map_err(|err| format!("failed to complete Android media control: {err}"))?;
    Ok(())
}

pub(crate) fn initialize_media_library(files_dir: PathBuf, cache_dir: PathBuf) {
    let _order = events::lock_media_control_order();
    initialize_media_library_locked(files_dir, cache_dir);
}

pub(crate) fn initialize_media_library_locked(files_dir: PathBuf, cache_dir: PathBuf) {
    std::env::set_var("XMMS_RS_CONFIG_DIR", files_dir.join("config"));
    std::env::set_var("XMMS_RS_CACHE_DIR", &cache_dir);
    let media_playlist =
        MEDIA_PLAYLIST.get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()));
    if !matches!(
        media_playlist
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .authority(),
        AndroidMediaPlaylistAuthority::Uninitialized
    ) {
        return;
    }

    let (config_path, playlist_path) = fallback_state_paths(&files_dir.join("config"));
    let media = match load_saved_state(&config_path, &playlist_path, false) {
        Ok(state) => {
            let titles = state
                .playlist
                .entries()
                .iter()
                .map(|entry| media_item_title(&entry.title, &entry.filename))
                .collect();
            AndroidMediaPlaylist::new(state.playlist, titles)
        }
        Err(err) => {
            eprintln!("xmms-rs: failed to load Android Auto media library: {err}");
            AndroidMediaPlaylist::new(Playlist::new(), Vec::new())
        }
    };
    media_playlist
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .initialize_authoritative(media);
}

pub(crate) fn media_item_count() -> i32 {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .media()
        .map_or(0, |shared| {
            shared.playlist.len().min(i32::MAX as usize) as i32
        })
}

pub(crate) fn media_item_title_at(index: usize) -> Option<String> {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .media()
        .and_then(|shared| {
            let entry = shared.playlist.entries().get(index)?;
            Some(
                shared
                    .titles
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| media_item_title(&entry.title, &entry.filename)),
            )
        })
}

pub(crate) fn media_item_duration_ms(index: usize) -> Option<i64> {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .media()
        .and_then(|shared| shared.playlist.entries().get(index))
        .map(|entry| entry.length_ms)
}

pub(crate) fn current_media_item_index() -> Option<usize> {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .media()
        .and_then(|shared| shared.playlist.position())
}

pub(crate) fn media_queue_snapshot_json() -> String {
    let state = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let Some(media) = state.media() else {
        return r#"{"version":0,"items":[]}"#.to_string();
    };
    let items = media
        .playlist
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            serde_json::json!({
                "title": media.titles.get(index).unwrap_or(&entry.title),
                "durationMs": entry.length_ms,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "version": media.playlist.revisions().content,
        "items": items,
    })
    .to_string()
}

pub(crate) fn poll_playback(env: &mut JNIEnv<'_>, service: &JObject<'_>) {
    let Some(backend) = existing_playback_backend() else {
        return;
    };
    let order = events::lock_media_control_order();
    let Ok(playback_events) = backend.poll_events() else {
        return;
    };
    let mut eof = false;
    let mut refresh_state = false;
    let mut queued = Vec::new();
    for event in playback_events {
        if matches!(event, PlaybackEvent::EndOfStream) {
            eof = true;
        } else {
            refresh_state |= matches!(
                event,
                PlaybackEvent::DurationChanged(_)
                    | PlaybackEvent::StreamInfo(_)
                    | PlaybackEvent::AsyncDone
            );
            queued.push(AndroidPlatformEvent::Playback(event));
        }
    }
    events::push_all(queued);
    let backend_executed = eof.then(|| {
        let mut state = MEDIA_PLAYLIST
            .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let backend_executed = match state.authority() {
            AndroidMediaPlaylistAuthority::Mirror(_) => false,
            AndroidMediaPlaylistAuthority::Authoritative => {
                let result = state
                    .authoritative_mut()
                    .expect("authoritative state exposes mutation capability")
                    .advance_after_end_of_stream(|uri| backend.play_uri(uri), || backend.stop());
                if let Err(err) = result {
                    eprintln!("xmms-rs: Android background playlist advance failed: {err}");
                    let _ = backend.stop();
                }
                true
            }
            AndroidMediaPlaylistAuthority::Uninitialized => {
                let _ = backend.stop();
                true
            }
        };
        events::push(AndroidPlatformEvent::MediaControl(
            AndroidMediaControlEvent {
                control: AndroidMediaControl::PlaylistEof,
                backend_executed,
                complete_activity_control: false,
            },
        ));
        backend_executed
    });
    drop(order);
    if backend_executed == Some(true) || (!eof && refresh_state) {
        update_service_from_backend(env, service);
    }
    persistence::checkpoint_playback_position(&backend, current_media_item_index);
    update_service_position_if_due(env, service, &backend);
}

pub(crate) fn handle_media_control(
    mut env: JNIEnv<'_>,
    service: Option<JObject<'_>>,
    control: AndroidMediaControl,
) {
    let _order = events::lock_media_control_order();
    let backend_executed = {
        let mut state = MEDIA_PLAYLIST
            .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match state.authority() {
            AndroidMediaPlaylistAuthority::Mirror(_) => false,
            AndroidMediaPlaylistAuthority::Authoritative => {
                let backend = match shared_playback_backend() {
                    Ok(backend) => backend,
                    Err(err) => {
                        eprintln!("xmms-rs: Android media control failed: {err}");
                        return;
                    }
                };
                let result = execute_android_media_control(
                    control,
                    &backend,
                    &mut state
                        .authoritative_mut()
                        .expect("authoritative state exposes mutation capability"),
                );
                if let Err(err) = result {
                    eprintln!("xmms-rs: Android media control failed: {err}");
                    return;
                }
                true
            }
            AndroidMediaPlaylistAuthority::Uninitialized => {
                eprintln!("xmms-rs: Android media playlist is unavailable");
                return;
            }
        }
    };
    events::push(AndroidPlatformEvent::MediaControl(
        AndroidMediaControlEvent {
            control,
            backend_executed,
            complete_activity_control: false,
        },
    ));
    if backend_executed {
        if let Some(service) = service {
            update_service_from_backend(&mut env, &service);
        }
    }
    events::request_registered_repaint();
}

fn execute_android_media_control(
    control: AndroidMediaControl,
    backend: &RodioBackend,
    playlist: &mut AndroidAuthoritativeMediaPlaylist<'_>,
) -> Result<(), String> {
    match control {
        AndroidMediaControl::PausePlayback => {
            match backend.state().transition(PlayerAction::Pause) {
                Some(PlayerTransition::Pause) => backend.pause(),
                _ => Ok(()),
            }
        }
        AndroidMediaControl::ResumePlayback => {
            match backend.state().transition(PlayerAction::Play) {
                Some(PlayerTransition::Resume) => backend.unpause(),
                Some(PlayerTransition::Start) => {
                    playlist.start_current(|uri| backend.play_uri(uri))
                }
                _ => Ok(()),
            }
        }
        AndroidMediaControl::NextTrack => playlist.change_track(
            TrackDirection::Next,
            |uri| backend.play_uri(uri),
            || backend.seek(0),
        ),
        AndroidMediaControl::PreviousTrack => playlist.change_track(
            TrackDirection::Previous,
            |uri| backend.play_uri(uri),
            || backend.seek(0),
        ),
        AndroidMediaControl::SeekToMs(position_ms) => backend.seek(position_ms),
        AndroidMediaControl::PlayMediaItem(index) => {
            playlist.play_media_item(index, |uri| backend.play_uri(uri))
        }
        AndroidMediaControl::HaltPlayback => match backend.state().transition(PlayerAction::Halt) {
            Some(PlayerTransition::Stop) => backend.stop(),
            _ => Ok(()),
        },
        AndroidMediaControl::PlaylistEof => {
            playlist.advance_after_end_of_stream(|uri| backend.play_uri(uri), || backend.stop())
        }
    }
}

fn media_item_title(title: &str, filename: &str) -> String {
    if !title.trim().is_empty() {
        return title.to_string();
    }
    Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Unknown track")
        .to_string()
}

fn current_media_entry() -> Option<(String, String, i64)> {
    let shared = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    shared.media()?.current_entry()
}

fn update_service_position_from_backend(
    env: &mut JNIEnv<'_>,
    service: &JObject<'_>,
    backend: &RodioBackend,
) {
    let Some(position_ms) = backend.position_ms().map(|position| position.max(0)) else {
        return;
    };
    let _ = env.call_method(
        service,
        "applyNativePlaybackPosition",
        "(J)V",
        &[JValue::Long(position_ms)],
    );
}

fn update_service_position_if_due(
    env: &mut JNIEnv<'_>,
    service: &JObject<'_>,
    backend: &RodioBackend,
) {
    let now = Instant::now();
    let mut last = LAST_POSITION_PUBLICATION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if last.is_some_and(|last| now.saturating_duration_since(last) < POSITION_PUBLICATION_INTERVAL)
    {
        return;
    }
    update_service_position_from_backend(env, service, backend);
    *last = Some(now);
}

fn update_service_from_backend(env: &mut JNIEnv<'_>, service: &JObject<'_>) {
    let Some(backend) = existing_playback_backend() else {
        return;
    };
    let state = AndroidPlaybackState::from(backend.state());
    let (_, title, entry_duration_ms) = current_media_entry().unwrap_or_else(|| {
        (
            String::new(),
            "XMMS Renascene".to_string(),
            backend.duration_ms().unwrap_or(-1),
        )
    });
    let duration_ms = (entry_duration_ms >= 0)
        .then_some(entry_duration_ms)
        .or_else(|| backend.duration_ms())
        .unwrap_or(-1);
    let position_ms = backend.position_ms().unwrap_or(0).max(0);
    let stream_info = backend.stream_info();
    let (current_index, playlist_len) = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(AndroidMediaPlaylistState::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .media()
        .map_or((-1, 0), |shared| {
            (
                shared.playlist.position().map_or(-1, |index| index as i64),
                shared.playlist.len().min(i32::MAX as usize) as i32,
            )
        });
    let has_entries = playlist_len > 0;
    let Ok(title) = env.new_string(title) else {
        return;
    };
    let _ = env.call_method(
        service,
        "applyNativePlaybackState",
        "(ILjava/lang/String;IIIJJJIZZ)V",
        &[
            JValue::Int(state as i32),
            JValue::Object(&title),
            JValue::Int(stream_info.bitrate.unwrap_or_default()),
            JValue::Int(stream_info.frequency.unwrap_or_default()),
            JValue::Int(stream_info.channels.unwrap_or_default()),
            JValue::Long(duration_ms),
            JValue::Long(position_ms),
            JValue::Long(current_index),
            JValue::Int(playlist_len),
            JValue::Bool(has_entries.into()),
            JValue::Bool(has_entries.into()),
        ],
    );
}
