# spfy — Design

A small Rust TUI Spotify client. Inspired by ncspot, intentionally narrower:
five library views, a search box, a now-playing strip, play/pause/next/prev.

## Goals

1. Browse your Spotify library: Liked Songs, Saved Albums, Playlists, Followed
   Artists, Recently Played.
2. Drill into an album or playlist to see its tracks.
3. Search for a track and play it.
4. Play / pause / next / previous, with audio streamed by the binary itself.

Premium account required (Spotify enforces this for any programmatic playback,
both via librespot and via the Web API). macOS only for v1; single audio
backend (`rodio` → CoreAudio via cpal).

## Non-goals (v1)

Repeat / shuffle, lyrics, editable queue, library write operations
(save/unsave), Spotify Connect device transfer, IPC / mpris / remote control,
podcasts, top tracks, genre browsing, Linux/Windows audio backends, on-disk
library cache.

## Approach

Workspace split — `spfy-core` (library crate: auth, api, player) plus `spfy`
(binary crate: ratatui TUI). The boundary rule: `core` does not depend on
`ratatui` or `crossterm`; the TUI imports only `core`'s public types.

Why a split rather than a single binary: the worker pattern naturally yields a
narrow interface (`Cmd` / `Event` channels), and codifying that interface as a
crate boundary makes adding a future headless mode (`spfy play <id>`) a
half-day refactor rather than a refactor of the whole codebase.

## Workspace layout

```
spotify-cli/
├── Cargo.toml                   workspace root
├── rust-toolchain.toml          stable
├── core/                        spfy-core (lib)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs               re-exports
│       ├── auth.rs              OAuth + credential cache
│       ├── api.rs               rspotify wrapper
│       ├── player.rs            librespot worker + Cmd/Event
│       ├── model.rs             plain structs the TUI consumes
│       └── error.rs             CoreError enum
└── tui/                         spfy (bin)
    ├── Cargo.toml
    └── src/
        ├── main.rs              tokio runtime, wires core + ui
        ├── app.rs               App state + reducer
        ├── event.rs             AppEvent + channel plumbing
        ├── ui/
        │   ├── mod.rs           render dispatch
        │   ├── library.rs
        │   ├── search.rs
        │   └── now_playing.rs
        └── keymap.rs            key → action table
```

### Crate dependencies

`core/Cargo.toml`:
- `librespot` (core, playback, oauth, protocol)
- `librespot-oauth` (the OAuth helper used directly for the Web API leg)
- `rspotify` (with `client-reqwest` feature)
- `tokio` (full)
- `rodio`
- `serde`, `serde_json`
- `directories` (XDG paths)
- `anyhow`, `thiserror`
- `tracing`
- `chrono`

`tui/Cargo.toml`:
- `spfy-core` (path dep)
- `ratatui`
- `crossterm`
- `tokio`
- `anyhow`
- `tracing`, `tracing-subscriber`

## Auth

Two OAuth flows, ncspot-style. One for librespot (streaming session), one for
rspotify (Web API). User clicks "Agree" twice on first run; subsequent runs
hit cache and never open the browser.

### Constants (in `core/src/auth.rs`)

```rust
pub const SPOTIFY_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
pub const SPFY_CLIENT_ID:    &str = "<YOUR_DEV_APP_ID>";
```

`SPOTIFY_CLIENT_ID` is Spotify's official desktop-app client_id, used by
librespot to authenticate as a Spotify Connect device. `SPFY_CLIENT_ID` is
the developer-registered "spfy" app, used for Web API calls.

One-time developer setup: register an app at `developer.spotify.com`, redirect
URI `http://127.0.0.1`, copy the client_id into the constant. Add yourself as
a test user (Spotify apps default to "Development Mode" with a 25-user cap;
public distribution would require a Quota Extension Request review).

### Scopes

```rust
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
```

### Cold-cache flow (first run)

1. **librespot leg.** Open `librespot::Cache::new("<config>/spfy/librespot")`.
   `cache.credentials()` returns `None`. Build
   `OAuthClientBuilder::new(SPOTIFY_CLIENT_ID, redirect, PLAYBACK_SCOPES)` and
   call `get_access_token()`. The builder picks a free localhost port, spins
   up an HTTP server at `/login`, opens the browser to Spotify's authorize
   URL, captures the redirect code, exchanges it via PKCE for tokens. We wrap
   the access token in `Credentials::with_access_token(...)`.
