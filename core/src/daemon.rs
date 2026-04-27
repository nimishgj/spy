//! Background daemon process: owns the librespot session/player and the
//! rspotify Web API client, and exposes them to TUI frontends over a Unix
//! domain socket.

use std::sync::Arc;

use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{info, warn};

use crate::api::SpotifyApi;
use crate::ipc::{
    read_envelope, write_envelope, DaemonMsg, Envelope, FrontendMsg,
};
use crate::paths;
use crate::player::{Cmd, Event};

/// Cached "last known" player state, replayed to fresh clients on connect so a
/// TUI re-attaching to a daemon already mid-playback doesn't show
/// "(nothing playing)".
#[derive(Clone, Default)]
struct LastState {
    /// The most recent `Event::Started` (with full metadata).
    started: Option<Event>,
    /// Whether playback is currently `Resumed` (`true`) or `Paused` (`false`).
    is_playing: bool,
    /// Latest `Event::Position` value seen.
    position_ms: u32,
    /// Latest volume; 70 by default.
    #[allow(dead_code)]
    volume: u8,
}

impl LastState {
    fn new() -> Self {
        Self {
            started: None,
            is_playing: false,
            position_ms: 0,
            volume: 70,
        }
    }
}

/// Run the daemon: log in, spawn the player, bind the socket, accept clients.
///
/// Returns when a frontend sends `FrontendMsg::Shutdown` (cleanly stopping the
/// player) or when an unrecoverable IO error occurs on the listener.
pub async fn run() -> anyhow::Result<()> {
    let session = crate::auth::login()?;

    let mut player = crate::player::spawn(
        session.player_credentials,
        tokio::runtime::Handle::current(),
    );
    let player_events = player.take_events();
    let cmd_tx = player.cmd_sender();

    let api = Arc::new(SpotifyApi::new(session.api));

    let socket_path = paths::config_dir()?.join("daemon.sock");
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    info!("daemon listening at {}", socket_path.display());

    let pid_path = paths::config_dir()?.join("daemon.pid");
    let _ = std::fs::write(&pid_path, std::process::id().to_string());

    // Broadcast channel fans out player events to every connected client.
    let (event_tx, _) = broadcast::channel::<Event>(64);

    // Cache the most recent player state so we can replay it to clients that
    // attach mid-playback (otherwise their reducer never sees `Started` and
    // shows "(nothing playing)").
    let last_state: Arc<RwLock<LastState>> = Arc::new(RwLock::new(LastState::new()));

    // Forwarder task: update the cache, then drain player events into the
    // broadcast channel.
    {
        let event_tx = event_tx.clone();
        let last_state = last_state.clone();
        let mut player_events = player_events;
        tokio::spawn(async move {
            while let Some(ev) = player_events.recv().await {
                {
                    let mut state = last_state.write().await;
                    match &ev {
                        Event::Started { .. } => {
                            state.started = Some(ev.clone());
                            state.position_ms = 0;
                        }
                        Event::Resumed => state.is_playing = true,
                        Event::Paused => state.is_playing = false,
                        Event::Position(ms) => state.position_ms = *ms,
                        Event::Stopped => {
                            state.started = None;
                            state.is_playing = false;
                            state.position_ms = 0;
                        }
                        _ => {}
                    }
                }
                let _ = event_tx.send(ev);
            }
        });
    }

    // Shutdown signal: when any client requests Shutdown we set this, finish
    // the in-flight handler, and break out of the accept loop.
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.recv() => {
                info!("daemon shutdown requested");
                break;
            }
            accept = listener.accept() => {
                let (stream, _addr) = match accept {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("accept failed: {e}");
                        continue;
                    }
                };
                let api = api.clone();
                let cmd_tx = cmd_tx.clone();
                let event_rx = event_tx.subscribe();
                let shutdown_tx = shutdown_tx.clone();
                let last_state = last_state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(
                        stream, api, cmd_tx, event_rx, shutdown_tx, last_state,
                    )
                    .await
                    {
                        warn!("client task ended: {e}");
                    }
                });
            }
        }
    }

    // Clean shutdown: tell the player worker to quit and clean up the socket.
    let _ = cmd_tx.send(Cmd::Quit);
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&pid_path);
    Ok(())
}

