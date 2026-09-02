//! Playback backend and visualization runtime for the egui frontend.

use crate::app_log_info;
use crate::playback::backend::PlaybackBackend;
#[cfg(all(not(test), not(target_os = "android")))]
use crate::playback::backend::{create_backend, PlaybackBackendKind};
use crate::playback::model::EqualizerBackendState;
use crate::skin::widget::{Visualization, WidgetId};

use super::effect_executor::PlaybackEffect;

pub struct PlaybackRuntime {
    pub(crate) backend: Option<Box<dyn PlaybackBackend>>,
    pub(crate) pending_seek_ms: Option<i64>,
    pub(crate) visualization: Visualization,
    pub(crate) visualization_tick_counter: i32,
}

impl PlaybackRuntime {
    pub fn new(backend: Option<Box<dyn PlaybackBackend>>) -> Self {
        Self {
            backend,
            pending_seek_ms: None,
            visualization: Visualization::new(WidgetId(6), 24, 43, 76),
            visualization_tick_counter: 0,
        }
    }

    #[cfg(target_os = "android")]
    pub(crate) fn attach_existing_android_backend(
        &mut self,
        balance: i32,
        equalizer: EqualizerBackendState,
    ) -> Result<bool, String> {
        if self.backend.is_some() {
            return Ok(false);
        }
        let Some(backend) = super::android::existing_playback_backend() else {
            return Ok(false);
        };
        self.install_backend_with_dsp(Box::new(backend), balance, equalizer)?;
        Ok(true)
    }

    pub(crate) fn apply_effect(
        &mut self,
        effect: &PlaybackEffect,
        balance: i32,
        equalizer: EqualizerBackendState,
        execute_backend: bool,
    ) -> Vec<String> {
        if !execute_backend {
            if matches!(
                effect,
                PlaybackEffect::Stop | PlaybackEffect::BeginStopFade { .. }
            ) {
                self.visualization_tick_counter = 0;
                self.visualization.clear_data();
            }
            return Vec::new();
        }
        #[cfg(any(test, not(target_os = "android")))]
        let _ = balance;
        #[cfg(all(not(test), target_os = "android"))]
        if matches!(effect, PlaybackEffect::StartUri { .. }) && self.backend.is_none() {
            match super::android::shared_playback_backend() {
                Ok(backend) => {
                    if let Err(err) =
                        self.install_backend_with_dsp(Box::new(backend), balance, equalizer)
                    {
                        return vec![err];
                    }
                }
                Err(err) => return vec![format!("failed to initialize audio output: {err}")],
            }
        }
        #[cfg(all(not(test), not(target_os = "android")))]
        if matches!(effect, PlaybackEffect::StartUri { .. }) && self.backend.is_none() {
            match create_backend(PlaybackBackendKind::Auto) {
                Ok(backend) => self.backend = Some(backend),
                Err(err) => return vec![format!("failed to initialize audio output: {err}")],
            }
        }

        let mut errors = Vec::new();
        if let Some(backend) = &self.backend {
            let result = match effect {
                PlaybackEffect::StartUri { uri, position_ms } => {
                    let pending_seek = *position_ms > 0;
                    app_log_info!(backend, "egui play_uri", uri, position_ms, pending_seek);
                    let result = backend.play_uri(uri);
                    if result.is_ok() {
                        self.pending_seek_ms = pending_seek.then_some(*position_ms);
                    }
                    result
                }
                PlaybackEffect::Resume => backend.unpause(),
                PlaybackEffect::Pause => backend.pause(),
                PlaybackEffect::Stop | PlaybackEffect::BeginStopFade { .. } => backend.stop(),
                PlaybackEffect::Seek(position_ms) => {
                    app_log_info!(backend, "egui seek", position_ms);
                    backend.seek(*position_ms)
                }
                PlaybackEffect::SetBackendVolume(volume) => backend.set_volume(*volume),
                PlaybackEffect::SetBackendBalance(balance) => backend.set_balance(*balance),
                PlaybackEffect::SetBackendEqualizer => backend.set_equalizer(equalizer),
                PlaybackEffect::Start | PlaybackEffect::StartFromCurrent => Ok(()),
            };
            if let Err(error) = result {
                errors.push(error);
            }
        }

        if matches!(
            effect,
            PlaybackEffect::Stop | PlaybackEffect::BeginStopFade { .. }
        ) {
            self.visualization_tick_counter = 0;
            self.visualization.clear_data();
        }
        errors
    }

    fn install_backend_with_dsp(
        &mut self,
        backend: Box<dyn PlaybackBackend>,
        balance: i32,
        equalizer: EqualizerBackendState,
    ) -> Result<(), String> {
        backend
            .set_balance(balance)
            .map_err(|err| format!("failed to initialize audio balance: {err}"))?;
        backend
            .set_equalizer(equalizer)
            .map_err(|err| format!("failed to initialize audio equalizer: {err}"))?;
        self.backend = Some(backend);
        Ok(())
    }

    pub fn set_output_volume(&self, volume: i32) -> Option<String> {
        self.backend
            .as_ref()
            .and_then(|backend| backend.set_volume(volume).err())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum BackendCall {
        Balance(i32),
        Equalizer(EqualizerBackendState),
        Play,
    }

    struct RecordingBackend {
        calls: Arc<Mutex<Vec<BackendCall>>>,
    }

    impl PlaybackBackend for RecordingBackend {
        fn play_uri(&self, _uri: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(BackendCall::Play);
            Ok(())
        }

        fn pause(&self) -> Result<(), String> {
            Ok(())
        }

        fn unpause(&self) -> Result<(), String> {
            Ok(())
        }

        fn stop(&self) -> Result<(), String> {
            Ok(())
        }

        fn seek(&self, _position_ms: i64) -> Result<(), String> {
            Ok(())
        }

        fn set_volume(&self, _volume: i32) -> Result<(), String> {
            Ok(())
        }

        fn set_balance(&self, balance: i32) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(BackendCall::Balance(balance));
            Ok(())
        }

        fn set_equalizer(&self, equalizer: EqualizerBackendState) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(BackendCall::Equalizer(equalizer));
            Ok(())
        }
    }

    #[test]
    fn lazy_backend_applies_dsp_state_before_first_playback() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let backend = RecordingBackend {
            calls: Arc::clone(&calls),
        };
        let equalizer = EqualizerBackendState {
            active: true,
            preamp_position: 41,
            band_positions: [10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
        };
        let mut runtime = PlaybackRuntime::new(None);

        runtime
            .install_backend_with_dsp(Box::new(backend), -25, equalizer)
            .unwrap();
        assert!(runtime
            .apply_effect(
                &PlaybackEffect::StartUri {
                    uri: "file:///song.ogg".to_string(),
                    position_ms: 0,
                },
                -25,
                equalizer,
                true,
            )
            .is_empty());

        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                BackendCall::Balance(-25),
                BackendCall::Equalizer(equalizer),
                BackendCall::Play,
            ]
        );
    }
}

impl PlaybackRuntime {
    pub fn position_ms(&self) -> Option<i64> {
        self.backend
            .as_ref()
            .and_then(|backend| backend.position_ms())
    }
}
