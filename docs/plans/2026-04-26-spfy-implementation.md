# spfy Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build `spfy`, a Rust TUI Spotify client that browses your library (Liked / Albums / Playlists / Artists / Recently Played), drills into containers, searches tracks, and plays/pauses audio streamed by the binary itself via librespot.

**Architecture:** Cargo workspace. `spfy-core` (lib) holds auth, Web API wrapper, and the librespot player worker. `spfy` (bin) is a ratatui TUI consuming `core`'s public types only. Single tokio runtime owned by the binary; the player runs as a tokio task on it. One extra `std::thread` exists solely to read blocking `crossterm::event::read()`.

**Tech Stack:** Rust 2024, tokio, librespot 0.8, librespot-oauth, rspotify (`client-reqwest`), rodio, ratatui, crossterm, tracing, thiserror, anyhow, directories, serde.

**Reference design doc:** `docs/plans/2026-04-26-spfy-design.md` — the *why* lives there. This plan is the *how*.

---

## Conventions for every task

Each task follows the same micro-loop:

1. **Red** — write the failing test (or compile error if the task has no testable surface yet).
2. **Verify red** — run the command shown, confirm the failure mode matches expectation.
3. **Green** — write minimal code to make it pass.
4. **Verify green** — re-run.
5. **Commit** — single-line message: `git commit -m "<short subject>"`. Do NOT pass `-c user.name=...` flags; user identity is configured globally.

Some tasks (rendering, OAuth flow) cannot be unit-tested cleanly. Those tasks substitute a manual smoke check for the test loop, called out explicitly.

`cargo` commands run from the workspace root unless stated.

---

## Task 0: Register Spotify Developer app (manual, by user)

**This is a one-time manual step the user must do — the implementation cannot proceed without the client_id.**

**Steps:**

