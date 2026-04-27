use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot_oauth::OAuthClientBuilder;
use rspotify::{
    AuthCodeSpotify, Config as RspotifyConfig, Credentials as RspotifyCredentials,
    Token as RspotifyToken,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::{CoreError, Result};
use crate::paths;

/// Default Spotify developer client_id used when `SPFY_CLIENT_ID` is unset.
/// This is the author's dev-app, which is in Development Mode and only
/// authorizes a small allowlist of users; downstream users should register
/// their own app at developer.spotify.com and export `SPFY_CLIENT_ID`.
const DEFAULT_SPFY_CLIENT_ID: &str = "84872059dc8545d9a664db22fa217733";

fn spfy_client_id() -> String {
    std::env::var("SPFY_CLIENT_ID").unwrap_or_else(|_| DEFAULT_SPFY_CLIENT_ID.to_string())
}

/// Spotify's official desktop-app client_id, used by librespot to authenticate
/// as a Spotify Connect device. Standard librespot practice — not a secret.
pub const SPOTIFY_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

const PLAYBACK_SCOPES: &[&str] = &["streaming"];

const API_SCOPES: &[&str] = &[
    "user-read-private",
    "user-library-read",
    "playlist-read-private",
    "playlist-read-collaborative",
    "user-follow-read",
    "user-read-recently-played",
    "user-read-playback-state",
    "user-modify-playback-state",
];

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub scopes: Vec<String>,
}

impl StoredToken {
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}

pub fn persist_rspotify_token(path: &Path, token: &StoredToken) -> Result<()> {
    let json = serde_json::to_string_pretty(token)?;
    std::fs::write(path, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn read_rspotify_token(path: &Path) -> Result<Option<StoredToken>> {
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(path)?;
    let token: StoredToken = serde_json::from_str(&json)?;
    Ok(Some(token))
}

fn random_redirect_uri() -> Result<String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| CoreError::Auth(format!("could not bind: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| CoreError::Auth(format!("addr: {e}")))?
        .port();
    drop(listener);
    Ok(format!("http://127.0.0.1:{port}/login"))
}

fn run_oauth(client_id: &str, scopes: &[&str]) -> Result<librespot_oauth::OAuthToken> {
    let redirect = random_redirect_uri()?;
    let builder = OAuthClientBuilder::new(client_id, &redirect, scopes.to_vec());
    let client = builder
        .build()
        .map_err(|e| CoreError::Auth(e.to_string()))?;
    client
        .get_access_token()
        .map_err(|e| CoreError::Auth(e.to_string()))
}

/// Get librespot credentials from cache or via OAuth.
pub fn librespot_credentials() -> Result<Credentials> {
    let cache_dir = paths::librespot_cache_dir()?;
    let cache = Cache::new(Some::<PathBuf>(cache_dir), None, None, None)
        .map_err(|e| CoreError::Auth(format!("cache: {e}")))?;

    if let Some(creds) = cache.credentials() {
        info!("using cached librespot credentials");
        return Ok(creds);
    }

    info!("running OAuth flow for librespot session");
    let token = run_oauth(SPOTIFY_CLIENT_ID, PLAYBACK_SCOPES)?;
    Ok(Credentials::with_access_token(token.access_token))
}

/// Get a valid rspotify token from cache (refreshing if needed) or via OAuth.
pub fn rspotify_token() -> Result<StoredToken> {
    let path = paths::rspotify_token_path()?;

    if let Some(t) = read_rspotify_token(&path)? {
        if !t.is_expired() {
            info!("using cached rspotify token");
            return Ok(t);
        }
        if let Some(refresh) = t.refresh_token.as_deref() {
            info!("refreshing expired rspotify token");
            if let Ok(refreshed) = refresh_with_token(refresh) {
                persist_rspotify_token(&path, &refreshed)?;
                return Ok(refreshed);
            }
        }
    }

    info!("running OAuth flow for rspotify token");
    let token = run_oauth(&spfy_client_id(), API_SCOPES)?;
    let stored = map_token(token);
    persist_rspotify_token(&path, &stored)?;
    Ok(stored)
}

fn refresh_with_token(refresh_token: &str) -> Result<StoredToken> {
    let redirect = random_redirect_uri()?;
    let builder = OAuthClientBuilder::new(&spfy_client_id(), &redirect, API_SCOPES.to_vec());
    let client = builder
        .build()
        .map_err(|e| CoreError::Auth(e.to_string()))?;
    let token = client
        .refresh_token(refresh_token)
        .map_err(|e| CoreError::Auth(e.to_string()))?;
    Ok(map_token(token))
}

fn map_token(token: librespot_oauth::OAuthToken) -> StoredToken {
    let remaining = token
        .expires_at
        .saturating_duration_since(std::time::Instant::now());
    let expires_at = chrono::Utc::now()
        + chrono::Duration::from_std(remaining).unwrap_or_else(|_| chrono::Duration::seconds(0));

    StoredToken {
        access_token: token.access_token,
        refresh_token: Some(token.refresh_token),
        expires_at,
        scopes: API_SCOPES.iter().map(|s| (*s).into()).collect(),
    }
}

pub struct Session {
    pub player_credentials: Credentials,
    pub api: AuthCodeSpotify,
}

pub fn login() -> Result<Session> {
    let player_credentials = librespot_credentials()?;
    let stored = rspotify_token()?;

    let scopes: HashSet<String> = stored.scopes.iter().cloned().collect();
    let token = RspotifyToken {
        access_token: stored.access_token,
        refresh_token: stored.refresh_token,
        expires_in: chrono::Duration::seconds(3600),
        expires_at: Some(stored.expires_at),
        scopes,
    };

    let creds = RspotifyCredentials {
        id: spfy_client_id(),
        secret: None,
    };
    let config = RspotifyConfig {
        token_cached: false,
        ..Default::default()
    };
    let api = AuthCodeSpotify::from_token_with_config(token, creds, Default::default(), config);

    Ok(Session {
        player_credentials,
        api,
    })
}