2. **rspotify leg.** Same `OAuthClientBuilder` flow with `SPFY_CLIENT_ID` and
   `API_SCOPES`. Result mapped into `rspotify::Token`.

Two browser pop-ups, two consent screens, because access tokens are scoped
per-client_id and can't be shared across clients.

### Warm-cache flow (every subsequent run)

1. `librespot::Cache::credentials()` returns `Some(...)`. No browser, no
   network.
2. Read `<config>/spfy/rspotify_token.json`. If unexpired → use it. If
   expired → `oauth_client.refresh_token(refresh_token)` silently POSTs to
   `/api/token`, gets new access token, persists. No browser.

### Mid-session refresh

`SpotifyApi::request_with_retry` catches a 401, refreshes the rspotify token,
retries once, and only then surfaces an error. The TUI never sees a 401.

### Storage layout (macOS)

Path resolved by `directories::ProjectDirs::from("io", "spfy", "spfy")`:

```
~/Library/Application Support/spfy/
├── librespot/
│   └── credentials.json     librespot's own session creds (it manages this)
├── rspotify_token.json      Web API tokens, mode 0600
└── spfy.log                 tracing output
```

No env vars or config file in v1.

### Public surface

```rust
// core/src/auth.rs
pub fn login() -> Result<Session>;

pub struct Session {
    pub player_credentials: librespot::core::authentication::Credentials,
    pub api: rspotify::AuthCodeSpotify,
}
```

## Core API surface

The TUI consumes `core::model::*` only. rspotify types stop at `core::api`.

### Plain types (`core/src/model.rs`)

```rust
pub struct TrackId(pub String);    // "spotify:track:..."
pub struct AlbumId(pub String);
pub struct PlaylistId(pub String);
pub struct ArtistId(pub String);

pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: u32,
}

pub struct Album {
    pub id: AlbumId,
    pub name: String,
    pub artists: Vec<String>,
    pub track_count: u32,
}

pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    pub owner: String,
    pub track_count: u32,
}

pub struct Artist { pub id: ArtistId, pub name: String, }

pub struct PlayHistoryEntry {
    pub track: Track,
    pub played_at: chrono::DateTime<chrono::Utc>,
}
```

### API (`core/src/api.rs`)

```rust
pub struct SpotifyApi { /* wraps rspotify::AuthCodeSpotify */ }

impl SpotifyApi {
    pub async fn liked_tracks(&self)        -> Result<Vec<Track>>;
    pub async fn saved_albums(&self)        -> Result<Vec<Album>>;
    pub async fn playlists(&self)           -> Result<Vec<Playlist>>;
    pub async fn followed_artists(&self)    -> Result<Vec<Artist>>;
    pub async fn recently_played(&self)     -> Result<Vec<PlayHistoryEntry>>;

    pub async fn album_tracks(&self, id: &AlbumId)       -> Result<Vec<Track>>;
    pub async fn playlist_tracks(&self, id: &PlaylistId) -> Result<Vec<Track>>;

    pub async fn search_tracks(&self, query: &str, limit: u32) -> Result<Vec<Track>>;
}
```

### Pagination strategy: eager

Each method loops `next.is_none()` and returns the full `Vec`. Same as
ncspot's `library.rs`. Personal libraries top out at a few thousand tracks;
streaming pagination isn't worth the complexity.

### Caching: none

ncspot persists library JSON to disk and version-checks it. That's a
non-trivial subsystem (versioning, partial refresh, invalidation). v1
refetches on startup. We add caching only if startup feels slow with a real
library.

### rspotify call mapping

| Our method | rspotify call |
|---|---|
| `liked_tracks` | `current_user_saved_tracks_manual` |
| `saved_albums` | `current_user_saved_albums_manual` |
| `playlists` | `current_user_playlists_manual` |
| `followed_artists` | `current_user_followed_artists` (cursor) |
| `recently_played` | `current_user_recently_played` |
| `album_tracks` | `album_track_manual` |
| `playlist_tracks` | `playlist_items_manual` |
| `search_tracks` | `search(.., SearchType::Track, ..)` |

