//! Frontend-neutral application controller.
//!
//! The controller owns application state transitions. It remains free of GTK
//! widgets, platform windows, and concrete backend objects.

use crate::app::command::{
    AppCommand, AudioCommand, EqualizerCommand, PanelCommand, PlayerCommand, PlaylistCommand,
    UiCommand,
};
use crate::app::effect::{AppEffect, FileDialogRequest, RenderTarget};
use crate::app::playlist_actions::PlaylistMenuCommand;
use crate::app_state::AppState;
use crate::player::{PlaybackEvent, PlayerAction, PlayerTransition};
use crate::playlist::{Playlist, TrackDirection};

#[derive(Debug, Clone)]
pub(super) struct AppController {
    state: AppState,
}

impl AppController {
    pub(super) fn new(state: AppState) -> Self {
        Self { state }
    }

    pub(super) fn state(&self) -> &AppState {
        &self.state
    }

    /// Mutable state access is confined to the controller/store implementation.
    pub(super) fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub(super) fn into_state(self) -> AppState {
        self.state
    }

    pub(super) fn handle_command(&mut self, command: AppCommand) -> Vec<AppEffect> {
        match command {
            AppCommand::Player(command) => self.handle_player_command(command),
            AppCommand::Audio(command) => self.handle_audio_command(command),
            AppCommand::Playlist(command) => self.handle_playlist_command(command),
            AppCommand::Equalizer(command) => self.handle_equalizer_command(command),
            AppCommand::Panel(command) => self.handle_panel_command(command),
            AppCommand::Ui(command) => self.handle_ui_command(command),
        }
    }

    fn handle_player_command(&mut self, command: PlayerCommand) -> Vec<AppEffect> {
        match command {
            PlayerCommand::Play => self.play(),
            PlayerCommand::StartCurrentTrack => self.start_current_playlist_playback(0),
            PlayerCommand::Pause => self.pause(),
            PlayerCommand::Halt => self.halt(),
            PlayerCommand::PlayPause => self.play_pause(),
            PlayerCommand::PreviousTrack => self.change_track(TrackDirection::Previous),
            PlayerCommand::NextTrack => self.change_track(TrackDirection::Next),
            PlayerCommand::SeekToMs(position_ms) => self.seek_to(position_ms),
        }
    }