async fn handle_client(
    stream: UnixStream,
    api: Arc<SpotifyApi>,
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    mut event_rx: broadcast::Receiver<Event>,
    shutdown_tx: mpsc::Sender<()>,
    last_state: Arc<RwLock<LastState>>,
) -> anyhow::Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Replay the daemon's cached "last known" player state to the new client,
    // so a TUI attaching mid-playback immediately sees the current track,
    // play/pause state, and position. The reducer treats these identically to
    // live events.
    {
        let snapshot = last_state.read().await.clone();
        if let Some(started) = snapshot.started {
            let env = Envelope {
                id: 0,
                msg: DaemonMsg::PlayerEvent(started),
            };
            if write_envelope(&mut write_half, &env).await.is_err() {
                return Ok(());
            }
            let cur_state_event = if snapshot.is_playing {
                Event::Resumed
            } else {
                Event::Paused
            };
            let env = Envelope {
                id: 0,
                msg: DaemonMsg::PlayerEvent(cur_state_event),
            };
            if write_envelope(&mut write_half, &env).await.is_err() {
                return Ok(());
            }
            let env = Envelope {
                id: 0,
                msg: DaemonMsg::PlayerEvent(Event::Position(snapshot.position_ms)),
            };
            if write_envelope(&mut write_half, &env).await.is_err() {
                return Ok(());
            }
        }
    }

    loop {
        tokio::select! {
            biased;
            ev = event_rx.recv() => {
                match ev {
                    Ok(ev) => {
                        let env = Envelope { id: 0, msg: DaemonMsg::PlayerEvent(ev) };
                        if write_envelope(&mut write_half, &env).await.is_err() {
                            return Ok(());
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
            frame = read_envelope::<FrontendMsg>(&mut reader) => {
                let env = match frame {
                    Ok(Some(env)) => env,
                    Ok(None) => return Ok(()),
                    Err(e) => {
                        warn!("client read error: {e}");
                        return Ok(());
                    }
                };
                let id = env.id;
                let api = api.clone();
                let cmd_tx = cmd_tx.clone();
                let shutdown_tx = shutdown_tx.clone();
                let response = dispatch(api, cmd_tx, shutdown_tx, env.msg).await;
                if let Some(msg) = response {
                    let env = Envelope { id, msg };
                    if write_envelope(&mut write_half, &env).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Dispatch one frontend message. Returns `Some(DaemonMsg)` if a reply is
/// expected, `None` for fire-and-forget messages (player commands).
async fn dispatch(
    api: Arc<SpotifyApi>,
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    shutdown_tx: mpsc::Sender<()>,
    msg: FrontendMsg,
) -> Option<DaemonMsg> {
    match msg {
        FrontendMsg::Cmd(cmd) => {
            let _ = cmd_tx.send(cmd);
            None
        }
        FrontendMsg::LoadLikedTracks => Some(DaemonMsg::LikedTracks(
            api.liked_tracks().await.map_err(|e| e.to_string()),
        )),
        FrontendMsg::LoadSavedAlbums => Some(DaemonMsg::SavedAlbums(
            api.saved_albums().await.map_err(|e| e.to_string()),
        )),
        FrontendMsg::LoadPlaylists => Some(DaemonMsg::Playlists(
            api.playlists().await.map_err(|e| e.to_string()),
        )),
        FrontendMsg::LoadFollowedArtists => Some(DaemonMsg::FollowedArtists(
            api.followed_artists().await.map_err(|e| e.to_string()),
        )),
        FrontendMsg::LoadRecentlyPlayed => Some(DaemonMsg::RecentlyPlayed(
            api.recently_played().await.map_err(|e| e.to_string()),
        )),
        FrontendMsg::LoadAlbumTracks(id) => Some(DaemonMsg::AlbumTracks(
            api.album_tracks(&id).await.map_err(|e| e.to_string()),
        )),
        FrontendMsg::LoadPlaylistTracks(id) => Some(DaemonMsg::PlaylistTracks(
            api.playlist_tracks(&id).await.map_err(|e| e.to_string()),
        )),
        FrontendMsg::SearchTracks { query, limit } => Some(DaemonMsg::SearchResult(
            api.search_tracks(&query, limit)
                .await
                .map_err(|e| e.to_string()),
        )),
        FrontendMsg::Shutdown => {
            let _ = shutdown_tx.send(()).await;
            Some(DaemonMsg::ShutdownAck)
        }
    }
}