1. Open `https://developer.spotify.com/dashboard` and sign in.
2. Click **Create app**. Name: `spfy`. Description: `Personal TUI Spotify client`. Redirect URI: `http://127.0.0.1:8888/login` (the OAuth helper picks a random port at runtime, but a registered URI must exist; `127.0.0.1` host with any path matches in Spotify's matcher).
3. Under **Which API/SDKs are you planning to use**, tick **Web API**.
4. Save. Copy the **Client ID** displayed on the dashboard.
5. Under **User Management**, add the user's own Spotify account email to the test users list (Spotify defaults to Development Mode, capped at 25 users — the user must be on the list to authorize).

The Client ID is pasted into `core/src/auth.rs` in Task 7. Hold on to it.

---

## Task 1: Workspace skeleton

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `rust-toolchain.toml`
- Create: `.gitignore`

**Step 1: Write `Cargo.toml`:**

```toml
[workspace]
resolver = "2"
members = ["core", "tui"]

[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["nimishgj <nimisha@base14.io>"]
license = "MIT"

[workspace.dependencies]
tokio        = { version = "1", features = ["full"] }
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
anyhow       = "1"
thiserror    = "2"
tracing      = "0.1"
chrono       = { version = "0.4", features = ["serde"] }
directories  = "5"

[profile.release]
lto = true
codegen-units = 1
```

**Step 2: Write `rust-toolchain.toml`:**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

**Step 3: Write `.gitignore`:**

```
/target
Cargo.lock.bak
*.swp
.DS_Store
```

**Step 4: Verify it parses (no members exist yet, so `cargo` will complain — that's fine, we're checking syntax):**

Run: `cargo metadata --no-deps --format-version 1 2>&1 | head -3`
Expected: error mentioning `core/Cargo.toml` not found. Workspace TOML itself is valid.

**Step 5: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore
git commit -m "chore: scaffold cargo workspace"
```

---

## Task 2: Core crate skeleton

**Files:**
- Create: `core/Cargo.toml`
- Create: `core/src/lib.rs`

**Step 1: `core/Cargo.toml`:**

```toml
[package]
name = "spfy-core"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true
chrono.workspace = true
directories.workspace = true

librespot       = { version = "0.8", default-features = false, features = ["rodio-backend"] }
librespot-oauth = "0.8"
rspotify        = { version = "0.15", default-features = false, features = ["client-reqwest", "reqwest-rustls-tls"] }
rodio           = "0.20"
```

**Step 2: `core/src/lib.rs`:**

```rust
//! spfy-core: auth, Spotify Web API wrapper, librespot player worker.

pub mod model;
pub mod error;
pub mod paths;
pub mod auth;
pub mod api;
pub mod player;
```

We'll create the `mod` files as empty stubs in this same task so the crate compiles:

```bash
mkdir -p core/src
touch core/src/{model,error,paths,auth,api,player}.rs
```

**Step 3: Verify:**

Run: `cargo check -p spfy-core`
Expected: compiles cleanly (or fetches deps then compiles cleanly; first run will be slow).

**Step 4: Commit**

```bash
git add core/
git commit -m "feat(core): create spfy-core crate skeleton"
```

---

## Task 3: TUI binary skeleton

**Files:**
- Create: `tui/Cargo.toml`
- Create: `tui/src/main.rs`

**Step 1: `tui/Cargo.toml`:**

```toml
[package]
name = "spfy"
version.workspace = true
edition.workspace = true

[[bin]]
name = "spfy"
path = "src/main.rs"

[dependencies]
spfy-core = { path = "../core" }

tokio.workspace = true
anyhow.workspace = true
tracing.workspace = true
chrono.workspace = true

ratatui   = "0.29"
crossterm = "0.28"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

**Step 2: `tui/src/main.rs`:**

```rust
fn main() -> anyhow::Result<()> {
    println!("spfy v{}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
```

**Step 3: Verify:**

Run: `cargo run -p spfy`
Expected output: `spfy v0.1.0`

**Step 4: Commit**

```bash
git add tui/
git commit -m "feat(tui): create spfy binary skeleton"
```

---

## Task 4: Plain model types (`core/src/model.rs`)

**Files:**
- Modify: `core/src/model.rs`
- Create: `core/tests/model_test.rs`

**Step 1: Write a failing test first.**

`core/tests/model_test.rs`:

```rust
use spfy_core::model::{Track, TrackId};

#[test]
fn track_id_round_trips_through_string() {
    let id = TrackId("spotify:track:6rqhFgbbKwnb9MLmUQDhG6".to_string());
    assert_eq!(id.0, "spotify:track:6rqhFgbbKwnb9MLmUQDhG6");
}

#[test]
fn track_struct_can_be_constructed() {
    let t = Track {
        id: TrackId("spotify:track:abc".into()),
        name: "Bohemian Rhapsody".into(),
        artists: vec!["Queen".into()],
        album: "A Night at the Opera".into(),
        duration_ms: 354_000,
    };
    assert_eq!(t.duration_ms, 354_000);
    assert_eq!(t.artists, vec!["Queen".to_string()]);
}
```

**Step 2: Verify red.**

Run: `cargo test -p spfy-core --test model_test`
Expected: compile error — `Track`, `TrackId` not found.

**Step 3: Implement `core/src/model.rs`:**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlbumId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlaylistId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtistId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    pub id: AlbumId,
    pub name: String,
    pub artists: Vec<String>,
    pub track_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    pub owner: String,
    pub track_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub id: ArtistId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayHistoryEntry {
    pub track: Track,
    pub played_at: DateTime<Utc>,
}
```

**Step 4: Verify green.**

Run: `cargo test -p spfy-core --test model_test`
Expected: 2 passed.

**Step 5: Commit**

```bash
git add core/
git commit -m "feat(core): add plain model types"
```

---

## Task 5: Error type (`core/src/error.rs`)

**Files:**
- Modify: `core/src/error.rs`

No test needed — pure type definitions. We'll cover error handling at call sites.

**Step 1: Write `core/src/error.rs`:**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Spotify API error: {0}")]
    Api(String),

    #[error("Premium account required")]
    NotPremium,

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Player error: {0}")]
    Player(String),

    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
```

The reason `Http` and `Api` take `String` (not `#[from] reqwest::Error` / `rspotify::ClientError`) is so this enum has no public dependency on those crates — keeps the boundary clean. We stringify at the call site.

**Step 2: Verify:**

Run: `cargo check -p spfy-core`
Expected: compiles.

**Step 3: Commit**

```bash
git add core/
git commit -m "feat(core): add CoreError enum"
```

---

## Task 6: Path helpers (`core/src/paths.rs`)

**Files:**
- Modify: `core/src/paths.rs`
- Create: `core/tests/paths_test.rs`

**Step 1: Test first.**

`core/tests/paths_test.rs`:

```rust
use spfy_core::paths;

#[test]
fn config_dir_resolves_under_home() {
    let dir = paths::config_dir().expect("config dir");
    let s = dir.to_string_lossy();
    assert!(s.contains("spfy"), "expected 'spfy' in {s}");
}

#[test]
fn rspotify_token_path_is_under_config() {
    let token = paths::rspotify_token_path().unwrap();
    let cfg = paths::config_dir().unwrap();
    assert!(token.starts_with(&cfg));
    assert_eq!(token.file_name().unwrap(), "rspotify_token.json");
}
```

**Step 2: Verify red.**

Run: `cargo test -p spfy-core --test paths_test`
Expected: compile error.

**Step 3: Implement `core/src/paths.rs`:**

```rust
use std::path::PathBuf;

use directories::ProjectDirs;

use crate::error::{CoreError, Result};

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("io", "spfy", "spfy")
        .ok_or_else(|| CoreError::Io(std::io::Error::other("could not resolve project dirs")))
}

pub fn config_dir() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    let path = dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn librespot_cache_dir() -> Result<PathBuf> {
    let path = config_dir()?.join("librespot");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn rspotify_token_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("rspotify_token.json"))
}

pub fn log_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("spfy.log"))
}
```

**Step 4: Verify green.**

Run: `cargo test -p spfy-core --test paths_test`
Expected: 2 passed.

**Step 5: Commit**

```bash
git add core/
git commit -m "feat(core): add config path helpers"
```

---

## Task 7: Auth module (`core/src/auth.rs`)

OAuth flows can't be unit-tested without a real Spotify endpoint. We test the bits we *can* test (token JSON round-trip) and rely on a manual smoke check for the OAuth flow itself.

**Files:**
- Modify: `core/src/auth.rs`
- Create: `core/tests/auth_test.rs`

**Step 1: Test the persistence helpers.**

`core/tests/auth_test.rs`:

```rust
use chrono::{Duration, Utc};
use spfy_core::auth::{persist_rspotify_token, read_rspotify_token, StoredToken};

#[test]
fn token_json_round_trips_via_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token.json");

    let original = StoredToken {
        access_token: "AT-abc".into(),
        refresh_token: Some("RT-xyz".into()),
        expires_at: Utc::now() + Duration::seconds(3600),
        scopes: vec!["streaming".into(), "user-library-read".into()],
    };

    persist_rspotify_token(&path, &original).unwrap();
    let loaded = read_rspotify_token(&path).unwrap().expect("token present");

    assert_eq!(loaded.access_token, original.access_token);
    assert_eq!(loaded.refresh_token, original.refresh_token);
    assert_eq!(loaded.scopes, original.scopes);
}

#[test]
fn missing_token_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.json");
    let loaded = read_rspotify_token(&path).unwrap();
    assert!(loaded.is_none());
}
```

Add `tempfile = "3"` to `core/Cargo.toml` under `[dev-dependencies]`.

**Step 2: Verify red.**

Run: `cargo test -p spfy-core --test auth_test`
Expected: compile error.

**Step 3: Implement `core/src/auth.rs`:**

```rust
use std::path::Path;

use chrono::{DateTime, Utc};
use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot_oauth::OAuthClientBuilder;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::{CoreError, Result};
use crate::paths;

/// PASTE YOUR DEVELOPER CLIENT ID HERE after registering at developer.spotify.com.
pub const SPFY_CLIENT_ID: &str = "REPLACE_ME";

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
    let client = builder.build().map_err(|e| CoreError::Auth(e.to_string()))?;
    client
        .get_access_token()
        .map_err(|e| CoreError::Auth(e.to_string()))
}

/// Get librespot credentials from cache or via OAuth.
pub fn librespot_credentials() -> Result<Credentials> {
    let cache_dir = paths::librespot_cache_dir()?;
    let cache = Cache::new(Some(cache_dir), None, None, None)
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
    let token = run_oauth(SPFY_CLIENT_ID, API_SCOPES)?;
    let stored = map_token(token);
    persist_rspotify_token(&path, &stored)?;
    Ok(stored)
}

fn refresh_with_token(refresh_token: &str) -> Result<StoredToken> {
    let redirect = random_redirect_uri()?;
    let builder = OAuthClientBuilder::new(SPFY_CLIENT_ID, &redirect, API_SCOPES.to_vec());
    let client = builder.build().map_err(|e| CoreError::Auth(e.to_string()))?;
    let token = client
        .refresh_token(refresh_token)
        .map_err(|e| CoreError::Auth(e.to_string()))?;
    Ok(map_token(token))
}

fn map_token(token: librespot_oauth::OAuthToken) -> StoredToken {
    let expires_at = chrono::Utc::now()
        + chrono::Duration::from_std(
            token
                .expires_at
                .duration_since(std::time::Instant::now())
                .unwrap_or_default(),
        )
        .unwrap_or_else(|_| chrono::Duration::seconds(0));

    StoredToken {
        access_token: token.access_token,
        refresh_token: Some(token.refresh_token),
        expires_at,
        scopes: API_SCOPES.iter().map(|s| (*s).into()).collect(),
    }
}
```

**Step 4: Verify green.**

Run: `cargo test -p spfy-core --test auth_test`
Expected: 2 passed.

Run: `cargo check -p spfy-core`
Expected: compiles cleanly.

**Step 5: Paste real client_id.**

Replace `"REPLACE_ME"` in `SPFY_CLIENT_ID` with the value the user copied from the dashboard in Task 0. (Without this the rspotify OAuth leg will fail.)

**Step 6: Commit**

```bash
git add core/
git commit -m "feat(core): implement two-leg OAuth + token persistence"
```

---

## Task 8: `Session` struct and `login()` orchestrator

**Files:**
- Modify: `core/src/auth.rs`

**Step 1: Add to bottom of `core/src/auth.rs`:**

```rust
use rspotify::{AuthCodeSpotify, Token as RspotifyToken, scopes, Credentials as RspotifyCredentials, Config as RspotifyConfig};
use std::collections::HashSet;

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
        id: SPFY_CLIENT_ID.to_string(),
        secret: None,
    };
    let config = RspotifyConfig {
        token_cached: false,
        ..Default::default()
    };
    let api = AuthCodeSpotify::from_token_with_config(token, creds, Default::default(), config);

    Ok(Session { player_credentials, api })
}
```

**Step 2: Manual smoke test.**

Modify `tui/src/main.rs` temporarily to call `login()`:

```rust
fn main() -> anyhow::Result<()> {
    spfy_core::auth::login()?;
    println!("login OK");
    Ok(())
}
```

Run: `cargo run -p spfy`
Expected first run: two browser pop-ups, two "Agree" clicks, then `login OK`. Files appear under `~/Library/Application Support/spfy/`.

Run a second time: no browser, immediate `login OK`.

**Step 3: Revert main.rs to its previous body** (we'll wire the real entry point later):

```rust
fn main() -> anyhow::Result<()> {
    println!("spfy v{}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
```

**Step 4: Commit**

```bash
git add core/ tui/
git commit -m "feat(core): assemble Session via login()"
```

---

## Task 9: SpotifyApi wrapper + 401 retry harness

**Files:**
- Modify: `core/src/api.rs`
- Create: `core/tests/api_test.rs`

We test the *retry logic* using a mockable inner closure — testing rspotify itself adds nothing since it's already tested upstream.

**Step 1: Test first.**

`core/tests/api_test.rs`:

```rust
use std::cell::Cell;
use spfy_core::api::retry_once_on_auth;

#[test]
fn retry_succeeds_on_second_attempt() {
    let attempts = Cell::new(0);
    let result = retry_once_on_auth(|| {
        attempts.set(attempts.get() + 1);
        if attempts.get() == 1 {
            Err(spfy_core::error::CoreError::Auth("expired".into()))
        } else {
            Ok::<_, spfy_core::error::CoreError>(42)
        }
    });
    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempts.get(), 2);
}

#[test]
fn retry_propagates_non_auth_error_immediately() {
    let attempts = Cell::new(0);
    let result: Result<i32, _> = retry_once_on_auth(|| {
        attempts.set(attempts.get() + 1);
        Err(spfy_core::error::CoreError::Api("500".into()))
    });
    assert!(result.is_err());
    assert_eq!(attempts.get(), 1);
}
```

**Step 2: Verify red.**

Run: `cargo test -p spfy-core --test api_test`
Expected: compile error.

**Step 3: Implement skeleton in `core/src/api.rs`:**

```rust
use rspotify::AuthCodeSpotify;

use crate::error::{CoreError, Result};

pub struct SpotifyApi {
    pub(crate) client: AuthCodeSpotify,
}

impl SpotifyApi {
    pub fn new(client: AuthCodeSpotify) -> Self {
        Self { client }
    }
}

/// Run `f`; if it returns `CoreError::Auth(...)`, run it once more.
/// Useful when an access token has just expired mid-call.
pub fn retry_once_on_auth<T, F>(mut f: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    match f() {
        Err(CoreError::Auth(_)) => f(),
        other => other,
    }
}
```

**Step 4: Verify green.**

Run: `cargo test -p spfy-core --test api_test`
Expected: 2 passed.

**Step 5: Commit**

```bash
git add core/
git commit -m "feat(core): SpotifyApi wrapper + retry helper"
```

---

## Task 10: Library fetch methods (liked / albums / playlists / artists / recent)

These five methods follow the same shape: paginated rspotify call, convert each item into our model type. We'll implement them together in one task since the pattern repeats.

**Files:**
- Modify: `core/src/api.rs`
- Modify: `core/src/model.rs` — add `From<...>` impls? **No.** Keep conversions private to `api.rs` so the boundary holds.
- Create: `core/tests/api_conversion_test.rs`

**Step 1: Write a fixture-based conversion test.**

We'll test the rspotify `FullTrack` → our `Track` conversion using a hand-built rspotify struct (no JSON fixtures needed — rspotify's types are public).

`core/tests/api_conversion_test.rs`:

```rust
use rspotify::model::{
    AlbumId as RsAlbumId, ArtistId as RsArtistId, FullTrack, SavedTrack,
    SimplifiedAlbum, SimplifiedArtist, TrackId as RsTrackId,
};
use chrono::Utc;
use spfy_core::api::convert::{full_track_to_model, saved_track_to_model};

fn sample_full_track() -> FullTrack {
    FullTrack {
        id: Some(RsTrackId::from_id("6rqhFgbbKwnb9MLmUQDhG6").unwrap()),
        name: "Don't Stop Me Now".into(),
        artists: vec![SimplifiedArtist {
            id: Some(RsArtistId::from_id("1dfeR4HaWDbWqFHLkxsg1d").unwrap()),
            name: "Queen".into(),
            external_urls: Default::default(),
            href: None,
        }],
        album: SimplifiedAlbum {
            id: Some(RsAlbumId::from_id("1GbtB4zTqAsyfZEsm1RZfx").unwrap()),
            name: "Jazz".into(),
            artists: vec![],
            album_group: None,
            album_type: None,
            available_markets: vec![],
            external_urls: Default::default(),
            href: None,
            images: vec![],
            release_date: None,
            release_date_precision: None,
            restrictions: None,
            track_number: None,
        },
        available_markets: vec![],
        disc_number: 1,
        duration: chrono::TimeDelta::milliseconds(209_000),
        explicit: false,
        external_ids: Default::default(),
        external_urls: Default::default(),
        href: None,
        is_local: false,
        is_playable: Some(true),
        linked_from: None,
        restrictions: None,
        popularity: 0,
        preview_url: None,
        track_number: 1,
    }
}

#[test]
fn full_track_converts_to_model_track() {
    let model = full_track_to_model(sample_full_track());
    assert_eq!(model.name, "Don't Stop Me Now");
    assert_eq!(model.artists, vec!["Queen".to_string()]);
    assert_eq!(model.album, "Jazz");
    assert_eq!(model.duration_ms, 209_000);
    assert!(model.id.0.starts_with("spotify:track:"));
}

#[test]
fn saved_track_unwraps_to_full_track() {
    let st = SavedTrack {
        added_at: Utc::now(),
        track: sample_full_track(),
    };
    let model = saved_track_to_model(st);
    assert_eq!(model.name, "Don't Stop Me Now");
}
```

**Step 2: Verify red.**

Run: `cargo test -p spfy-core --test api_conversion_test`
Expected: compile error — `convert` module missing.

**Step 3: Implement the conversions and methods.**

Add to `core/src/api.rs`:

```rust
pub mod convert {
    use crate::model::{Album, AlbumId, Artist, ArtistId, PlayHistoryEntry, Playlist, PlaylistId, Track, TrackId};

    pub fn full_track_to_model(t: rspotify::model::FullTrack) -> Track {
        Track {
            id: TrackId(
                t.id
                    .map(|i| i.to_string()) // returns "spotify:track:..."
                    .unwrap_or_default(),
            ),
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
            track_count: p.tracks.total,
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

use rspotify::clients::OAuthClient;
use rspotify::model::{AlbumId as RsAlbumId, Market, PlaylistId as RsPlaylistId};

use crate::model::*;

impl SpotifyApi {
    pub async fn liked_tracks(&self) -> Result<Vec<Track>> {
        let mut out = Vec::new();
        let mut offset: u32 = 0;
        loop {
            let page = self
                .client
                .current_user_saved_tracks_manual(Some(Market::FromToken), Some(50), Some(offset))
                .await
                .map_err(|e| CoreError::Api(e.to_string()))?;
            let len = page.items.len() as u32;
            out.extend(page.items.into_iter().map(convert::saved_track_to_model));
            if page.next.is_none() || len == 0 {
                break;
            }
            offset += len;
        }
        Ok(out)
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
            out.extend(page.items.into_iter().map(convert::simplified_playlist_to_model));
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
            let last_id = page.items.last().map(|a| a.id.to_string());
            let len = page.items.len();
            out.extend(page.items.into_iter().map(convert::full_artist_to_model));
            if len == 0 || page.next.is_none() {
                break;
            }
            after = last_id.map(|id| id.trim_start_matches("spotify:artist:").to_string());
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
}
```

**Step 4: Verify green.**

Run: `cargo test -p spfy-core --test api_conversion_test`
Expected: 2 passed.

Run: `cargo check -p spfy-core`
Expected: clean.

**Step 5: Manual smoke test.**

Temporarily replace `tui/src/main.rs`:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let session = spfy_core::auth::login()?;
    let api = spfy_core::api::SpotifyApi::new(session.api);

    let liked = api.liked_tracks().await?;
    println!("Liked tracks: {}", liked.len());
    for t in liked.iter().take(5) {
        println!("  {} — {}", t.name, t.artists.join(", "));
    }
    Ok(())
}
```

Run: `cargo run -p spfy`
Expected: prints liked-track count and the first 5 tracks. **If you get 401**, run again — refresh logic should kick in.

**Step 6: Revert main.rs** to the simple `println!` body.

**Step 7: Commit**

```bash
git add core/
git commit -m "feat(core): library fetch methods + conversions"
```

---

## Task 11: `album_tracks` and `playlist_tracks`

**Files:**
- Modify: `core/src/api.rs`

No new test — these are the same shape as Task 10 methods. Conversion already covered.

**Step 1: Append to `impl SpotifyApi`:**

```rust
pub async fn album_tracks(&self, id: &AlbumId) -> Result<Vec<Track>> {
    let parsed = RsAlbumId::from_uri(&id.0)
        .or_else(|_| RsAlbumId::from_id(&id.0))
        .map_err(|e| CoreError::Api(format!("bad album id {}: {e}", id.0)))?;
    let mut out = Vec::new();
    let mut offset: u32 = 0;
    loop {
        let page = self
            .client
            .album_track_manual(parsed.clone(), Some(Market::FromToken), Some(50), Some(offset))
            .await
            .map_err(|e| CoreError::Api(e.to_string()))?;
        let len = page.items.len() as u32;
        for s in page.items {
            // SimplifiedTrack lacks album name; pull it from the parent album
            // For simplicity, we re-use a minimal conversion path:
            out.push(Track {
                id: TrackId(
                    s.id.map(|i| i.to_string()).unwrap_or_default(),
                ),
                name: s.name,
                artists: s.artists.into_iter().map(|a| a.name).collect(),
                album: String::new(),  // album name fetched separately if needed
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
    let parsed = RsPlaylistId::from_uri(&id.0)
        .or_else(|_| RsPlaylistId::from_id(&id.0))
        .map_err(|e| CoreError::Api(format!("bad playlist id {}: {e}", id.0)))?;
    let mut out = Vec::new();
    let mut offset: u32 = 0;
    loop {
        let page = self
            .client
            .playlist_items_manual(parsed.clone(), None, Some(Market::FromToken), Some(50), Some(offset))
            .await
            .map_err(|e| CoreError::Api(e.to_string()))?;
        let len = page.items.len() as u32;
        for item in page.items {
            if let Some(rspotify::model::PlayableItem::Track(t)) = item.track {
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
```

**Step 2: Verify:**

Run: `cargo check -p spfy-core`
Expected: clean.

**Step 3: Commit**

```bash
git add core/
git commit -m "feat(core): album_tracks + playlist_tracks"
```

---

## Task 12: `search_tracks`

**Files:**
- Modify: `core/src/api.rs`

**Step 1: Append:**

```rust
use rspotify::model::SearchType;

impl SpotifyApi {
    pub async fn search_tracks(&self, query: &str, limit: u32) -> Result<Vec<Track>> {
        let result = self
            .client
            .search(query, SearchType::Track, Some(Market::FromToken), None, Some(limit), None)
            .await
            .map_err(|e| CoreError::Api(e.to_string()))?;

        let tracks = match result {
            rspotify::model::SearchResult::Tracks(p) => p.items,
            _ => Vec::new(),
        };
        Ok(tracks.into_iter().map(convert::full_track_to_model).collect())
    }
}
```

(Take care: `impl SpotifyApi` block can be merged with the previous one — just keep one block. The split here is just for readability of the plan.)

**Step 2: Verify:**

Run: `cargo check -p spfy-core`
Expected: clean.

**Step 3: Commit**

```bash
git add core/
git commit -m "feat(core): search_tracks"
```

---

## Task 13: Player worker — types + pure queue logic

**Files:**
- Modify: `core/src/player.rs`
- Create: `core/tests/player_queue_test.rs`

**Step 1: Test pure queue/index behavior.**

`core/tests/player_queue_test.rs`:

```rust
use spfy_core::model::TrackId;
use spfy_core::player::queue::{Queue, AdvanceResult};

fn ids(strs: &[&str]) -> Vec<TrackId> {
    strs.iter().map(|s| TrackId((*s).into())).collect()
}

#[test]
fn play_context_loads_at_start_index() {
    let mut q = Queue::default();
    let current = q.set(ids(&["a", "b", "c"]), 1);
    assert_eq!(current.unwrap().0, "b");
}

#[test]
fn next_advances_or_stops() {
    let mut q = Queue::default();
    q.set(ids(&["a", "b"]), 0);
    assert_eq!(q.next(), AdvanceResult::Loaded(TrackId("b".into())));
    assert_eq!(q.next(), AdvanceResult::EndReached);
}

#[test]
fn prev_walks_backwards_clamped_to_zero() {
    let mut q = Queue::default();
    q.set(ids(&["a", "b", "c"]), 2);
    assert_eq!(q.previous(), AdvanceResult::Loaded(TrackId("b".into())));
    assert_eq!(q.previous(), AdvanceResult::Loaded(TrackId("a".into())));
    assert_eq!(q.previous(), AdvanceResult::Loaded(TrackId("a".into()))); // clamps
}
```

**Step 2: Verify red.**

Run: `cargo test -p spfy-core --test player_queue_test`
Expected: compile error.

**Step 3: Implement.** In `core/src/player.rs`:

```rust
use tokio::sync::mpsc;

use crate::model::TrackId;

pub mod queue {
    use crate::model::TrackId;

    #[derive(Default)]
    pub struct Queue {
        items: Vec<TrackId>,
        idx: usize,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum AdvanceResult {
        Loaded(TrackId),
        EndReached,
    }

    impl Queue {
        pub fn set(&mut self, items: Vec<TrackId>, start: usize) -> Option<TrackId> {
            self.items = items;
            self.idx = start.min(self.items.len().saturating_sub(1));
            self.items.get(self.idx).cloned()
        }

        pub fn current(&self) -> Option<&TrackId> {
            self.items.get(self.idx)
        }

        pub fn next(&mut self) -> AdvanceResult {
            if self.idx + 1 >= self.items.len() {
                return AdvanceResult::EndReached;
            }
            self.idx += 1;
            AdvanceResult::Loaded(self.items[self.idx].clone())
        }

        pub fn previous(&mut self) -> AdvanceResult {
            self.idx = self.idx.saturating_sub(1);
            match self.items.get(self.idx) {
                Some(t) => AdvanceResult::Loaded(t.clone()),
                None => AdvanceResult::EndReached,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Cmd {
    Play(TrackId),
    PlayContext { uris: Vec<TrackId>, start: usize },
    Toggle,
    Next,
    Previous,
    Seek(u32),
    SetVolume(u8),
    Quit,
}

#[derive(Debug, Clone)]
pub enum Event {
    Started { track: TrackId, duration_ms: u32 },
    Resumed,
    Paused,
    Position(u32),
    EndOfTrack,
    Stopped,
    Error(String),
}

pub struct PlayerHandle {
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    event_rx: Option<mpsc::UnboundedReceiver<Event>>,
}

impl PlayerHandle {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn take_events(&mut self) -> mpsc::UnboundedReceiver<Event> {
        self.event_rx
            .take()
            .expect("PlayerHandle::take_events called twice")
    }
}
```

**Step 4: Verify green.**

Run: `cargo test -p spfy-core --test player_queue_test`
Expected: 3 passed.

**Step 5: Commit**

```bash
git add core/
git commit -m "feat(core): player Cmd/Event types + queue logic"
```

---

## Task 14: Player worker — `spawn` and Session connect

**Files:**
- Modify: `core/src/player.rs`

**Step 1: Append to `core/src/player.rs`:**

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};

use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::SessionConfig;
use librespot::core::session::Session;
use librespot::core::spotify_id::SpotifyId;
use librespot::playback::audio_backend;
use librespot::playback::config::{PlayerConfig, VolumeCtrl};
use librespot::playback::mixer::softmixer::SoftMixer;
use librespot::playback::mixer::{Mixer, MixerConfig};
use librespot::playback::player::{Player, PlayerEvent};
use tokio::runtime::Handle;
use tracing::{error, info, warn};

use crate::paths;
use crate::player::queue::{AdvanceResult, Queue};

pub fn spawn(creds: Credentials, rt: Handle) -> PlayerHandle {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Cmd>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<Event>();

    rt.spawn(async move {
        if let Err(e) = run(creds, cmd_rx, event_tx.clone()).await {
            let _ = event_tx.send(Event::Error(e.to_string()));
        }
    });

    PlayerHandle {
        cmd_tx,
        event_rx: Some(event_rx),
    }
}

async fn run(
    creds: Credentials,
    mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
    event_tx: mpsc::UnboundedSender<Event>,
) -> anyhow::Result<()> {
    let cache_dir = paths::librespot_cache_dir()?;
    let cache = Cache::new(Some(cache_dir.clone()), None, Some(cache_dir), None)
        .map_err(|e| anyhow::anyhow!("cache: {e}"))?;
    let session = Session::new(SessionConfig::default(), Some(cache));

    info!("connecting to Spotify");
    if let Err(e) = session.connect(creds, true).await {
        let msg = e.to_string();
        if msg.to_lowercase().contains("premium") || msg.to_lowercase().contains("badcredentials") {
            let _ = event_tx.send(Event::Error("Premium account required".into()));
        } else {
            let _ = event_tx.send(Event::Error(format!("connect failed: {msg}")));
        }
        return Ok(());
    }

    // Continuation in Task 15.
    info!("session connected; player setup deferred to Task 15");
    let _ = event_tx;
    let _ = cmd_rx;
    let _ = session;
    Ok(())
}
```

**Step 2: Verify:**

Run: `cargo check -p spfy-core`
Expected: clean (warnings about unused are OK at this checkpoint).

**Step 3: Commit**

```bash
git add core/
git commit -m "feat(core): player worker spawn + session connect"
```

---

## Task 15: Player creation, command + event loop

**Files:**
- Modify: `core/src/player.rs`

**Step 1: Replace the body of `run` after `session.connect(...).await` succeeds with the full event loop:**

```rust
async fn run(
    creds: Credentials,
    mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
    event_tx: mpsc::UnboundedSender<Event>,
) -> anyhow::Result<()> {
    let cache_dir = paths::librespot_cache_dir()?;
    let cache = Cache::new(Some(cache_dir.clone()), None, Some(cache_dir), None)
        .map_err(|e| anyhow::anyhow!("cache: {e}"))?;
    let session = Session::new(SessionConfig::default(), Some(cache));

    if let Err(e) = session.connect(creds, true).await {
        let msg = e.to_string();
        let translated = if msg.to_lowercase().contains("premium")
            || msg.to_lowercase().contains("badcredentials")
        {
            "Premium account required".to_string()
        } else {
            format!("connect failed: {msg}")
        };
        let _ = event_tx.send(Event::Error(translated));
        return Ok(());
    }

    // ---- Audio backend + mixer ----
    let mixer = Box::new(SoftMixer::open(MixerConfig {
        volume_ctrl: VolumeCtrl::Linear,
        ..MixerConfig::default()
    })) as Box<dyn Mixer>;
    let backend = audio_backend::find(Some("rodio".into()))
        .ok_or_else(|| anyhow::anyhow!("rodio backend missing"))?;
    let player = Player::new(
        PlayerConfig::default(),
        session.clone(),
        mixer.get_soft_volume(),
        move || backend(None, librespot::playback::config::AudioFormat::default()),
    );

    let mut player_events = player.get_player_event_channel();
    let mut queue = Queue::default();
    let mut anchor: Option<(Instant, u32)> = None;
    let mut playing = false;
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    let player = Arc::new(player);

    let load = |player: &Arc<Player>, id: &TrackId| -> anyhow::Result<()> {
        let sid = SpotifyId::from_uri(&id.0)
            .map_err(|e| anyhow::anyhow!("bad uri {}: {e:?}", id.0))?;
        player.load(sid, true, 0);
        Ok(())
    };

    loop {
        tokio::select! {
            Some(cmd) = cmd_rx.recv() => match cmd {
                Cmd::Play(id) => {
                    queue.set(vec![id.clone()], 0);
                    if let Err(e) = load(&player, &id) {
                        let _ = event_tx.send(Event::Error(e.to_string()));
                    } else {
                        playing = true;
                    }
                }
                Cmd::PlayContext { uris, start } => {
                    if let Some(first) = queue.set(uris, start) {
                        if let Err(e) = load(&player, &first) {
                            let _ = event_tx.send(Event::Error(e.to_string()));
                        } else {
                            playing = true;
                        }
                    }
                }
                Cmd::Toggle => {
                    if playing { player.pause(); } else { player.play(); }
                    playing = !playing;
                    let _ = event_tx.send(if playing { Event::Resumed } else { Event::Paused });
                    if playing {
                        if let Some((_, pos)) = anchor.take() {
                            anchor = Some((Instant::now(), pos));
                        }
                    }
                }
                Cmd::Next => {
                    if let AdvanceResult::Loaded(id) = queue.next() {
                        let _ = load(&player, &id);
                        playing = true;
                    } else {
                        let _ = event_tx.send(Event::Stopped);
                        playing = false;
                    }
                }
                Cmd::Previous => {
                    if let AdvanceResult::Loaded(id) = queue.previous() {
                        let _ = load(&player, &id);
                        playing = true;
                    }
                }
                Cmd::Seek(ms) => {
                    player.seek(ms);
                    anchor = Some((Instant::now(), ms));
                }
                Cmd::SetVolume(v) => {
                    let scaled = (v as u32 * 65535 / 100).min(65535) as u16;
                    mixer.set_volume(scaled);
                }
                Cmd::Quit => break,
            },
            Some(ev) = player_events.recv() => match ev {
                PlayerEvent::TrackChanged { audio_item } => {
                    let id = TrackId(audio_item.track_id.to_uri().unwrap_or_default());
                    let dur = audio_item.duration_ms;
                    anchor = Some((Instant::now(), 0));
                    let _ = event_tx.send(Event::Started { track: id, duration_ms: dur });
                }
                PlayerEvent::Playing { position_ms, .. } => {
                    anchor = Some((Instant::now(), position_ms));
                    playing = true;
                    let _ = event_tx.send(Event::Resumed);
                }
                PlayerEvent::Paused { position_ms, .. } => {
                    anchor = Some((Instant::now(), position_ms));
                    playing = false;
                    let _ = event_tx.send(Event::Paused);
                }
                PlayerEvent::EndOfTrack { .. } => {
                    let _ = event_tx.send(Event::EndOfTrack);
                    if let AdvanceResult::Loaded(id) = queue.next() {
                        let _ = load(&player, &id);
                    } else {
                        playing = false;
                        let _ = event_tx.send(Event::Stopped);
                    }
                }
                PlayerEvent::Unavailable { .. } => {
                    warn!("track unavailable; skipping");
                    if let AdvanceResult::Loaded(id) = queue.next() {
                        let _ = load(&player, &id);
                    }
                }
                _ => {}
            },
            _ = tick.tick() => {
                if playing {
                    if let Some((started, base)) = anchor {
                        let elapsed = (Instant::now() - started).as_millis() as u32;
                        let _ = event_tx.send(Event::Position(base + elapsed));
                    }
                }
            }
        }
    }

    info!("player worker exiting");
    let _ = player.stop();
    Ok(())
}
```

(Note: librespot's `PlayerEvent` variants drift across versions. If this code fails to compile, run `cargo doc --open -p librespot-playback` and align variant names + fields with the installed version. The skeleton — match on event, update anchor, forward to TUI — stays the same.)

**Step 2: Verify:**

Run: `cargo check -p spfy-core`
Expected: clean.

**Step 3: Manual smoke test.**

Replace `tui/src/main.rs` temporarily:

```rust
use std::time::Duration;
use spfy_core::model::TrackId;
use spfy_core::player::{spawn, Cmd, Event};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let session = spfy_core::auth::login()?;
    let mut player = spawn(session.player_credentials, tokio::runtime::Handle::current());
    let mut events = player.take_events();

    // Replace with an actual track URI from your library:
    player.send(Cmd::Play(TrackId("spotify:track:6rqhFgbbKwnb9MLmUQDhG6".into())));

    let timeout = tokio::time::sleep(Duration::from_secs(15));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => break,
            Some(ev) = events.recv() => println!("{ev:?}"),
        }
    }

    player.send(Cmd::Quit);
    Ok(())
}
```

Run: `cargo run -p spfy`
Expected: you hear ~15 seconds of audio. Console prints `Started { ... }`, `Resumed`, `Position(...)` lines.

If silent: check macOS audio output device, check `cargo run` ran with no audio errors logged.

**Step 4: Revert `main.rs`.**

**Step 5: Commit**

```bash
git add core/
git commit -m "feat(core): player command + event loop"
```

---

## Task 16: TUI — terminal init, panic hook, minimal render loop

**Files:**
- Create: `tui/src/event.rs`
- Modify: `tui/src/main.rs`

**Step 1: Write `tui/src/event.rs`:**

```rust
use std::time::Duration;

use crossterm::event::{self, Event as CtEvent, KeyEvent};
use spfy_core::player::Event as PlayerEvent;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    Player(PlayerEvent),
}

pub fn channel() -> (mpsc::UnboundedSender<AppEvent>, mpsc::UnboundedReceiver<AppEvent>) {
    mpsc::unbounded_channel()
}

pub fn spawn_key_thread(tx: mpsc::UnboundedSender<AppEvent>) {
    std::thread::spawn(move || loop {
        match event::poll(Duration::from_millis(250)) {
            Ok(true) => match event::read() {
                Ok(CtEvent::Key(k)) => {
                    if tx.send(AppEvent::Key(k)).is_err() {
                        break;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    });
}

pub fn spawn_tick(tx: mpsc::UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;
            if tx.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });
}

pub fn spawn_player_forwarder(
    tx: mpsc::UnboundedSender<AppEvent>,
    mut player_rx: mpsc::UnboundedReceiver<PlayerEvent>,
) {
    tokio::spawn(async move {
        while let Some(ev) = player_rx.recv().await {
            if tx.send(AppEvent::Player(ev)).is_err() {
                break;
            }
        }
    });
}
```

**Step 2: Replace `tui/src/main.rs`:**

```rust
mod event;

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::event::{spawn_key_thread, spawn_tick, AppEvent};

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}

fn enter() -> Result<Tui> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(io::stdout()))?)
}

fn leave(mut term: Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    install_panic_hook();
    let mut term = enter()?;

    let (tx, mut rx) = event::channel();
    spawn_key_thread(tx.clone());
    spawn_tick(tx.clone());

    loop {
        term.draw(|f| {
            use ratatui::widgets::{Block, Borders, Paragraph};
            f.render_widget(
                Paragraph::new("spfy — press q to quit").block(Block::default().borders(Borders::ALL)),
                f.area(),
            );
        })?;

        match rx.recv().await {
            Some(AppEvent::Key(k)) if k.kind == KeyEventKind::Press => {
                if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
                    break;
                }
            }
            Some(_) => {}
            None => break,
        }
    }

    leave(term)?;
    Ok(())
}
```

**Step 3: Manual smoke test.**

Run: `cargo run -p spfy`
Expected: alternate-screen TUI shows a bordered "press q to quit" message. Press `q` → returns to shell cleanly. Press Ctrl+C while running → terminal recovers (panic hook test).

**Step 4: Commit**

```bash
git add tui/
git commit -m "feat(tui): terminal scaffold + event channel"
```

---

## Task 17: App state + reducer skeleton

**Files:**
- Create: `tui/src/app.rs`
- Modify: `tui/src/main.rs`
- Create: `tui/tests/app_test.rs`

**Step 1: Reducer test.**

`tui/tests/app_test.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use spfy::app::App;
use spfy::event::AppEvent;

fn key(c: char) -> AppEvent {
    AppEvent::Key(KeyEvent::new_with_kind(
        KeyCode::Char(c),
        KeyModifiers::NONE,
        KeyEventKind::Press,
    ))
}

#[test]
fn pressing_q_sets_should_quit() {
    let mut app = App::new();
    assert!(!app.should_quit);
    app.update(key('q'));
    assert!(app.should_quit);
}
```

For `spfy::app::App` to be importable from a test crate, expose the binary's modules through a small library shim. Add to `tui/Cargo.toml`:

```toml
[lib]
name = "spfy"
path = "src/lib.rs"
```

Create `tui/src/lib.rs`:

```rust
pub mod app;
pub mod event;
```

Modify `tui/src/main.rs` to use `spfy::*`:

```rust
use spfy::app::App;
use spfy::event::{spawn_key_thread, spawn_tick, AppEvent};
```

…and remove the duplicate `mod event;` declaration.

**Step 2: Verify red.**

Run: `cargo test -p spfy --test app_test`
Expected: compile error — `App` missing.

**Step 3: Implement `tui/src/app.rs`:**

```rust
use crossterm::event::KeyCode;

use crate::event::AppEvent;

pub struct App {
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self { should_quit: false }
    }

    pub fn update(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(k) => match k.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                _ => {}
            },
            AppEvent::Tick => {}
            AppEvent::Player(_) => {}
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
```

Wire it into `main.rs`'s loop, replacing the inline `match` block with `app.update(ev); if app.should_quit { break; }`.

**Step 4: Verify green.**

Run: `cargo test -p spfy --test app_test`
Expected: 1 passed.

Run: `cargo run -p spfy` — `q` still quits.

**Step 5: Commit**

```bash
git add tui/
git commit -m "feat(tui): App state + reducer"
```

---

## Task 18: App state — library sections + Mode enum

**Files:**
- Modify: `tui/src/app.rs`
- Modify: `tui/tests/app_test.rs`

**Step 1: Add reducer tests for tab navigation.**

Append to `tui/tests/app_test.rs`:

```rust
use spfy::app::{LibTab, Mode};

#[test]
fn tab_cycles_through_library_tabs() {
    let mut app = App::new();
    assert!(matches!(app.mode, Mode::Library { tab: LibTab::Liked, .. }));

    app.update(AppEvent::Key(KeyEvent::new_with_kind(
        KeyCode::Tab, KeyModifiers::NONE, KeyEventKind::Press,
    )));
    assert!(matches!(app.mode, Mode::Library { tab: LibTab::Albums, .. }));
}
```

**Step 2: Verify red.**

Run: `cargo test -p spfy --test app_test`
Expected: compile error.

**Step 3: Extend `tui/src/app.rs`:**

```rust
use ratatui::widgets::ListState;
use spfy_core::model::*;

pub enum SectionState<T> {
    Idle,
    Loading,
    Loaded(T),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibTab { Liked, Albums, Playlists, Artists, Recent }

impl LibTab {
    pub fn next(self) -> Self {
        match self {
            Self::Liked => Self::Albums,
            Self::Albums => Self::Playlists,
            Self::Playlists => Self::Artists,
            Self::Artists => Self::Recent,
            Self::Recent => Self::Liked,
        }
    }
    pub fn previous(self) -> Self {
        match self {
            Self::Liked => Self::Recent,
            Self::Albums => Self::Liked,
            Self::Playlists => Self::Albums,
            Self::Artists => Self::Playlists,
            Self::Recent => Self::Artists,
        }
    }
}

pub enum Mode {
    Library { tab: LibTab, list: ListState },
    Detail  { title: String, tracks: Vec<Track>, list: ListState, back: Box<Mode> },
    Search  { input: String, results: SectionState<Vec<Track>>, list: ListState },
}

pub struct App {
    pub now_playing: Option<Track>,
    pub is_playing: bool,
    pub position_ms: u32,
    pub volume: u8,

    pub liked:     SectionState<Vec<Track>>,
    pub albums:    SectionState<Vec<Album>>,
    pub playlists: SectionState<Vec<Playlist>>,
    pub artists:   SectionState<Vec<Artist>>,
    pub recent:    SectionState<Vec<PlayHistoryEntry>>,

    pub mode: Mode,
    pub toast: Option<(std::time::Instant, String)>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            now_playing: None,
            is_playing: false,
            position_ms: 0,
            volume: 70,
            liked: SectionState::Idle,
            albums: SectionState::Idle,
            playlists: SectionState::Idle,
            artists: SectionState::Idle,
            recent: SectionState::Idle,
            mode: Mode::Library { tab: LibTab::Liked, list: ListState::default() },
            toast: None,
            should_quit: false,
        }
    }

    pub fn update(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(k) if k.kind == crossterm::event::KeyEventKind::Press => {
                self.handle_key(k);
            }
            AppEvent::Tick => self.clear_stale_toast(),
            _ => {}
        }
    }

    fn handle_key(&mut self, k: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode::*;
        use crossterm::event::KeyModifiers as Mods;

        match (&mut self.mode, k.code, k.modifiers) {
            (_, Char('q'), _) | (_, Esc, _) if !matches!(self.mode, Mode::Search { .. }) => {
                self.should_quit = true;
            }
            (Mode::Library { tab, .. }, Tab, m) if !m.contains(Mods::SHIFT) => *tab = tab.next(),
            (Mode::Library { tab, .. }, BackTab, _) => *tab = tab.previous(),
            (Mode::Library { tab, .. }, Tab, m) if m.contains(Mods::SHIFT) => *tab = tab.previous(),
            _ => {}
        }
    }

    fn clear_stale_toast(&mut self) {
        if let Some((at, _)) = self.toast {
            if at.elapsed() > std::time::Duration::from_secs(5) {
                self.toast = None;
            }
        }
    }
}
```

**Step 4: Verify green.**

Run: `cargo test -p spfy --test app_test`
Expected: 2 passed.

**Step 5: Commit**

```bash
git add tui/
git commit -m "feat(tui): App library sections + tab navigation"
```

---

## Task 19: Render skeleton — library tabs + list

**Files:**
- Create: `tui/src/ui.rs`
- Modify: `tui/src/lib.rs`
- Modify: `tui/src/main.rs`

This is render code — no unit tests, manual smoke instead.

**Step 1: Add `pub mod ui;` to `tui/src/lib.rs`.**

**Step 2: `tui/src/ui.rs`:**

```rust
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, LibTab, Mode, SectionState};

pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Min(1),    // body
            Constraint::Length(3), // now-playing
            Constraint::Length(1), // help
        ])
        .split(f.area());

    render_tabs(f, chunks[0], app);
    render_body(f, chunks[1], app);
    render_now_playing(f, chunks[2], app);
    render_help(f, chunks[3]);
}

fn render_tabs(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let titles = ["Liked", "Albums", "Playlists", "Artists", "Recent"];
    let selected = match &app.mode {
        Mode::Library { tab, .. } => match tab {
            LibTab::Liked => 0, LibTab::Albums => 1, LibTab::Playlists => 2,
            LibTab::Artists => 3, LibTab::Recent => 4,
        },
        _ => 0,
    };
    let tabs = Tabs::new(titles.iter().map(|t| Line::from(*t)).collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL))
        .select(selected)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED));
    f.render_widget(tabs, area);
}

fn render_body(f: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    match &mut app.mode {
        Mode::Library { tab, list } => {
            let items: Vec<ListItem> = match tab {
                LibTab::Liked => section_items(&app.liked, |t| format!("{} — {}", t.name, t.artists.join(", "))),
                LibTab::Albums => section_items(&app.albums, |a| format!("{} — {}", a.name, a.artists.join(", "))),
                LibTab::Playlists => section_items(&app.playlists, |p| format!("{} ({} tracks)", p.name, p.track_count)),
                LibTab::Artists => section_items(&app.artists, |a| a.name.clone()),
                LibTab::Recent => section_items(&app.recent, |e| format!("{} — {}", e.track.name, e.track.artists.join(", "))),
            };
            let list_widget = List::new(items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            f.render_stateful_widget(list_widget, area, list);
        }
        Mode::Detail { title, tracks, list, .. } => {
            let items: Vec<ListItem> = tracks
                .iter()
                .map(|t| ListItem::new(format!("{} — {}", t.name, t.artists.join(", "))))
                .collect();
            let list_widget = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(title.clone()))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            f.render_stateful_widget(list_widget, area, list);
        }
        Mode::Search { input, results, list } => {
            let inner = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(area);
            let prompt = Paragraph::new(format!("/ {input}"))
                .block(Block::default().borders(Borders::ALL).title("Search"));
            f.render_widget(prompt, inner[0]);
            let items: Vec<ListItem> = match results {
                SectionState::Loaded(tracks) => tracks
                    .iter()
                    .map(|t| ListItem::new(format!("{} — {}", t.name, t.artists.join(", "))))
                    .collect(),
                SectionState::Loading => vec![ListItem::new("Loading…")],
                SectionState::Failed(e) => vec![ListItem::new(format!("Error: {e}"))],
                SectionState::Idle => vec![ListItem::new("Type a query and press Enter")],
            };
            let list_widget = List::new(items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            f.render_stateful_widget(list_widget, inner[1], list);
        }
    }
}

fn section_items<T>(state: &SectionState<Vec<T>>, fmt: impl Fn(&T) -> String) -> Vec<ListItem<'static>> {
    match state {
        SectionState::Idle => vec![ListItem::new("Idle")],
        SectionState::Loading => vec![ListItem::new("Loading…")],
        SectionState::Failed(e) => vec![ListItem::new(format!("Error: {e}"))],
        SectionState::Loaded(items) if items.is_empty() => vec![ListItem::new("(empty)")],
        SectionState::Loaded(items) => items.iter().map(|x| ListItem::new(fmt(x))).collect(),
    }
}

fn render_now_playing(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let text = match &app.now_playing {
        Some(t) => {
            let icon = if app.is_playing { "▶" } else { "⏸" };
            let pos = format_ms(app.position_ms);
            format!("{icon} {} — {}   {pos}", t.name, t.artists.join(", "))
        }
        None => "(nothing playing)".to_string(),
    };
    let para = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    f.render_widget(para, area);
}

fn render_help(f: &mut Frame, area: ratatui::layout::Rect) {
    let p = Paragraph::new("j/k navigate · Tab next pane · ⏎ play · p pause · / search · q quit");
    f.render_widget(p, area);
}

fn format_ms(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}
```

**Step 3: Replace the inline draw closure in `tui/src/main.rs`:**

```rust
term.draw(|f| crate::ui::render(f, &mut app))?;
```

…and add `pub mod ui;` at the top of `tui/src/main.rs`. Also import `App` — `let mut app = App::new();` before the loop.

**Step 4: Manual smoke test.**

Run: `cargo run -p spfy`
Expected: tabs strip on top, "Idle" placeholder list, now-playing strip, help line. `Tab` cycles tabs. `q` quits.

**Step 5: Commit**

```bash
git add tui/
git commit -m "feat(tui): render library tabs + section placeholders"
```

---

## Task 20: List cursor navigation (j/k)

**Files:**
- Modify: `tui/src/app.rs`
- Modify: `tui/tests/app_test.rs`

**Step 1: Test.**

Append to `app_test.rs`:

```rust
#[test]
fn j_moves_cursor_down_when_section_loaded() {
    use spfy_core::model::{Track, TrackId};
    let mut app = App::new();
    app.liked = spfy::app::SectionState::Loaded(vec![
        Track { id: TrackId("a".into()), name: "A".into(), artists: vec![], album: "".into(), duration_ms: 0 },
        Track { id: TrackId("b".into()), name: "B".into(), artists: vec![], album: "".into(), duration_ms: 0 },
    ]);
    app.update(key('j'));
    if let Mode::Library { list, .. } = &app.mode {
        assert_eq!(list.selected(), Some(1));
    } else { panic!() }
}
```

**Step 2: Verify red.**

Run: `cargo test -p spfy --test app_test`

**Step 3: Implement.** In `App::handle_key`, add:

```rust
(Mode::Library { tab, list }, Char('j') | Down, _) => move_cursor_in_library(tab, list, app_section_lengths(app), 1),
(Mode::Library { tab, list }, Char('k') | Up, _) => move_cursor_in_library(tab, list, app_section_lengths(app), -1),
```

This won't compile because `app` isn't accessible inside the borrow. Restructure: split key handling into `handle_navigation_key` that takes `&mut self` and looks at `self.mode` plus `self.section_len(tab)`. Concretely:

```rust
impl App {
    fn section_len(&self, tab: LibTab) -> usize {
        match tab {
            LibTab::Liked => loaded_len(&self.liked),
            LibTab::Albums => loaded_len(&self.albums),
            LibTab::Playlists => loaded_len(&self.playlists),
            LibTab::Artists => loaded_len(&self.artists),
            LibTab::Recent => loaded_len(&self.recent),
        }
    }
}

fn loaded_len<T>(s: &SectionState<Vec<T>>) -> usize {
    match s { SectionState::Loaded(v) => v.len(), _ => 0 }
}

fn move_cursor(list: &mut ratatui::widgets::ListState, len: usize, delta: i32) {
    if len == 0 { list.select(None); return; }
    let cur = list.selected().unwrap_or(0) as i32;
    let next = (cur + delta).rem_euclid(len as i32) as usize;
    list.select(Some(next));
}
```

In `handle_key`, before matching on mode, compute `let lib_len = if let Mode::Library { tab, .. } = self.mode { Some(self.section_len(tab)) } else { None };` then inside the `Library` branch use `move_cursor(list, lib_len.unwrap(), 1)`.

**Step 4: Verify green.**

Run: `cargo test -p spfy --test app_test`
Expected: all passing.

**Step 5: Manual smoke.**

Run: `cargo run -p spfy`
Press `j`/`k` — selection moves once we have data (still no real data — selection just moves on the empty placeholder, which is fine for now; will validate fully in Task 21).

**Step 6: Commit**

```bash
git add tui/
git commit -m "feat(tui): list cursor navigation"
```

---

## Task 21: Login + initial library load + wire to UI

**Files:**
- Modify: `tui/src/main.rs`
- Modify: `tui/src/event.rs`
- Modify: `tui/src/app.rs`

This is where we glue auth → api → 5 parallel loads → reducer → render.

**Step 1: Extend `AppEvent`:**

```rust
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    Player(PlayerEvent),
    LibraryLoaded(LibrarySection),
    LibraryFailed(LibrarySection, String),
}

pub enum LibrarySection {
    Liked(Vec<spfy_core::model::Track>),
    Albums(Vec<spfy_core::model::Album>),
    Playlists(Vec<spfy_core::model::Playlist>),
    Artists(Vec<spfy_core::model::Artist>),
    Recent(Vec<spfy_core::model::PlayHistoryEntry>),
}
```

(`LibraryFailed` carries an empty payload — promote to a separate `LibrarySectionId` enum if the borrow checker complains. Pragmatic shape: have `LibraryLoaded(LibrarySection)` carry the loaded data; have `LibraryFailed(LibrarySectionId, String)` carry just the id. Adjust accordingly.)

**Step 2: In `App::update`, handle the new variants:**

```rust
AppEvent::LibraryLoaded(section) => match section {
    LibrarySection::Liked(v) => self.liked = SectionState::Loaded(v),
    LibrarySection::Albums(v) => self.albums = SectionState::Loaded(v),
    LibrarySection::Playlists(v) => self.playlists = SectionState::Loaded(v),
    LibrarySection::Artists(v) => self.artists = SectionState::Loaded(v),
    LibrarySection::Recent(v) => self.recent = SectionState::Loaded(v),
},
AppEvent::LibraryFailed(id, msg) => {
    let _ = (id, msg); // store on the relevant SectionState as Failed(msg)
}
```

**Step 3: In `main`, after entering raw mode:**

```rust
let session = spfy_core::auth::login()?;
let api = std::sync::Arc::new(spfy_core::api::SpotifyApi::new(session.api));

// Mark all sections Loading and dispatch fetches.
app.liked = spfy::app::SectionState::Loading;
app.albums = spfy::app::SectionState::Loading;
app.playlists = spfy::app::SectionState::Loading;
app.artists = spfy::app::SectionState::Loading;
app.recent = spfy::app::SectionState::Loading;

{
    let api = api.clone(); let tx = tx.clone();
    tokio::spawn(async move {
        let _ = match api.liked_tracks().await {
            Ok(v) => tx.send(AppEvent::LibraryLoaded(LibrarySection::Liked(v))),
            Err(e) => tx.send(AppEvent::LibraryFailed(LibrarySectionId::Liked, e.to_string())),
        };
    });
}
// Repeat for albums, playlists, artists, recent.
```

(Five copies of the spawn block — verbose but straightforward.)

**Step 4: Manual smoke test.**

Run: `cargo run -p spfy`
Expected: TUI launches, login completes (browser if cold cache), tab strip shows; each tab shows "Loading…" briefly then real data. `j`/`k` moves selection through real tracks. `Tab` cycles tabs.

**Step 5: Commit**

```bash
git add tui/
git commit -m "feat(tui): initial parallel library load"
```

---

## Task 22: Drill into album / playlist on Enter

**Files:**
- Modify: `tui/src/app.rs`
- Modify: `tui/src/event.rs`
- Modify: `tui/src/main.rs`

**Step 1: Extend `AppEvent`:**

```rust
DetailLoaded { title: String, tracks: Vec<spfy_core::model::Track> },
DetailFailed(String),
```

Add a new field on `App`: `pending_detail: Option<&'static str>` (or just rely on a toast).

**Step 2: In `App::handle_key`, on Enter inside `Mode::Library`:**

- If on Albums tab: read selected `Album`, emit a request through a side channel (`requests` tx) carrying `DetailRequest::Album(id)`.
- If on Playlists tab: same with `DetailRequest::Playlist(id)`.
- Else (Liked / Recent): play the track (Task 24 will hook this up).

To avoid ratatui-unfriendly side effects in `update`, give `App` a `pending: Vec<UiAction>` queue:

```rust
pub enum UiAction {
    LoadAlbumTracks(spfy_core::model::AlbumId),
    LoadPlaylistTracks(spfy_core::model::PlaylistId),
    Play(spfy_core::model::TrackId),
    PlayContext { uris: Vec<spfy_core::model::TrackId>, start: usize },
    Toggle, Next, Previous, VolumeUp, VolumeDown,
    Search(String),
}

pub struct App { /* ... */ pub pending: Vec<UiAction>, }
```

After `app.update(ev)`, the main loop drains `app.pending` and dispatches each:

```rust
for action in std::mem::take(&mut app.pending) {
    match action {
        UiAction::LoadAlbumTracks(id) => { /* spawn api.album_tracks(id) */ }
        UiAction::LoadPlaylistTracks(id) => { /* spawn api.playlist_tracks(id) */ }
        // ...
    }
}
```

**Step 3: Implement Enter handler** in `handle_key`:

```rust
(Mode::Library { tab, list }, Enter, _) => {
    let Some(idx) = list.selected() else { return };
    match tab {
        LibTab::Albums => if let SectionState::Loaded(v) = &self.albums {
            if let Some(a) = v.get(idx) {
                self.pending.push(UiAction::LoadAlbumTracks(a.id.clone()));
            }
        }
        LibTab::Playlists => if let SectionState::Loaded(v) = &self.playlists {
            if let Some(p) = v.get(idx) {
                self.pending.push(UiAction::LoadPlaylistTracks(p.id.clone()));
            }
        }
        LibTab::Liked => if let SectionState::Loaded(v) = &self.liked {
            let uris: Vec<_> = v.iter().map(|t| t.id.clone()).collect();
            self.pending.push(UiAction::PlayContext { uris, start: idx });
        }
        LibTab::Recent => if let SectionState::Loaded(v) = &self.recent {
            if let Some(e) = v.get(idx) {
                self.pending.push(UiAction::Play(e.track.id.clone()));
            }
        }
        LibTab::Artists => { /* drilling into artists out of scope v1 */ }
    }
}
```

**Step 4: In `App::update`, on `DetailLoaded`:**

```rust
AppEvent::DetailLoaded { title, tracks } => {
    let prev = std::mem::replace(&mut self.mode, Mode::Library {
        tab: LibTab::Liked, list: ListState::default(),
    });
    self.mode = Mode::Detail {
        title, tracks, list: ListState::default(), back: Box::new(prev),
    };
}
```

On Esc inside Detail: restore `*back`.

**Step 5: Manual smoke.**

Run, navigate to Albums, Enter on one — title strip changes to album name, list shows track names. Esc returns. Same with Playlists.

**Step 6: Commit**

```bash
git add tui/
git commit -m "feat(tui): drill into albums and playlists"
```

---

## Task 23: Player wiring — Enter plays, p toggles, n/b skips, +/- volume

**Files:**
- Modify: `tui/src/main.rs`
- Modify: `tui/src/app.rs`

**Step 1: Extend `App::handle_key`** to push the appropriate `UiAction` for `p`, `Space`, `n`, `b`, `+`, `-`. Also handle Enter inside `Mode::Detail` → `UiAction::PlayContext { uris: tracks_in_view, start: idx }`.

**Step 2: In `main.rs`**, after spawning the player and capturing its handle:

```rust
let mut player = spfy_core::player::spawn(session.player_credentials, tokio::runtime::Handle::current());
spfy::event::spawn_player_forwarder(tx.clone(), player.take_events());
```

In the action drain loop:

```rust
match action {
    UiAction::Play(id) => player.send(spfy_core::player::Cmd::Play(id)),
    UiAction::PlayContext { uris, start } => player.send(
        spfy_core::player::Cmd::PlayContext { uris, start }
    ),
    UiAction::Toggle => player.send(spfy_core::player::Cmd::Toggle),
    UiAction::Next => player.send(spfy_core::player::Cmd::Next),
    UiAction::Previous => player.send(spfy_core::player::Cmd::Previous),
    UiAction::VolumeUp => {
        app.volume = (app.volume + 5).min(100);
        player.send(spfy_core::player::Cmd::SetVolume(app.volume));
    }
    UiAction::VolumeDown => {
        app.volume = app.volume.saturating_sub(5);
        player.send(spfy_core::player::Cmd::SetVolume(app.volume));
    }
    UiAction::LoadAlbumTracks(id) => { /* fire-and-forget spawn calling api.album_tracks */ }
    UiAction::LoadPlaylistTracks(id) => { /* same */ }
    UiAction::Search(q) => { /* same, calls api.search_tracks */ }
}
```

**Step 3: Handle `AppEvent::Player`** in the reducer:

```rust
AppEvent::Player(ev) => match ev {
    spfy_core::player::Event::Started { track, duration_ms } => {
        // Look up the track from current view to populate now_playing
        self.now_playing = self.find_track_by_id(&track);
        self.is_playing = true;
        self.position_ms = 0;
    }
    spfy_core::player::Event::Position(ms) => self.position_ms = ms,
    spfy_core::player::Event::Paused => self.is_playing = false,
    spfy_core::player::Event::Resumed => self.is_playing = true,
    spfy_core::player::Event::Stopped => { self.is_playing = false; self.now_playing = None; }
    spfy_core::player::Event::Error(msg) => {
        if msg.contains("Premium") {
            self.toast = Some((std::time::Instant::now(), msg));
            self.should_quit = true;
        } else {
            self.toast = Some((std::time::Instant::now(), msg));
        }
    }
    _ => {}
},
```

`find_track_by_id` walks `liked`, `recent`, `Mode::Detail.tracks`, search results — first match wins. If none found, set `now_playing = Some(Track { name: "Unknown", ... })` so the strip still shows something.

**Step 4: On Quit**, before exiting the loop:

```rust
player.send(spfy_core::player::Cmd::Quit);
```

**Step 5: Manual smoke.**

Run, navigate to Liked, press Enter on a track — audio starts, now-playing strip updates, position ticks. `p` pauses; `p` again resumes. `n` skips; `b` goes back. `+`/`-` changes volume.

**Step 6: Commit**

```bash
git add tui/
git commit -m "feat(tui): wire player commands and events"
```

---

## Task 24: Search mode

**Files:**
- Modify: `tui/src/app.rs`
- Modify: `tui/src/event.rs`
- Modify: `tui/src/main.rs`

**Step 1: Extend `AppEvent`:**

```rust
SearchResult(Vec<spfy_core::model::Track>),
SearchFailed(String),
```

**Step 2: In `handle_key`**, add a `/` handler outside any mode that switches to `Mode::Search { input: String::new(), results: SectionState::Idle, list: ListState::default() }`. Inside `Mode::Search`:

- Char(c) → input.push(c)
- Backspace → input.pop()
- Enter → if non-empty, `pending.push(UiAction::Search(input.clone()))` and set results to Loading
- Enter on a result row → `UiAction::Play(...)` and exit search
- Esc → restore previous mode (use a `back` field similar to Detail)

**Step 3: In `App::update`:**

```rust
AppEvent::SearchResult(tracks) => {
    if let Mode::Search { results, .. } = &mut self.mode {
        *results = SectionState::Loaded(tracks);
    }
}
AppEvent::SearchFailed(msg) => {
    if let Mode::Search { results, .. } = &mut self.mode {
        *results = SectionState::Failed(msg);
    }
}
```

**Step 4: In `main.rs` action handler, on `UiAction::Search(q)`:**

```rust
let api = api.clone(); let tx = tx.clone();
tokio::spawn(async move {
    let _ = match api.search_tracks(&q, 50).await {
        Ok(v) => tx.send(AppEvent::SearchResult(v)),
        Err(e) => tx.send(AppEvent::SearchFailed(e.to_string())),
    };
});
```

**Step 5: Manual smoke.**

Run, press `/`, type `bohemian rhapsody`, Enter — results appear. `j`/`k` to navigate, Enter to play.

**Step 6: Commit**

```bash
git add tui/
git commit -m "feat(tui): search mode"
```

---

## Task 25: Toast rendering + fatal-error screen

**Files:**
- Modify: `tui/src/ui.rs`
- Modify: `tui/src/app.rs`

**Step 1: Render the toast** in `render_now_playing` — append `app.toast` text on a separate line if present.

**Step 2: Add a fatal-error mode.** Extend `App` with `fatal: Option<String>`. When `fatal` is `Some`, `ui::render` overlays a centered Paragraph with the message and `q to quit`.

In the player error handler, set `fatal = Some(msg)` for `"Premium account required"` instead of `should_quit = true`.

**Step 3: Manual smoke.**

Hard-code a fake fatal error in `main.rs` for one run to confirm the layout looks right, then remove.

**Step 4: Commit**

```bash
git add tui/
git commit -m "feat(tui): toast + fatal-error overlay"
```

---

## Task 26: Tracing to file

**Files:**
- Modify: `tui/src/main.rs`

**Step 1: Add at top of `main`, before terminal setup:**

```rust
use tracing_subscriber::EnvFilter;

let log_path = spfy_core::paths::log_path()?;
let log_file = std::fs::File::create(&log_path)?;
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,librespot=warn".into()))
    .with_writer(log_file)
    .with_ansi(false)
    .init();
tracing::info!("spfy starting");
```

**Step 2: Manual smoke.**

Run, quit, `cat ~/Library/Application\ Support/spfy/spfy.log` — should show structured log lines including the player worker's connect log.

**Step 3: Commit**

```bash
git add tui/
git commit -m "feat(tui): tracing to file"
```

---

## Task 27: README with manual smoke script

**Files:**
- Create: `README.md`

**Step 1: Write a minimal README:**

```markdown
# spfy

A small Rust TUI Spotify client. Browse your library, search, play music.
Inspired by ncspot.

## Requirements

- macOS (Linux/Windows untested in v1)
- Spotify **Premium** account
- A Spotify Developer app (one-time setup)

## Setup

1. Visit https://developer.spotify.com/dashboard.
2. Create app: name `spfy`, redirect URI `http://127.0.0.1:8888/login`,
   API: Web API.
3. Add your Spotify account email to the app's user list.
4. Open `core/src/auth.rs` and replace `REPLACE_ME` in `SPFY_CLIENT_ID` with
   the client_id shown on the dashboard.

## Build & run

```
cargo run --release -p spfy
```

First run opens two browser pop-ups for OAuth (one for streaming, one for
the Web API). Tokens cache under `~/Library/Application Support/spfy/`.

## Smoke test

1. Launch — TUI shows tabs (Liked / Albums / Playlists / Artists / Recent).
2. `j`/`k` to navigate.
3. `Tab` / `Shift+Tab` cycles tabs.
4. `Enter` on a Liked track — audio plays.
5. `p` pauses, `p` again resumes.
6. `n` / `b` to skip.
7. `+` / `-` changes volume.
8. `/` enters search; type a query, `Enter`; `j`/`k` and `Enter` to play.
9. `Esc` exits search or Detail.
10. `q` quits.

## Logs

`~/Library/Application Support/spfy/spfy.log`. Override level:
`RUST_LOG=spfy=debug,librespot=warn cargo run -p spfy`.
```

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README + smoke test script"
```

---

## Wrap-up

After Task 27, the smoke test script in the README is the acceptance gate. If every step works, v1 is shipped.

Known follow-ups (out of scope for this plan):
- Drill into Artists tab (top tracks for an artist).
- "Add to queue" UI.
- Repeat / shuffle.
- Library on-disk cache for faster startup.
- Linux/Windows audio backends.
- Spotify Quota Extension Request (when ready to share publicly).

Reference implementations to consult when stuck:
- `/tmp/ncspot/src/spotify_worker.rs` — the player loop pattern.
- `/tmp/ncspot/src/library.rs` — pagination + parallel fetch shape.
- `/tmp/ncspot/src/authentication.rs` — OAuth scopes and token mapping.
