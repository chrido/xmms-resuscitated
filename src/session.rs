use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app_state::{AppState, PersistenceSnapshot};
use crate::config::Config;
use crate::playlist::{Playlist, PlaylistMenuKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplicationLaunchFlags {
    pub handles_command_line: bool,
    pub non_unique: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionCommand {
    pub options: PreviewOptions,
    pub playlist_menu: Option<PlaylistMenuKind>,
    pub positional_paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionApplyResult {
    pub files_added: usize,
    pub should_start_playback: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionState {
    pub playlist_visible: bool,
    pub playlist_detached: bool,
    pub equalizer_visible: bool,
    pub equalizer_detached: bool,
}

pub fn application_launch_flags(xmms_non_unique_env: Option<&str>) -> ApplicationLaunchFlags {
    ApplicationLaunchFlags {
        handles_command_line: true,
        non_unique: xmms_non_unique_env.is_some_and(|value| !value.is_empty()),
    }
}

pub fn parse_session_command(args: &[String]) -> Result<SessionCommand, String> {
    let mut command = SessionCommand::default();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--playlist" | "--show-playlist" => command.options.show_playlist = true,
            "--equalizer" => command.options.show_equalizer = true,
            "--dock-playlist" | "--playlist-docked" => {
                command.options.show_playlist = true;
                command.options.playlist_detached = Some(false);
            }
            "--undock-playlist" | "--playlist-undocked" => {
                command.options.show_playlist = true;
                command.options.playlist_detached = Some(true);
            }
            "--dock-equalizer" | "--equalizer-docked" => {
                command.options.show_equalizer = true;
                command.options.equalizer_detached = Some(false);
            }
            "--undock-equalizer" | "--equalizer-undocked" => {
                command.options.show_equalizer = true;
                command.options.equalizer_detached = Some(true);
            }
            "--shade-main" | "--shade" | "--main-shaded" => {
                command.options.main_shaded = Some(true)
            }
            "--unshade-main" => command.options.main_shaded = Some(false),
            "--shade-playlist" | "--playlist-shaded" => {
                command.options.show_playlist = true;
                command.options.playlist_shaded = Some(true);
            }
            "--unshade-playlist" => command.options.playlist_shaded = Some(false),
            "--shade-equalizer" | "--equalizer-shaded" => {
                command.options.show_equalizer = true;
                command.options.equalizer_shaded = Some(true);
            }
            "--unshade-equalizer" => command.options.equalizer_shaded = Some(false),
            "--reset" => command.options.reset = true,
            "--playlist-menu-add" => command.playlist_menu = Some(PlaylistMenuKind::Add),
            "--playlist-menu-remove" => command.playlist_menu = Some(PlaylistMenuKind::Remove),
            "--playlist-menu-select" => command.playlist_menu = Some(PlaylistMenuKind::Select),
            "--playlist-menu-misc" => command.playlist_menu = Some(PlaylistMenuKind::Misc),
            "--playlist-menu-list" => command.playlist_menu = Some(PlaylistMenuKind::List),
            "--skin" => {
                let Some(value) = iter.next() else {
                    return Err("--skin requires PATH".to_string());
                };
                command.options.skin_path = Some(value.to_string());
            }
            _ if arg.starts_with("--skin=") => {
                command.options.skin_path = Some(arg["--skin=".len()..].to_string());
            }
            _ if arg.starts_with('-') => {}
            _ => command.positional_paths.push(arg.to_string()),
        }
    }

    if command.playlist_menu.is_some() {
        command.options.show_playlist = true;
    }
    Ok(command)
}

pub fn apply_session_command(
    app_state: &mut AppState,
    command: &SessionCommand,
) -> io::Result<SessionApplyResult> {
    if command.options.reset {
        *app_state = AppState::from_config(Config::default());
    }

    apply_preview_options_to_config(&mut app_state.config, &command.options)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

    let mut files_added = 0;
    for path in &command.positional_paths {
        files_added += app_state.playlist.add_location(path)?;
    }
    Ok(SessionApplyResult {
        files_added,
        should_start_playback: files_added > 0 && !app_state.playlist.is_empty(),
    })
}

pub fn save_session_state(app_state: &AppState) -> SessionState {
    SessionState {
        playlist_visible: app_state.config.playlist_visible,
        playlist_detached: app_state.config.playlist_detached,
        equalizer_visible: app_state.config.equalizer_visible,
        equalizer_detached: app_state.config.equalizer_detached,
    }
}

pub fn restore_session_state(app_state: &mut AppState, state: &SessionState, reset: bool) {
    if reset {
        return;
    }
    app_state.config.playlist_visible = state.playlist_visible;
    app_state.config.playlist_detached = state.playlist_detached;
    app_state.config.equalizer_visible = state.equalizer_visible;
    app_state.config.equalizer_detached = state.equalizer_detached;
}

pub fn save_state_dict(state: &SessionState) -> BTreeMap<&'static str, bool> {
    BTreeMap::from([
        ("playlist-visible", state.playlist_visible),
        ("playlist-detached", state.playlist_detached),
        ("equalizer-visible", state.equalizer_visible),
        ("equalizer-detached", state.equalizer_detached),
    ])
}

pub fn restore_state_dict(app_state: &mut AppState, dict: &BTreeMap<String, bool>, reset: bool) {
    if reset {
        return;
    }
    let state = SessionState {
        playlist_visible: dict
            .get("playlist-visible")
            .copied()
            .unwrap_or(app_state.config.playlist_visible),
        playlist_detached: dict
            .get("playlist-detached")
            .copied()
            .unwrap_or(app_state.config.playlist_detached),
        equalizer_visible: dict
            .get("equalizer-visible")
            .copied()
            .unwrap_or(app_state.config.equalizer_visible),
        equalizer_detached: dict
            .get("equalizer-detached")
            .copied()
            .unwrap_or(app_state.config.equalizer_detached),
    };
    restore_session_state(app_state, &state, false);
}

pub fn save_fallback_state(
    app_state: &AppState,
    config_path: &Path,
    playlist_path: &Path,
) -> io::Result<()> {
    save_fallback_snapshot(
        &app_state.persistence_snapshot(),
        config_path,
        playlist_path,
    )
}

pub fn save_fallback_snapshot(
    snapshot: &PersistenceSnapshot<'_>,
    config_path: &Path,
    playlist_path: &Path,
) -> io::Result<()> {
    snapshot.config.save_to_file(config_path)?;
    if let Some(parent) = playlist_path.parent() {
        fs::create_dir_all(parent)?;
    }
    snapshot.playlist.save_m3u_file(playlist_path)
}

pub fn fallback_state_paths(config_dir: &Path) -> (PathBuf, PathBuf) {
    let dir = config_dir.join("xmms-renascene");
    (dir.join("config"), dir.join("playlist.m3u"))
}

pub fn default_config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XMMS_RS_CONFIG_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("HOME") {
        return PathBuf::from(path).join(".config");
    }
    PathBuf::from(".")
}

