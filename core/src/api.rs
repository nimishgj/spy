use rspotify::AuthCodeSpotify;
use rspotify::clients::{BaseClient, OAuthClient};
use rspotify::model::Market;

use crate::error::{CoreError, Result};
use crate::model::*;

pub struct SpotifyApi {
    pub(crate) client: AuthCodeSpotify,
}

impl SpotifyApi {
    pub fn new(client: AuthCodeSpotify) -> Self {
        Self { client }
    }

    pub async fn liked_tracks(&self) -> Result<Vec<Track>> {
        let mut saved: Vec<rspotify::model::SavedTrack> = Vec::new();
        let mut offset: u32 = 0;
        loop {
            let page = self
                .client
                .current_user_saved_tracks_manual(Some(Market::FromToken), Some(50), Some(offset))
                .await
                .map_err(|e| CoreError::Api(e.to_string()))?;
            let len = page.items.len() as u32;
            saved.extend(page.items);
            if page.next.is_none() || len == 0 {
                break;
            }
            offset += len;
        }
        saved.sort_by_key(|b| std::cmp::Reverse(b.added_at));
        Ok(saved
            .into_iter()
            .map(convert::saved_track_to_model)
            .collect())
    }

    pub async fn saved_albums(&self) -> Result<Vec<Album>> {
        let mut out = Vec::new();
        let mut offset: u32 = 0;
        loop {
            let page = self
                .client
                .current_user_saved_albums_manual(Some(Market::FromToken), Some(50), Some(offset))
                .await
                .map_err(|e| CoreError::Api(e.to_string()))?;
            let len = page.items.len() as u32;
            out.extend(page.items.into_iter().map(convert::saved_album_to_model));
            if page.next.is_none() || len == 0 {
                break;
            }
            offset += len;
        }
        Ok(out)
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut out = Vec::new();
        let mut offset: u32 = 0;
        loop {
            let page = self
                .client
                .current_user_playlists_manual(Some(50), Some(offset))
                .await
                .map_err(|e| CoreError::Api(e.to_string()))?;
            let len = page.items.len() as u32;
            out.extend(
                page.items
                    .into_iter()
                    .map(convert::simplified_playlist_to_model),
            );
            if page.next.is_none() || len == 0 {
                break;
            }
            offset += len;
        }
        Ok(out)
    }

