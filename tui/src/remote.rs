//! Frontend-side facades for the daemon-owned API + player. These mirror the
//! public surface of `spfy_core::api::SpotifyApi` and
//! `spfy_core::player::PlayerHandle` so that `tui::main` doesn't need to care
//! whether the resources are local or remote.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use spfy_core::error::{CoreError, Result as CoreResult};
use spfy_core::ipc::{
    read_envelope, write_envelope, DaemonMsg, Envelope, FrontendMsg,
};
use spfy_core::model::*;
use spfy_core::player::{Cmd, Event};
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<DaemonMsg>>>>;

/// Spawn the reader/writer halves of a connected daemon socket. Returns the
/// pieces needed to construct a `RemoteApi` and a `RemotePlayer`.
pub fn split(stream: UnixStream) -> (RemoteApi, RemotePlayer) {
    let (read_half, write_half) = stream.into_split();
    let reader = BufReader::new(read_half);

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<(u64, FrontendMsg)>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<Event>();
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));

    // Writer task: forwards (id, msg) pairs out to the daemon.
    {
        let mut write_half = write_half;
        tokio::spawn(async move {
            while let Some((id, msg)) = out_rx.recv().await {
                let env = Envelope { id, msg };
                if write_envelope(&mut write_half, &env).await.is_err() {
                    break;
                }
            }
        });
    }

    // Reader task: routes incoming envelopes to either the per-request oneshot
    // or the player-event channel.
    {
        let pending = pending.clone();
        let event_tx = event_tx.clone();
        let mut reader = reader;
        tokio::spawn(async move {
            loop {
                let env = match read_envelope::<DaemonMsg>(&mut reader).await {
                    Ok(Some(env)) => env,
                    Ok(None) => break,
                    Err(_) => break,
                };
                if env.id == 0 {
                    if let DaemonMsg::PlayerEvent(ev) = env.msg {
                        let _ = event_tx.send(ev);
                    }
                } else {
                    let waiter = pending.lock().await.remove(&env.id);
                    if let Some(tx) = waiter {
                        let _ = tx.send(env.msg);
                    }
                }
            }
        });
    }

    let api = RemoteApi {
        tx: out_tx.clone(),
        pending,
        next_id: AtomicU64::new(1),
    };
    let player = RemotePlayer {
        tx: out_tx,
        event_rx: Some(event_rx),
    };
    (api, player)
}

pub struct RemoteApi {
    tx: mpsc::UnboundedSender<(u64, FrontendMsg)>,
    pending: Pending,
    next_id: AtomicU64,
}

impl RemoteApi {
    async fn request(&self, msg: FrontendMsg) -> CoreResult<DaemonMsg> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.tx
            .send((id, msg))
            .map_err(|e| CoreError::Api(format!("daemon disconnected: {e}")))?;
        rx.await
            .map_err(|e| CoreError::Api(format!("daemon dropped reply: {e}")))
    }

    pub async fn liked_tracks(&self) -> CoreResult<Vec<Track>> {
        match self.request(FrontendMsg::LoadLikedTracks).await? {
            DaemonMsg::LikedTracks(r) => r.map_err(CoreError::Api),
            other => Err(CoreError::Api(format!("unexpected reply: {other:?}"))),
        }
    }

    pub async fn saved_albums(&self) -> CoreResult<Vec<Album>> {
        match self.request(FrontendMsg::LoadSavedAlbums).await? {
            DaemonMsg::SavedAlbums(r) => r.map_err(CoreError::Api),
            other => Err(CoreError::Api(format!("unexpected reply: {other:?}"))),
        }
    }

    pub async fn playlists(&self) -> CoreResult<Vec<Playlist>> {
        match self.request(FrontendMsg::LoadPlaylists).await? {
            DaemonMsg::Playlists(r) => r.map_err(CoreError::Api),
            other => Err(CoreError::Api(format!("unexpected reply: {other:?}"))),
        }
    }

    pub async fn followed_artists(&self) -> CoreResult<Vec<Artist>> {
        match self.request(FrontendMsg::LoadFollowedArtists).await? {
            DaemonMsg::FollowedArtists(r) => r.map_err(CoreError::Api),
            other => Err(CoreError::Api(format!("unexpected reply: {other:?}"))),
        }
    }

    pub async fn recently_played(&self) -> CoreResult<Vec<PlayHistoryEntry>> {
        match self.request(FrontendMsg::LoadRecentlyPlayed).await? {
            DaemonMsg::RecentlyPlayed(r) => r.map_err(CoreError::Api),
            other => Err(CoreError::Api(format!("unexpected reply: {other:?}"))),
        }
    }

    pub async fn album_tracks(&self, id: &AlbumId) -> CoreResult<Vec<Track>> {
        match self
            .request(FrontendMsg::LoadAlbumTracks(id.clone()))
            .await?
        {
            DaemonMsg::AlbumTracks(r) => r.map_err(CoreError::Api),
            other => Err(CoreError::Api(format!("unexpected reply: {other:?}"))),
        }
    }

    pub async fn playlist_tracks(&self, id: &PlaylistId) -> CoreResult<Vec<Track>> {
        match self
            .request(FrontendMsg::LoadPlaylistTracks(id.clone()))
            .await?
        {
            DaemonMsg::PlaylistTracks(r) => r.map_err(CoreError::Api),
            other => Err(CoreError::Api(format!("unexpected reply: {other:?}"))),
        }
    }

    pub async fn search_tracks(&self, query: &str, limit: u32) -> CoreResult<Vec<Track>> {
        match self
            .request(FrontendMsg::SearchTracks {
                query: query.to_string(),
                limit,
            })
            .await?
        {
            DaemonMsg::SearchResult(r) => r.map_err(CoreError::Api),
            other => Err(CoreError::Api(format!("unexpected reply: {other:?}"))),
        }
    }
}

pub struct RemotePlayer {
    tx: mpsc::UnboundedSender<(u64, FrontendMsg)>,
    event_rx: Option<mpsc::UnboundedReceiver<Event>>,
}

impl RemotePlayer {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send((0, FrontendMsg::Cmd(cmd)));
    }

    pub fn take_events(&mut self) -> mpsc::UnboundedReceiver<Event> {
        self.event_rx
            .take()
            .expect("RemotePlayer::take_events called twice")
    }
}