    fn handle_audio_command(&mut self, command: AudioCommand) -> Vec<AppEffect> {
        match command {
            AudioCommand::SetVolume(volume) => {
                self.state.player.set_volume(volume);
                vec![
                    AppEffect::SetOutputVolume(self.state.player.volume()),
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
            AudioCommand::SetBalance(balance) => {
                self.state.player.set_balance(balance);
                vec![
                    AppEffect::SetBackendBalance(self.state.player.balance()),
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
        }
    }

    fn handle_equalizer_command(&mut self, command: EqualizerCommand) -> Vec<AppEffect> {
        match command {
            EqualizerCommand::SetActive(active) => self.state.config.equalizer_active = active,
            EqualizerCommand::ToggleActive => {
                self.state.config.equalizer_active = !self.state.config.equalizer_active;
            }
            EqualizerCommand::SetAuto(auto) => self.state.config.equalizer_auto = auto,
            EqualizerCommand::ToggleAuto => {
                self.state.config.equalizer_auto = !self.state.config.equalizer_auto;
            }
            EqualizerCommand::SetPreamp(position) => {
                self.state.config.equalizer_preamp_pos = position.clamp(0, 100);
            }
            EqualizerCommand::SetBand { band, position } => {
                if let Some(value) = self.state.config.equalizer_band_pos.get_mut(band) {
                    *value = position.clamp(0, 100);
                }
            }
        }
        vec![
            AppEffect::SetBackendEqualizer,
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::Equalizer),
        ]
    }

    fn handle_playlist_command(&mut self, command: PlaylistCommand) -> Vec<AppEffect> {
        match command {
            PlaylistCommand::ToggleShuffle => {
                self.state
                    .playlist
                    .set_shuffle(!self.state.playlist.shuffle());
                self.playlist_changed_effects()
            }
            PlaylistCommand::ToggleRepeat => {
                self.state
                    .playlist
                    .set_repeat(!self.state.playlist.repeat());
                self.playlist_changed_effects()
            }
            PlaylistCommand::ToggleNoAdvance => {
                self.state
                    .playlist
                    .set_no_advance(!self.state.playlist.no_advance());
                self.playlist_changed_effects()
            }
            PlaylistCommand::SetSize { .. } => Vec::new(),
            PlaylistCommand::ExecuteMenu { kind, index } => self.execute_playlist_menu(kind, index),
            PlaylistCommand::Sort(key) => {
                self.state.playlist.sort_by(key);
                self.playlist_changed_effects()
            }
            PlaylistCommand::SortSelected(key) => {
                self.state.playlist.sort_selected_by(key);
                self.playlist_changed_effects()
            }
            PlaylistCommand::Reverse => {
                self.state.playlist.reverse();
                self.playlist_changed_effects()
            }
            PlaylistCommand::Randomize => {
                self.state.playlist.randomize();
                self.playlist_changed_effects()
            }
            PlaylistCommand::AddUris(uris) => {
                for uri in uris {
                    self.state.playlist.add_uri(uri);
                }
                self.playlist_changed_effects()
            }
            PlaylistCommand::AddLocations(locations) => {
                let mut added = false;
                for location in locations {
                    match self.state.playlist.add_location(&location) {
                        Ok(count) => added |= count > 0,
                        Err(err) => {
                            return vec![AppEffect::ShowError(format!(
                                "failed to add playlist location {location}: {err}"
                            ))];
                        }
                    }
                }
                if added {
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
            PlaylistCommand::AddFiles(paths) => {
                for path in paths {
                    let _ = self.state.playlist.add_path_or_directory(&path);
                }
                self.playlist_changed_effects()
            }
            PlaylistCommand::Clear => self.clear_playlist(),
            PlaylistCommand::RemoveSelectedOrCurrent => {
                self.state.playlist.remove_selected_or_current();
                self.playlist_changed_effects()
            }
            PlaylistCommand::RemoveSelected => {
                if self.state.playlist.remove_selected() {
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
            PlaylistCommand::CropToSelection => {
                self.state.playlist.crop_to_selected_or_current();
                self.playlist_changed_effects()
            }
            PlaylistCommand::RemoveDead => {
                self.state.playlist.remove_dead_files();
                self.playlist_changed_effects()
            }
            PlaylistCommand::PhysicallyDeleteSelected => {
                match self.state.playlist.physically_delete_selected() {
                    Ok(_) => self.playlist_changed_effects(),
                    Err(err) => vec![AppEffect::ShowError(format!(
                        "failed to delete selected playlist files: {err}"
                    ))],
                }
            }
            PlaylistCommand::SelectAll => {
                self.state.playlist.select_all(true);
                self.playlist_changed_effects()
            }
            PlaylistCommand::SelectNone => {
                self.state.playlist.select_all(false);
                self.playlist_changed_effects()
            }
            PlaylistCommand::InvertSelection => {
                self.state.playlist.invert_selection();
                self.playlist_changed_effects()
            }
            PlaylistCommand::Enqueue(index) => {
                self.update_queue(|playlist| playlist.enqueue(index))
            }
            PlaylistCommand::Dequeue(index) => {
                self.update_queue(|playlist| playlist.dequeue(index))
            }
            PlaylistCommand::ToggleQueue(indices) => {
                self.update_queue(|playlist| playlist.toggle_queue(&indices))
            }
            PlaylistCommand::ClearQueue => self.update_queue(Playlist::clear_queue),
            PlaylistCommand::SetPosition(index) => {
                self.state.playlist.set_position(index);
                self.playlist_changed_effects()
            }
            PlaylistCommand::ToggleEntrySelection(index) => {
                if let Some(entry) = self.state.playlist.entries_mut().get_mut(index) {
                    entry.selected = !entry.selected;
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
            PlaylistCommand::MoveEntry { from, to } => {
                if self.state.playlist.move_entry(from, to) {
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
            PlaylistCommand::UpdateTitleForUri { uri, title } => {
                let title = title.trim();
                if title.is_empty() {
                    return Vec::new();
                }
                let mut changed = false;
                for entry in self.state.playlist.entries_mut() {
                    if entry.filename == uri && entry.title != title {
                        entry.title = title.to_string();
                        changed = true;
                    }
                }
                if changed {
                    self.playlist_changed_effects()
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn handle_panel_command(&mut self, command: PanelCommand) -> Vec<AppEffect> {
        match command {
            PanelCommand::ToggleMainShade => {
                self.state.config.main_shaded = !self.state.config.main_shaded;
                self.panel_changed_effects()
            }
            PanelCommand::SetMainShade(shaded) => {
                self.state.config.main_shaded = shaded;
                self.panel_changed_effects()
            }
            PanelCommand::TogglePlaylistVisibility => {
                self.state.config.playlist_visible = !self.state.config.playlist_visible;
                self.panel_changed_effects()
            }
            PanelCommand::SetPlaylistVisibility(visible) => {
                self.state.config.playlist_visible = visible;
                self.panel_changed_effects()
            }
            PanelCommand::TogglePlaylistShade => {
                self.state.config.playlist_shaded = !self.state.config.playlist_shaded;
                self.panel_changed_effects()
            }
            PanelCommand::SetPlaylistShade(shaded) => {
                self.state.config.playlist_shaded = shaded;
                self.panel_changed_effects()
            }
            PanelCommand::TogglePlaylistDetached => {
                self.state.config.playlist_detached = !self.state.config.playlist_detached;
                self.panel_changed_effects()
            }
            PanelCommand::SetPlaylistDetached(detached) => {
                self.state.config.playlist_detached = detached;
                self.panel_changed_effects()
            }
            PanelCommand::ToggleEqualizerVisibility => {
                self.state.config.equalizer_visible = !self.state.config.equalizer_visible;
                self.panel_changed_effects()
            }
            PanelCommand::SetEqualizerVisibility(visible) => {
                self.state.config.equalizer_visible = visible;
                self.panel_changed_effects()
            }
            PanelCommand::ToggleEqualizerShade => {
                self.state.config.equalizer_shaded = !self.state.config.equalizer_shaded;
                self.panel_changed_effects()
            }
            PanelCommand::SetEqualizerShade(shaded) => {
                self.state.config.equalizer_shaded = shaded;
                self.panel_changed_effects()
            }
            PanelCommand::ToggleEqualizerDetached => {
                self.state.config.equalizer_detached = !self.state.config.equalizer_detached;
                self.panel_changed_effects()
            }
            PanelCommand::SetEqualizerDetached(detached) => {
                self.state.config.equalizer_detached = detached;
                self.panel_changed_effects()
            }
        }
    }

    fn handle_ui_command(&mut self, command: UiCommand) -> Vec<AppEffect> {
        match command {
            UiCommand::SetPreferencesVisible(visible) => {
                self.state.ui.preferences_visible = visible;
            }
            UiCommand::TogglePreferences => {
                self.state.ui.preferences_visible = !self.state.ui.preferences_visible;
            }
            UiCommand::SetMainMenuVisible(visible) => {
                self.state.ui.main_menu_visible = visible;
            }
            UiCommand::SetSkinBrowserVisible(visible) => {
                self.state.ui.skin_browser_visible = visible;
            }
            UiCommand::ToggleSkinBrowser => {
                self.state.ui.skin_browser_visible = !self.state.ui.skin_browser_visible;
            }
            UiCommand::SetFileInfoVisible(visible) => {
                self.state.ui.file_info_visible = visible;
            }
            UiCommand::ToggleFileInfo => {
                self.state.ui.file_info_visible = !self.state.ui.file_info_visible;
            }
        }
        vec![AppEffect::QueueRender(RenderTarget::All)]
    }

    fn execute_playlist_menu(
        &mut self,
        kind: crate::playlist::PlaylistMenuKind,
        index: usize,
    ) -> Vec<AppEffect> {
        let Some(command) = PlaylistMenuCommand::from_menu_item(kind, index) else {
            return Vec::new();
        };
        match command {
            PlaylistMenuCommand::OpenLocationWindow => {
                vec![AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles)]
            }
            PlaylistMenuCommand::OpenDirectoryDialog => vec![AppEffect::OpenFileDialog(
                FileDialogRequest::AddAudioDirectory,
            )],
            PlaylistMenuCommand::OpenFileDialog => {
                vec![AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles)]
            }
            PlaylistMenuCommand::ShowSortMenu => Vec::new(),
            PlaylistMenuCommand::ShowFileInfo => vec![AppEffect::OpenFileInfoDialog],
            PlaylistMenuCommand::OpenOptions => vec![AppEffect::OpenPreferences],
            PlaylistMenuCommand::ClearList => self.clear_playlist(),
            PlaylistMenuCommand::CropToSelection => {
                self.state.playlist.crop_to_selected_or_current();
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::RemoveSelectedOrCurrent => {
                self.state.playlist.remove_selected_or_current();
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::InvertSelection => {
                self.state.playlist.invert_selection();
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::SelectNone => {
                self.state.playlist.select_all(false);
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::SelectAll => {
                self.state.playlist.select_all(true);
                self.playlist_changed_effects()
            }
            PlaylistMenuCommand::SavePlaylist => {
                vec![AppEffect::OpenFileDialog(FileDialogRequest::SavePlaylist)]
            }
            PlaylistMenuCommand::LoadPlaylist => {
                vec![AppEffect::OpenFileDialog(FileDialogRequest::LoadPlaylist)]
            }
        }
    }

    fn playlist_changed_effects(&self) -> Vec<AppEffect> {
        vec![
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::Playlist),
        ]
    }

    fn queue_changed_effects(&self) -> Vec<AppEffect> {
        vec![AppEffect::QueueRender(RenderTarget::Playlist)]
    }

    fn update_queue(&mut self, update: impl FnOnce(&mut Playlist) -> bool) -> Vec<AppEffect> {
        if update(&mut self.state.playlist) { self.queue_changed_effects() } else { Default::default() }
    }

    fn clear_playlist(&mut self) -> Vec<AppEffect> {
        self.state.playlist.clear();
        self.state.player.terminate();
        self.state.player.clear_visualization_data();
        self.state.config.playback_position_ms = 0;
        vec![
            AppEffect::StopPlayback,
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn panel_changed_effects(&self) -> Vec<AppEffect> {
        vec![
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn play(&mut self) -> Vec<AppEffect> {
        self.apply_player_action(PlayerAction::Play)
    }

    fn pause(&mut self) -> Vec<AppEffect> {
        self.apply_player_action(PlayerAction::Pause)
    }

    fn halt(&mut self) -> Vec<AppEffect> {
        self.apply_player_action(PlayerAction::Halt)
    }

    fn play_pause(&mut self) -> Vec<AppEffect> {
        self.apply_player_action(self.state.player.state().play_pause_action())
    }

    fn apply_player_action(&mut self, action: PlayerAction) -> Vec<AppEffect> {
        match self.state.player.state().transition(action) {
            Some(PlayerTransition::Start) => {
                self.start_current_playlist_playback(self.state.config.playback_position_ms.max(0))
            }
            Some(PlayerTransition::Resume) => {
                self.state.player.unpause();
                vec![
                    AppEffect::ResumePlayback,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
            Some(PlayerTransition::Pause) => {
                self.state.player.pause();
                vec![
                    AppEffect::PausePlayback,
                    AppEffect::QueueRender(RenderTarget::All),
                ]
            }
            Some(PlayerTransition::Stop) => self.terminate_playback(),
            None => Vec::new(),
        }
    }

    fn change_track(&mut self, direction: TrackDirection) -> Vec<AppEffect> {
        self.state.config.playback_position_ms = 0;
        let advanced = self.state.playlist.move_track(direction);
        if advanced {
            self.start_current_playlist_playback(0)
        } else {
            vec![
                AppEffect::SeekPlayback(0),
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::All),
            ]
        }
    }

    fn seek_to(&mut self, position_ms: i64) -> Vec<AppEffect> {
        let position_ms = self
            .state
            .player
            .duration_ms()
            .filter(|duration_ms| *duration_ms > 0)
            .map_or(position_ms.max(0), |duration_ms| {
                position_ms.clamp(0, duration_ms)
            });
        self.state.config.playback_position_ms = position_ms;
        vec![
            AppEffect::SeekPlayback(position_ms),
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    fn start_current_playlist_playback(&mut self, position_ms: i64) -> Vec<AppEffect> {
        if self.state.playlist.position().is_none() && !self.state.playlist.is_empty() {
            self.state.playlist.set_position(0);
        }
        let Some(position) = self.state.playlist.position() else {
            self.state.player.terminate();
            self.state.config.playback_position_ms = 0;
            return vec![
                AppEffect::StopPlayback,
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::All),
            ];
        };
        let Some(entry) = self.state.playlist.entries().get(position) else {
            self.state.player.terminate();
            self.state.config.playback_position_ms = 0;
            return vec![
                AppEffect::StopPlayback,
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::All),
            ];
        };
        let uri = entry.filename.clone();
        let position_ms = position_ms.max(0);
        self.state.config.playback_position_ms = position_ms;
        self.state.player.mark_playing();
        vec![
            AppEffect::StartPlaybackUri { uri, position_ms },
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }

    pub(super) fn handle_playback_event(&mut self, event: PlaybackEvent) -> Vec<AppEffect> {
        if self.state.player.apply_playback_event(&event) {
            vec![AppEffect::QueueRender(RenderTarget::All)]
        } else {
            Vec::new()
        }
    }

    pub(super) fn handle_playlist_eof(&mut self) -> Vec<AppEffect> {
        if self.state.playlist.eof_reached() {
            self.start_current_playlist_playback(0)
        } else {
            self.terminate_playback()
        }
    }

    fn terminate_playback(&mut self) -> Vec<AppEffect> {
        self.state.player.terminate();
        self.state.player.clear_visualization_data();
        self.state.config.playback_position_ms = 0;
        vec![
            AppEffect::StopPlayback,
            AppEffect::SaveConfig,
            AppEffect::QueueRender(RenderTarget::All),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::effect::RenderTarget;
    use crate::player::PlayerState;

    #[test]
    fn controller_volume_command_clamps_and_returns_output_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(AudioCommand::SetVolume(150).into());

        assert_eq!(controller.state().player.volume(), 100);
        assert_eq!(effects[0], AppEffect::SetOutputVolume(100));
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::SetBackendVolume(_))));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn equalizer_preamp_remains_a_backend_equalizer_effect() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(EqualizerCommand::SetPreamp(73).into());

        assert_eq!(controller.state().config.equalizer_preamp_pos, 73);
        assert!(effects.contains(&AppEffect::SetBackendEqualizer));
        assert!(!effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::SetOutputVolume(_))));
    }

    #[test]
    fn controller_balance_command_clamps_and_returns_backend_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(AudioCommand::SetBalance(-150).into());

        assert_eq!(controller.state().player.balance(), -100);
        assert_eq!(effects[0], AppEffect::SetBackendBalance(-100));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn play_from_stopped_selects_first_entry_and_requests_playback() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        let mut controller = AppController::new(state);

        let effects = controller.handle_command(PlayerCommand::Play.into());

        assert_eq!(controller.state().playlist.position(), Some(0));
        assert_eq!(controller.state().player.state(), PlayerState::Playing);
        assert!(effects.contains(&AppEffect::StartPlaybackUri {
            uri: "file:///tmp/one.ogg".to_string(),
            position_ms: 0,
        }));
    }

    #[test]
    fn play_without_a_current_entry_resets_the_saved_position() {
        let mut state = AppState::default();
        state.config.playback_position_ms = 42_000;
        let mut controller = AppController::new(state);

        let effects = controller.handle_command(PlayerCommand::Play.into());

        assert_eq!(controller.state().player.state(), PlayerState::Stopped);
        assert_eq!(controller.state().config.playback_position_ms, 0);
        assert!(effects.contains(&AppEffect::StopPlayback));
        assert!(effects.contains(&AppEffect::SaveConfig));
    }

    #[test]
    fn next_track_starts_from_beginning() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.add_uri("file:///tmp/two.ogg");
        state.playlist.set_position(0);
        state.player.mark_playing();
        let mut controller = AppController::new(state);

        let effects = controller.handle_command(PlayerCommand::NextTrack.into());

        assert_eq!(controller.state().playlist.position(), Some(1));
        assert_eq!(controller.state().player.state(), PlayerState::Playing);
        assert!(effects.contains(&AppEffect::StartPlaybackUri {
            uri: "file:///tmp/two.ogg".to_string(),
            position_ms: 0,
        }));
    }

    #[test]
    fn start_current_track_restarts_selected_entry_while_playing() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.add_uri("file:///tmp/two.ogg");
        state.playlist.set_position(0);
        let mut controller = AppController::new(state);
        controller.handle_command(PlayerCommand::Play.into());
        controller.handle_command(PlaylistCommand::SetPosition(1).into());

        let effects = controller.handle_command(PlayerCommand::StartCurrentTrack.into());

        assert_eq!(controller.state().playlist.position(), Some(1));
        assert!(effects.contains(&AppEffect::StartPlaybackUri {
            uri: "file:///tmp/two.ogg".to_string(),
            position_ms: 0,
        }));
    }

    #[test]
    fn play_and_pause_only_apply_valid_state_transitions() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        let mut controller = AppController::new(state);
        controller.handle_command(PlayerCommand::Play.into());

        assert!(controller
            .handle_command(PlayerCommand::Play.into())
            .is_empty());
        assert_eq!(controller.state().player.state(), PlayerState::Playing);

        let pause_effects = controller.handle_command(PlayerCommand::Pause.into());
        assert_eq!(controller.state().player.state(), PlayerState::Paused);
        assert!(pause_effects.contains(&AppEffect::PausePlayback));

        assert!(controller
            .handle_command(PlayerCommand::Pause.into())
            .is_empty());
        assert_eq!(controller.state().player.state(), PlayerState::Paused);

        let resume_effects = controller.handle_command(PlayerCommand::Play.into());
        assert_eq!(controller.state().player.state(), PlayerState::Playing);
        assert!(resume_effects.contains(&AppEffect::ResumePlayback));
    }

    #[test]
    fn halt_stops_playback_and_resets_position() {
        for state in [PlayerState::Playing, PlayerState::Paused] {
            let mut app_state = AppState::default();
            app_state
                .playlist
                .add_timed_uri("file:///tmp/one.ogg", "One", 60_000);
            app_state.playlist.set_position(0);
            app_state.player.mark_playing();
            if state == PlayerState::Paused {
                app_state.player.pause();
            }
            app_state.config.playback_position_ms = 42_000;
            let mut controller = AppController::new(app_state);

            let effects = controller.handle_command(PlayerCommand::Halt.into());

            assert_eq!(controller.state().player.state(), PlayerState::Stopped);
            assert_eq!(controller.state().config.playback_position_ms, 0);
            assert!(effects.contains(&AppEffect::StopPlayback));
            assert!(!effects.contains(&AppEffect::PausePlayback));
            assert!(!effects.contains(&AppEffect::SeekPlayback(0)));
        }
    }

    #[test]
    fn playlist_menu_commands_mutate_playlist_state() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.add_uri("file:///tmp/two.ogg");
        let mut controller = AppController::new(state);

        let effects = controller.handle_command(
            PlaylistCommand::ExecuteMenu {
                kind: crate::playlist::PlaylistMenuKind::Select,
                index: 0,
            }
            .into(),
        );

        assert!(controller
            .state()
            .playlist
            .entries()
            .iter()
            .all(|entry| entry.selected));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::Playlist)));
    }

    #[test]
    fn seek_command_updates_saved_position_and_requests_backend_seek() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(PlayerCommand::SeekToMs(42_000).into());

        assert_eq!(controller.state().config.playback_position_ms, 42_000);
        assert!(effects.contains(&AppEffect::SeekPlayback(42_000)));
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn playback_events_update_controller_player_state() {
        let mut controller = AppController::new(AppState::default());

        let effects =
            controller.handle_playback_event(PlaybackEvent::DurationChanged(Some(12_000)));

        assert_eq!(controller.state().player.duration_ms(), Some(12_000));
        assert_eq!(effects, vec![AppEffect::QueueRender(RenderTarget::All)]);
    }

    #[test]
    fn playlist_eof_advances_and_restarts_playback() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.add_uri("file:///tmp/two.ogg");
        state.playlist.set_position(0);
        state.player.mark_playing();
        let mut controller = AppController::new(state);

        let effects = controller.handle_playlist_eof();

        assert_eq!(controller.state().playlist.position(), Some(1));
        assert!(effects.contains(&AppEffect::StartPlaybackUri {
            uri: "file:///tmp/two.ogg".to_string(),
            position_ms: 0,
        }));
    }

    #[test]
    fn playlist_eof_stops_at_the_end_without_repeat() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.set_position(0);
        state.player.mark_playing();
        let mut controller = AppController::new(state);

        let effects = controller.handle_playlist_eof();

        assert_eq!(controller.state().player.state(), PlayerState::Stopped);
        assert!(effects.contains(&AppEffect::StopPlayback));
    }

    #[test]
    fn playlist_eof_repeats_the_final_track() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.set_position(0);
        state.playlist.set_repeat(true);
        state.player.mark_playing();
        let mut controller = AppController::new(state);

        let effects = controller.handle_playlist_eof();

        assert_eq!(controller.state().playlist.position(), Some(0));
        assert!(effects.contains(&AppEffect::StartPlaybackUri {
            uri: "file:///tmp/one.ogg".to_string(),
            position_ms: 0,
        }));
    }

    #[test]
    fn playlist_eof_honors_no_advance_and_empty_playlists() {
        let mut no_advance = AppState::default();
        no_advance.playlist.add_uri("file:///tmp/one.ogg");
        no_advance.playlist.add_uri("file:///tmp/two.ogg");
        no_advance.playlist.set_position(0);
        no_advance.playlist.set_no_advance(true);
        no_advance.player.mark_playing();
        let mut controller = AppController::new(no_advance);

        let effects = controller.handle_playlist_eof();

        assert_eq!(controller.state().playlist.position(), Some(0));
        assert_eq!(controller.state().player.state(), PlayerState::Stopped);
        assert!(effects.contains(&AppEffect::StopPlayback));

        let mut empty = AppController::new(AppState::default());
        let effects = empty.handle_playlist_eof();
        assert_eq!(empty.state().player.state(), PlayerState::Stopped);
        assert!(effects.contains(&AppEffect::StopPlayback));
    }

    #[test]
    fn playlist_eof_with_shuffle_starts_an_entry_from_the_playlist() {
        let mut state = AppState::default();
        for uri in [
            "file:///tmp/one.ogg",
            "file:///tmp/two.ogg",
            "file:///tmp/three.ogg",
        ] {
            state.playlist.add_uri(uri);
        }
        state.playlist.set_position(0);
        state.playlist.set_shuffle(true);
        state.playlist.set_repeat(true);
        state.player.mark_playing();
        let mut controller = AppController::new(state);

        let effects = controller.handle_playlist_eof();

        let started_uri = effects.iter().find_map(|effect| match effect {
            AppEffect::StartPlaybackUri { uri, .. } => Some(uri.as_str()),
            _ => None,
        });
        assert!(matches!(
            started_uri,
            Some("file:///tmp/one.ogg" | "file:///tmp/two.ogg" | "file:///tmp/three.ogg")
        ));
    }

    #[test]
    fn add_playlist_uris_command_preserves_current_position() {
        let mut state = AppState::default();
        state.playlist.add_uri("file:///tmp/one.ogg");
        state.playlist.set_position(0);
        let mut controller = AppController::new(state);

        controller.handle_command(
            PlaylistCommand::AddUris(vec!["file:///tmp/two.ogg".to_string()]).into(),
        );

        assert_eq!(controller.state().playlist.position(), Some(0));
        assert_eq!(controller.state().playlist.len(), 2);
    }

    #[test]
    fn queue_commands_are_domain_transitions_with_playlist_render_effects() {
        let mut state = AppState::default();
        for name in ["one", "two", "three"] {
            state.playlist.add_uri(format!("file:///tmp/{name}.ogg"));
        }
        let mut controller = AppController::new(state);

        let enqueue = controller.handle_command(PlaylistCommand::Enqueue(1).into());
        assert_eq!(controller.state().playlist.queued_indices(), vec![1]);
        assert_eq!(
            enqueue,
            vec![AppEffect::QueueRender(RenderTarget::Playlist)]
        );

        assert!(controller
            .handle_command(PlaylistCommand::Enqueue(1).into())
            .is_empty());
        controller.handle_command(PlaylistCommand::ToggleQueue(vec![1, 2]).into());
        assert_eq!(controller.state().playlist.queued_indices(), vec![2]);

        controller.handle_command(PlaylistCommand::Dequeue(2).into());
        assert!(controller.state().playlist.queued_indices().is_empty());
        assert!(controller
            .handle_command(PlaylistCommand::Dequeue(2).into())
            .is_empty());

        controller.handle_command(PlaylistCommand::Enqueue(0).into());
        controller.handle_command(PlaylistCommand::ClearQueue.into());
        assert!(controller.state().playlist.queued_indices().is_empty());
        assert!(controller
            .handle_command(PlaylistCommand::ClearQueue.into())
            .is_empty());
    }

    #[test]
    fn controller_structural_commands_preserve_or_prune_queue_identity() {
        let mut state = AppState::default();
        for name in ["zulu", "alpha", "echo", "bravo"] {
            state.playlist.add_uri(format!("file:///tmp/{name}.ogg"));
        }
        assert!(state.playlist.enqueue(1));
        assert!(state.playlist.enqueue(3));
        let mut controller = AppController::new(state);

        controller.handle_command(
            PlaylistCommand::AddUris(vec!["file:///tmp/charlie.ogg".to_string()]).into(),
        );
        assert_eq!(
            controller_queued_titles(&controller),
            vec!["alpha", "bravo"]
        );

        controller.handle_command(PlaylistCommand::MoveEntry { from: 1, to: 4 }.into());
        assert_eq!(
            controller_queued_titles(&controller),
            vec!["alpha", "bravo"]
        );

        controller.handle_command(PlaylistCommand::Reverse.into());
        assert_eq!(
            controller_queued_titles(&controller),
            vec!["alpha", "bravo"]
        );

        controller.handle_command(PlaylistCommand::Randomize.into());
        assert_eq!(
            controller_queued_titles(&controller),
            vec!["alpha", "bravo"]
        );

        controller
            .handle_command(PlaylistCommand::Sort(crate::playlist::PlaylistSortKey::Title).into());
        assert_eq!(
            controller_queued_titles(&controller),
            vec!["alpha", "bravo"]
        );

        for title in ["zulu", "alpha", "bravo"] {
            let index = controller_title_index(&controller, title);
            controller.handle_command(PlaylistCommand::ToggleEntrySelection(index).into());
        }
        controller.handle_command(
            PlaylistCommand::SortSelected(crate::playlist::PlaylistSortKey::Filename).into(),
        );
        assert_eq!(
            controller_queued_titles(&controller),
            vec!["alpha", "bravo"]
        );

        controller.handle_command(PlaylistCommand::SelectNone.into());
        let alpha = controller_title_index(&controller, "alpha");
        controller.handle_command(PlaylistCommand::ToggleEntrySelection(alpha).into());
        controller.handle_command(PlaylistCommand::RemoveSelected.into());
        assert_eq!(controller_queued_titles(&controller), vec!["bravo"]);

        controller.handle_command(PlaylistCommand::Clear.into());
        assert!(controller.state().playlist.queued_indices().is_empty());
    }

    #[test]
    fn controller_single_remove_and_crop_prune_queue_atomically() {
        let mut removed = controller_with_all_entries_queued();
        removed.handle_command(PlaylistCommand::SetPosition(2).into());
        removed.handle_command(PlaylistCommand::RemoveSelectedOrCurrent.into());
        assert_eq!(
            controller_queued_titles(&removed),
            vec!["one", "two", "four"]
        );

        let mut cropped = controller_with_all_entries_queued();
        cropped.handle_command(PlaylistCommand::ToggleEntrySelection(1).into());
        cropped.handle_command(PlaylistCommand::ToggleEntrySelection(3).into());
        cropped.handle_command(PlaylistCommand::CropToSelection.into());
        assert_eq!(controller_queued_titles(&cropped), vec!["two", "four"]);
    }

    #[test]
    fn clearing_playlist_stops_and_resets_player() {
        for command in [
            AppCommand::from(PlaylistCommand::Clear),
            AppCommand::from(PlaylistCommand::ExecuteMenu {
                kind: crate::playlist::PlaylistMenuKind::List,
                index: 0,
            }),
        ] {
            let mut state = AppState::default();
            state.playlist.add_uri("file:///tmp/one.ogg");
            state.playlist.set_position(0);
            state.player.mark_playing();
            state.config.playback_position_ms = 42_000;
            let mut controller = AppController::new(state);

            let effects = controller.handle_command(command);

            assert!(controller.state().playlist.is_empty());
            assert_eq!(controller.state().playlist.position(), None);
            assert_eq!(controller.state().player.state(), PlayerState::Stopped);
            assert_eq!(controller.state().config.playback_position_ms, 0);
            assert!(effects.contains(&AppEffect::StopPlayback));
            assert!(effects.contains(&AppEffect::SaveConfig));
            assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
        }
    }

    #[test]
    fn panel_commands_update_config_and_request_redraw() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(PanelCommand::SetPlaylistVisibility(true).into());
        controller.handle_command(PanelCommand::ToggleEqualizerShade.into());
        controller.handle_command(PanelCommand::SetPlaylistDetached(true).into());

        assert!(controller.state().config.playlist_visible);
        assert!(controller.state().config.equalizer_shaded);
        assert!(controller.state().config.playlist_detached);
        assert!(effects.contains(&AppEffect::SaveConfig));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::All)));
    }

    #[test]
    fn equalizer_commands_update_config_and_backend_effects() {
        let mut controller = AppController::new(AppState::default());

        let effects = controller.handle_command(
            EqualizerCommand::SetBand {
                band: 2,
                position: 150,
            }
            .into(),
        );
        controller.handle_command(EqualizerCommand::SetActive(true).into());

        assert_eq!(controller.state().config.equalizer_band_pos[2], 100);
        assert!(controller.state().config.equalizer_active);
        assert!(effects.contains(&AppEffect::SetBackendEqualizer));
        assert!(effects.contains(&AppEffect::QueueRender(RenderTarget::Equalizer)));
    }

    fn controller_queued_titles(controller: &AppController) -> Vec<&str> {
        let playlist = &controller.state().playlist;
        playlist
            .queued_indices()
            .into_iter()
            .map(|index| playlist.entries()[index].title.as_str())
            .collect()
    }

    fn controller_title_index(controller: &AppController, title: &str) -> usize {
        controller
            .state()
            .playlist
            .entries()
            .iter()
            .position(|entry| entry.title == title)
            .unwrap()
    }

    fn controller_with_all_entries_queued() -> AppController {
        let mut state = AppState::default();
        for name in ["one", "two", "three", "four"] {
            state.playlist.add_uri(format!("file:///tmp/{name}.ogg"));
        }
        for index in 0..state.playlist.len() {
            state.playlist.enqueue(index);
        }
        AppController::new(state)
    }
}
