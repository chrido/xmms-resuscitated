//! Android platform boundary for the egui frontend.
//!
//! The module layout is responsibility-oriented:
//! - `android_media` owns the target-neutral playlist authority machine and
//!   deterministic transition tests.
//! - [`activity`] owns the replaceable `NativeActivity` JNI reference.
//! - [`events`] owns ordered process-wide ingress and repaint registration.
//! - [`picker`] owns SAF requests and activity-generation operation tokens.
//! - [`layout`] validates the compact Java window snapshot.
//! - [`media_session`] owns the service projection, shared backend, and the
//!   deliberate activity-absent playback fallback.
//! - [`audio_focus`] contains activity-side media-volume access; Android audio
//!   focus itself is owned exclusively by `XmmsPlaybackService`.
//! - [`persistence`] serializes in-process writes; snapshot replacement is
//!   atomic so Activity-absent readers and process recreation never see a
//!   partially written file.
//! - [`widgets`] owns synchronous widget rendering caches.
//! - [`jni_bridge`] contains JNI ABI adapters and documents the allowed exceptions.
//!
//! Activity callbacks run on the Android main thread and only validate, convert,
//! enqueue, or request repaint. The egui thread drains a bounded FIFO batch once
//! at the beginning of each frame, before local input is dispatched. Service
//! transport callbacks execute the shared backend synchronously only while the
//! explicit media-playlist state is authoritative. Activity resume/pause,
//! replacement, destruction, and egui exit drive that state; a registered
//! repaint context is only a callback target and never a liveness signal.
//! Media-browser queries and widget rendering are synchronous because Android
//! requires an immediate return value.
//!
//! The generated manifest currently assigns no `android:process`, so Activity,
//! service, and widgets normally share one Linux process. Android may still
//! create those components without an Activity and may kill/recreate that
//! process at any time; durable coordination therefore uses atomically replaced
//! files rather than relying on these process-local registries.

use std::sync::MutexGuard;

use jni::objects::JObject;
use jni::JNIEnv;

mod activity;
mod audio_focus;
mod events;
#[path = "jni.rs"]
mod jni_bridge;
mod layout;
mod media_session;
mod persistence;
mod picker;
pub(crate) mod playlist_manager;
mod widgets;

pub(crate) use super::android_media::AndroidActivityGeneration;
pub use audio_focus::{media_volume_percent, set_media_volume_percent};
pub use events::{
    AndroidMediaControl, AndroidMediaControlEvent, AndroidPickerResult, AndroidPlatformEvent,
    AndroidPlaybackState,
};
pub use layout::window_layout_snapshot_pixels;
pub(crate) use media_session::existing_playback_backend;
pub use media_session::{
    complete_media_control, shared_playback_backend, sync_media_playlist,
    sync_media_playlist_position, update_playback_notification,
};
pub use persistence::{
    flush_persistence_writer, persist_app_state, persist_playback_position, take_persistence_error,
};
pub use picker::{open, save_equalizer_preset};
pub use widgets::refresh_player_widgets;

pub struct AndroidPlatformEventBatch {
    _order: MutexGuard<'static, ()>,
    events: std::vec::IntoIter<AndroidPlatformEvent>,
}

impl Iterator for AndroidPlatformEventBatch {
    type Item = AndroidPlatformEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.events.next()
    }
}

pub fn initialize(
    app: &winit::platform::android::activity::AndroidApp,
) -> Result<AndroidActivityGeneration, String> {
    let _order = events::lock_media_control_order();
    let initialized = activity::initialize(app)?;
    media_session::initialize_media_library_locked(initialized.files_dir, initialized.cache_dir);
    events::replace_activity();
    media_session::replace_activity(initialized.generation, initialized.resumed);
    media_session::reset_notification();
    picker::replace_activity();
    Ok(initialized.generation)
}

pub fn drain_platform_events(
    activity_generation: AndroidActivityGeneration,
    context: &egui::Context,
) -> Option<AndroidPlatformEventBatch> {
    let order = events::lock_media_control_order();
    if !activity::is_current(activity_generation)
        || !media_session::is_foreground_mirror(activity_generation)
    {
        return None;
    }
    events::register_repaint_context(activity_generation, context);
    Some(AndroidPlatformEventBatch {
        _order: order,
        events: events::drain_platform_events().into_iter(),
    })
}

pub fn begin_local_media_control(
    activity_generation: AndroidActivityGeneration,
) -> Option<MutexGuard<'static, ()>> {
    let order = events::lock_media_control_order();
    if !activity::is_current(activity_generation)
        || !media_session::is_foreground_mirror(activity_generation)
    {
        return None;
    }
    events::remove_unexecuted_media_controls();
    Some(order)
}

pub fn is_current_activity(activity_generation: AndroidActivityGeneration) -> bool {
    activity::is_current(activity_generation)
}

pub fn is_foreground_activity(activity_generation: AndroidActivityGeneration) -> bool {
    activity::is_current(activity_generation)
        && media_session::is_foreground_mirror(activity_generation)
}

pub fn runtime_exited(activity_generation: AndroidActivityGeneration) {
    let _order = events::lock_media_control_order();
    events::unregister_repaint_context(activity_generation);
    if !activity::is_current(activity_generation) {
        return;
    }
    media_session::activity_paused_or_exited(activity_generation);
    picker::replace_activity();
    activity::clear_generation(activity_generation);
}

pub(crate) fn request_background_repaint() {
    events::request_registered_repaint();
}

pub(crate) fn handle_activity_resumed(env: &mut JNIEnv<'_>, activity_object: &JObject<'_>) {
    let _order = events::lock_media_control_order();
    let Some(activity_generation) = activity::set_resumed(env, activity_object, true) else {
        return;
    };
    media_session::activity_resumed(activity_generation);
}

pub(crate) fn handle_activity_paused(env: &mut JNIEnv<'_>, activity_object: &JObject<'_>) {
    let _order = events::lock_media_control_order();
    let Some(activity_generation) = activity::set_resumed(env, activity_object, false) else {
        return;
    };
    media_session::persist_playback_position_now();
    media_session::activity_paused_or_exited(activity_generation);
}

pub(crate) fn handle_activity_destroyed(env: &mut JNIEnv<'_>, activity_object: &JObject<'_>) {
    let _order = events::lock_media_control_order();
    let Some(activity_generation) = activity::destroy_current(env, activity_object) else {
        return;
    };
    media_session::activity_paused_or_exited(activity_generation);
    events::unregister_repaint_context(activity_generation);
    picker::replace_activity();
}

pub(crate) fn handle_activity_media_control(
    env: &mut JNIEnv<'_>,
    activity_object: &JObject<'_>,
    control: AndroidMediaControl,
) {
    let _order = events::lock_media_control_order();
    let Some((activity_generation, true)) = activity::generation_for_object(env, activity_object)
    else {
        return;
    };
    if !media_session::is_foreground_mirror(activity_generation) {
        return;
    }
    events::push(AndroidPlatformEvent::MediaControl(
        AndroidMediaControlEvent {
            control,
            backend_executed: false,
            complete_activity_control: true,
        },
    ));
    events::request_registered_repaint();
}

pub(crate) fn activity_callback_is_current(
    env: &mut JNIEnv<'_>,
    activity_object: &JObject<'_>,
) -> bool {
    let _order = events::lock_media_control_order();
    activity::is_current_object(env, activity_object)
}

pub(crate) fn request_activity_repaint(env: &mut JNIEnv<'_>, activity_object: &JObject<'_>) {
    let _order = events::lock_media_control_order();
    if activity::is_current_object(env, activity_object) {
        events::request_registered_repaint();
    }
}
