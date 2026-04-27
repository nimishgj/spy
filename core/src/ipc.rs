//! IPC types and helpers shared between the spfy daemon and the TUI frontend.
//!
//! Wire format: line-delimited JSON. Each line is a single JSON-serialized
//! `Envelope<T>`. `id == 0` is reserved for unsolicited messages (player events
//! pushed by the daemon); request IDs greater than zero are used for
//! request/response pairs.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedReadHalf;

use crate::model::*;
use crate::player::{Cmd, Event};

#[derive(Serialize, Deserialize, Debug)]
pub struct Envelope<T> {
    pub id: u64,
    pub msg: T,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum FrontendMsg {
    Cmd(Cmd),
    LoadLikedTracks,
    LoadSavedAlbums,
    LoadPlaylists,
    LoadFollowedArtists,
    LoadRecentlyPlayed,
    LoadAlbumTracks(AlbumId),
    LoadPlaylistTracks(PlaylistId),
    SearchTracks { query: String, limit: u32 },
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum DaemonMsg {
    PlayerEvent(Event),
    LikedTracks(Result<Vec<Track>, String>),
    SavedAlbums(Result<Vec<Album>, String>),
    Playlists(Result<Vec<Playlist>, String>),
    FollowedArtists(Result<Vec<Artist>, String>),
    RecentlyPlayed(Result<Vec<PlayHistoryEntry>, String>),
    AlbumTracks(Result<Vec<Track>, String>),
    PlaylistTracks(Result<Vec<Track>, String>),
    SearchResult(Result<Vec<Track>, String>),
    ShutdownAck,
}

/// Read one newline-terminated JSON envelope from the reader. Returns
/// `Ok(None)` on clean EOF.
pub async fn read_envelope<T: DeserializeOwned>(
    reader: &mut BufReader<OwnedReadHalf>,
) -> anyhow::Result<Option<Envelope<T>>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(None);
    }
    let env: Envelope<T> = serde_json::from_str(line.trim_end_matches('\n'))?;
    Ok(Some(env))
}

/// Serialize the envelope as JSON followed by a newline and write it to
/// `writer`. Flushes after the write.
pub async fn write_envelope<T: Serialize, W: AsyncWrite + Unpin>(
    writer: &mut W,
    env: &Envelope<T>,
) -> anyhow::Result<()> {
    let mut buf = serde_json::to_vec(env)?;
    buf.push(b'\n');
    writer.write_all(&buf).await?;
    writer.flush().await?;
    Ok(())
}