pub fn load_saved_state(
    config_path: &Path,
    playlist_path: &Path,
    reset: bool,
) -> io::Result<AppState> {
    crate::perf_span!("saved_state_load");
    let config = if reset {
        Config::default()
    } else {
        match Config::load_from_file(config_path) {
            Ok(config) => config,
            Err(err) if err.kind() == io::ErrorKind::NotFound => Config::default(),
            Err(err) => return Err(err),
        }
    };
    let mut app_state = AppState::from_config(config);
    if !reset {
        match Playlist::load_m3u_file(playlist_path) {
            Ok(playlist) => app_state.playlist = playlist,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        app_state.apply_config_to_runtime();
    }
    Ok(app_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(args: &[&str]) -> Vec<String> {
        std::iter::once("xmms-rs")
            .chain(args.iter().copied())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn application_flags_match_c_defaults_and_env_override() {
        assert_eq!(
            application_launch_flags(None),
            ApplicationLaunchFlags {
                handles_command_line: true,
                non_unique: false,
            }
        );
        assert!(application_launch_flags(Some("1")).non_unique);
    }

    #[test]
    fn parses_secondary_command_options_and_positional_files() {
        let command = parse_session_command(&args(&[
            "--playlist",
            "--undock-playlist",
            "--shade-playlist",
            "--equalizer",
            "--undock-equalizer",
            "--playlist-menu-list",
            "--skin=/tmp/skin.wsz",
            "/tmp/song.mp3",
        ]))
        .unwrap();

        assert!(command.options.show_playlist);
        assert_eq!(command.options.playlist_detached, Some(true));
        assert_eq!(command.options.playlist_shaded, Some(true));
        assert!(command.options.show_equalizer);
        assert_eq!(command.options.equalizer_detached, Some(true));
        assert_eq!(command.playlist_menu, Some(PlaylistMenuKind::List));
        assert_eq!(command.options.skin_path.as_deref(), Some("/tmp/skin.wsz"));
        assert_eq!(command.positional_paths, vec!["/tmp/song.mp3"]);
    }

    #[test]
    fn applies_command_preserves_unspecified_docking_state() {
        let mut state = AppState::from_config(Config {
            playlist_detached: true,
            equalizer_detached: true,
            ..Config::default()
        });
        let command = parse_session_command(&args(&["--playlist", "--equalizer"])).unwrap();

        apply_session_command(&mut state, &command).unwrap();

        assert!(state.config.playlist_visible);
        assert!(state.config.equalizer_visible);
        assert!(state.config.playlist_detached);
        assert!(state.config.equalizer_detached);
    }

    #[test]
    fn applies_command_resets_state_adds_files_and_requests_playback() {
        let mut state = AppState::from_config(Config {
            volume: 10,
            playlist_visible: true,
            ..Config::default()
        });
        state.playlist.add_uri("file:///tmp/old.mp3");
        let command = parse_session_command(&args(&[
            "--reset",
            "--equalizer",
            "https://example.test/new.mp3",
        ]))
        .unwrap();

        let result = apply_session_command(&mut state, &command).unwrap();

        assert_eq!(state.config.volume, Config::default().volume);
        assert!(!state.config.playlist_visible);
        assert!(state.config.equalizer_visible);
        assert_eq!(state.playlist.len(), 1);
        assert_eq!(
            state.playlist.entries()[0].filename,
            "https://example.test/new.mp3"
        );
        assert!(result.should_start_playback);
    }

    #[test]
    fn session_state_restore_respects_reset() {
        let mut state = AppState::default();
        let session = SessionState {
            playlist_visible: true,
            playlist_detached: true,
            equalizer_visible: true,
            equalizer_detached: true,
        };
        restore_session_state(&mut state, &session, false);
        assert!(state.config.playlist_visible);
        assert!(state.config.playlist_detached);
        assert!(state.config.equalizer_visible);
        assert!(state.config.equalizer_detached);

        restore_session_state(&mut state, &SessionState::default(), true);
        assert!(state.config.playlist_visible);
        assert!(state.config.equalizer_visible);
    }

    #[test]
    fn saved_playlist_restore_preserves_order_and_current_position() {
        let root = std::env::temp_dir().join(format!(
            "xmms-session-restore-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let config_path = root.join("config");
        let playlist_path = root.join("playlist.m3u");
        Config {
            playlist_position: 1,
            ..Config::default()
        }
        .save_to_file(&config_path)
        .unwrap();
        fs::write(
            &playlist_path,
            "#EXTM3U\n#EXTINF:60,First\nfirst.ogg\n#EXTINF:61,Second\nsecond.ogg\n",
        )
        .unwrap();

        let state = load_saved_state(&config_path, &playlist_path, false).unwrap();

        assert_eq!(state.playlist.entries()[0].title, "First");
        assert_eq!(state.playlist.entries()[1].title, "Second");
        assert_eq!(state.playlist.position(), Some(1));
        fs::remove_dir_all(root).unwrap();
    }
}