    pub async fn followed_artists(&self) -> Result<Vec<Artist>> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let page = self
                .client
                .current_user_followed_artists(after.as_deref(), Some(50))
                .await
                .map_err(|e| CoreError::Api(e.to_string()))?;
            let next_after = page.cursors.as_ref().and_then(|c| c.after.clone());
            let len = page.items.len();
            out.extend(page.items.into_iter().map(convert::full_artist_to_model));
            if len == 0 || page.next.is_none() {
                break;
            }
            match next_after {
                Some(a) => after = Some(a),
                None => break,
            }
        }
        Ok(out)
    }

    pub async fn recently_played(&self) -> Result<Vec<PlayHistoryEntry>> {
        let page = self
            .client
            .current_user_recently_played(Some(50), None)
            .await
            .map_err(|e| CoreError::Api(e.to_string()))?;
        Ok(page
            .items
            .into_iter()
            .map(convert::play_history_to_model)
            .collect())
    }

    pub async fn album_tracks(&self, id: &AlbumId) -> Result<Vec<Track>> {
        let parsed = rspotify::model::AlbumId::from_uri(&id.0)
            .or_else(|_| rspotify::model::AlbumId::from_id(&id.0))
            .map_err(|e| CoreError::Api(format!("bad album id {}: {e}", id.0)))?;
        let mut out = Vec::new();
        let mut offset: u32 = 0;
        loop {
            let page = self
                .client
                .album_track_manual(
                    parsed.clone(),
                    Some(Market::FromToken),
                    Some(50),
                    Some(offset),
                )
                .await
                .map_err(|e| CoreError::Api(e.to_string()))?;
            let len = page.items.len() as u32;
            for s in page.items {
                out.push(Track {
                    id: TrackId(s.id.map(|i| i.to_string()).unwrap_or_default()),
                    name: s.name,
                    artists: s.artists.into_iter().map(|a| a.name).collect(),
                    album: String::new(),
                    duration_ms: s.duration.num_milliseconds() as u32,
                });
            }
            if page.next.is_none() || len == 0 {
                break;
            }
            offset += len;
        }
        Ok(out)
    }

    pub async fn playlist_tracks(&self, id: &PlaylistId) -> Result<Vec<Track>> {
        let parsed = rspotify::model::PlaylistId::from_uri(&id.0)
            .or_else(|_| rspotify::model::PlaylistId::from_id(&id.0))
            .map_err(|e| CoreError::Api(format!("bad playlist id {}: {e}", id.0)))?;
        let mut out = Vec::new();
        let mut offset: u32 = 0;
        loop {
            let page = self
                .client
                .playlist_items_manual(
                    parsed.clone(),
                    None,
                    Some(Market::FromToken),
                    Some(50),
                    Some(offset),
                )
                .await
                .map_err(|e| CoreError::Api(e.to_string()))?;
            let len = page.items.len() as u32;
            for item in page.items {
                if let Some(rspotify::model::PlayableItem::Track(t)) = item.item {
                    out.push(convert::full_track_to_model(t));
                }
            }
            if page.next.is_none() || len == 0 {
                break;
            }
            offset += len;
        }
        Ok(out)
    }

    pub async fn search_tracks(&self, query: &str, limit: u32) -> Result<Vec<Track>> {
        let result = self
            .client
            .search(
                query,
                rspotify::model::SearchType::Track,
                Some(Market::FromToken),
                None,
                Some(limit),
                None,
            )
            .await
            .map_err(|e| CoreError::Api(e.to_string()))?;

        let tracks = match result {
            rspotify::model::SearchResult::Tracks(p) => p.items,
            _ => Vec::new(),
        };
        Ok(tracks
            .into_iter()
            .map(convert::full_track_to_model)
            .collect())
    }
}

pub mod convert {
    use crate::model::{
        Album, AlbumId, Artist, ArtistId, PlayHistoryEntry, Playlist, PlaylistId, Track, TrackId,
    };

    pub fn full_track_to_model(t: rspotify::model::FullTrack) -> Track {
        Track {
            id: TrackId(t.id.map(|i| i.to_string()).unwrap_or_default()),
            name: t.name,
            artists: t.artists.into_iter().map(|a| a.name).collect(),
            album: t.album.name,
            duration_ms: t.duration.num_milliseconds() as u32,
        }
    }

    pub fn saved_track_to_model(s: rspotify::model::SavedTrack) -> Track {
        full_track_to_model(s.track)
    }

    pub fn saved_album_to_model(s: rspotify::model::SavedAlbum) -> Album {
        let a = s.album;
        Album {
            id: AlbumId(a.id.to_string()),
            name: a.name,
            artists: a.artists.into_iter().map(|x| x.name).collect(),
            track_count: a.tracks.total,
        }
    }

    pub fn simplified_playlist_to_model(p: rspotify::model::SimplifiedPlaylist) -> Playlist {
        Playlist {
            id: PlaylistId(p.id.to_string()),
            name: p.name,
            owner: p.owner.display_name.unwrap_or_default(),
            track_count: p.items.total,
        }
    }

    pub fn full_artist_to_model(a: rspotify::model::FullArtist) -> Artist {
        Artist {
            id: ArtistId(a.id.to_string()),
            name: a.name,
        }
    }

    pub fn play_history_to_model(p: rspotify::model::PlayHistory) -> PlayHistoryEntry {
        PlayHistoryEntry {
            track: full_track_to_model(p.track),
            played_at: p.played_at,
        }
    }
}