## Player worker

Single tokio task. Owns `librespot::Player` + `librespot::Session`. Talks to
the TUI via two `mpsc::UnboundedChannel`s.

### Public surface (`core/src/player.rs`)

```rust
pub enum Cmd {
    Play(TrackId),
    PlayContext { uris: Vec<TrackId>, start: usize },
    Toggle, Next, Previous,
    Seek(u32),
    SetVolume(u8),
    Quit,
}

pub enum Event {
    Started   { track: TrackId, duration_ms: u32 },
    Resumed, Paused,
    Position(u32),
    EndOfTrack, Stopped,
    Error(String),
}

pub struct PlayerHandle { /* cmd_tx + event_rx */ }

pub fn spawn(creds: Credentials, rt: tokio::runtime::Handle) -> PlayerHandle;
```

### Internal loop

```rust
async fn run(creds, mut cmd_rx, event_tx) {
    let session = Session::new(SessionConfig::default(), Some(cache));
    session.connect(creds, true).await?;       // Premium check fails here

    let mixer  = SoftMixer::open(MixerConfig::default());
    let player = Player::new(
        PlayerConfig::default(),
        session.clone(),
        mixer.get_soft_volume(),
        || rodio_backend::open(None),
    );

    let mut player_events = player.get_player_event_channel();
    let mut queue: Vec<TrackId> = vec![];
    let mut idx: usize = 0;
    let mut anchor: Option<(Instant, u32)> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(500));

    loop {
        tokio::select! {
            Some(cmd) = cmd_rx.recv() => handle_cmd(cmd, ...),
            Some(ev)  = player_events.recv() => handle_librespot_event(ev, ...),
            _ = tick.tick() => emit_position_estimate(...),
        }
    }
}
```

### Behaviors

- **Position tracking**: anchored, not timer-driven. On
  `Started`/`Resumed`/`Seek`: `anchor = (now, track_pos_ms)`. On tick while
  playing: emit `Position(anchor.1 + (now - anchor.0).as_millis())`.
- **Queue**: in-worker. `PlayContext` sets `queue` and `idx`, loads
  `queue[start]`. `Next`/`Previous` walks `idx`. `EndOfTrack` auto-advances or
  emits `Stopped` at the end.
- **Premium**: `session.connect()` returns `BadCredentials` for free
  accounts. Worker emits `Event::Error("Premium account required")` and
  exits; TUI promotes to a fatal error screen.
- **Audio backend**: `rodio` only. `MixerConfig` `SoftMixer` for in-app
  digital volume (0..=65535 internally; we expose 0..=100 and scale).

### Why a tokio task, not an OS thread

librespot is async-first. A dedicated thread would have to build a tokio
runtime inside it, and the binary already owns one for the TUI's event loop
and API calls — two runtimes communicating across an OS-thread boundary buys
nothing. Worker is `tokio::spawn(run(...))` on the binary's runtime.

## TUI architecture

### Threads / tasks

```
Main OS thread (tokio runtime owner):
  - ratatui render loop
  - App reducer

std::thread (one):
  - crossterm::event::read() — blocking, can't run inside tokio

tokio tasks (many):
  - player worker
  - tick (500ms)
  - one per outbound API call (library section, drill-down, search)
  - player-event forwarder (drains PlayerHandle events into AppEvent channel)
```

The crossterm reader is the *only* extra OS thread. Everything else is a
tokio task on the main runtime.

### App state

```rust
pub struct App {
    now_playing: Option<Track>,
    is_playing: bool,
    position_ms: u32,
    volume: u8,

    liked:     SectionState<Vec<Track>>,
    albums:    SectionState<Vec<Album>>,
    playlists: SectionState<Vec<Playlist>>,
    artists:   SectionState<Vec<Artist>>,
    recent:    SectionState<Vec<PlayHistoryEntry>>,

    mode: Mode,
    toast: Option<(Instant, String)>,
    should_quit: bool,
}

pub enum Mode {
    Library { tab: LibTab, list: ListState },
    Detail  { title: String, tracks: Vec<Track>, list: ListState, back: Box<Mode> },
    Search  { input: String, results: SectionState<Vec<Track>>, list: ListState },
}

pub enum LibTab { Liked, Albums, Playlists, Artists, Recent }

pub enum SectionState<T> { Idle, Loading, Loaded(T), Failed(String) }
```

### Event channel

```rust
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    Player(core::player::Event),
    Loaded(SectionId, SectionPayload),
    Failed(SectionId, String),
    DetailLoaded(String, Vec<Track>),
    SearchResult(Vec<Track>),
    SearchFailed(String),
}
```

### Main loop

```rust
loop {
    terminal.draw(|f| ui::render(f, &app))?;
    let Some(ev) = rx.recv().await else { break };
    app.update(ev, &ctx);
    if app.should_quit { break; }
}
```

Re-render on every event.

### Keymap

| Key | Action |
|---|---|
| `j`/`k` (or `↓`/`↑`) | cursor |
| `Tab` / `Shift+Tab` | next / prev library tab |
| `Enter` | play track, or drill into album/playlist |
| `Esc` | back |
| `p` / `Space` | toggle play/pause |
| `n` / `b` | next / previous track |
| `/` | enter search mode |
| `+` / `-` | volume |
| `q` | quit |

### Initial load

Five `tokio::spawn` calls dispatched in parallel after auth completes — one
per library section. Each section renders "Loading…" until its event
arrives. A failure in one section shows an inline error; other sections still
work.

### Drill-down

Enter on an album → spawn `api.album_tracks(id)` → toast "Loading…" → on
`DetailLoaded`, transition to `Mode::Detail` with `back = current Library
mode`. Enter on a track in Detail → `Cmd::PlayContext { uris: tracks_in_view,
start: idx }` so the rest auto-advances.

### Search

`/` → `Mode::Search { input: "" }`. Typing edits `input`; Enter spawns
`api.search_tracks(input, 50)`; results stream in; Enter on a result issues
`Cmd::Play`. Esc returns to previous mode.

## Errors, logging, shutdown

### Error types

- `core::error::CoreError` (thiserror) variants: `Http`, `Api`, `NotPremium`,
  `Auth`, `Player`.
- `tui` uses `anyhow::Result` at the top.

### Surfacing

- Recoverable (search failed, one section failed) → toast in now-playing
  area, auto-clears after 5s.
- Fatal (auth failed, Premium required, terminal init failed) → full-screen
  error mode, `q` to quit.
- Player `Event::Error` is classified: `"Premium required"` → fatal,
  everything else → toast.

### Terminal-state safety

```rust
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen, Show);
        let _ = crossterm::terminal::disable_raw_mode();
        original(info);
    }));
}
```

Mirror cleanup on every normal exit. A panic that leaves the terminal in raw
mode would be the worst first-run UX bug we could ship.

### Logging

`tracing` + `tracing-subscriber::fmt` writing to a file at
`<config>/spfy/spfy.log`. Never to stdout (ratatui owns it). Truncate on each
run for v1. Default level `info`; `RUST_LOG=spfy=debug,librespot=warn` for
dev.

### Shutdown sequence

1. `q` → `app.should_quit = true` → main loop breaks.
2. `Cmd::Quit` to player → worker stops player, shuts down session.
3. Disable raw mode, leave alternate screen, show cursor.
4. Drop tokio runtime (waits up to 2s for tasks).
5. Process exits.

## Testing

| Layer | Approach | Worth it |
|---|---|---|
| `core::api` conversions | unit tests with JSON fixtures | yes |
| `core::api` pagination | mocked paginated response | yes |
| `core::auth` token persistence | round-trip on tempdir | yes |
| `core::player` queue logic | extract index/queue mutation as pure function | yes |
| `core::player` against real librespot | — | no (flaky, network, Premium creds) |
| `tui::app::update` reducer | unit tests, no terminal | yes — bug magnet |
| TUI snapshot rendering | `insta` + ratatui `TestBackend` | optional v2 |
| End-to-end | manual smoke script in README | yes |

## Distribution caveat (forward-looking)

Spotify Developer apps default to "Development Mode" — only manually-added
test users can authorize them, capped at 25. For personal use this is
invisible; for public distribution we'd need to submit a Quota Extension
Request to Spotify (free, 1-2 week review). Out of scope for v1; mentioned
here so the constraint is documented.
