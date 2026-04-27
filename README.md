# spfy

A small Rust TUI Spotify client. Browse your library, search tracks, and
play music — all from your terminal. Inspired by [ncspot], with a smaller
scope and a daemon mode that keeps playback alive after you close the UI.

[ncspot]: https://github.com/hrkfdn/ncspot

## Features

- Browse your library: Liked Songs, Saved Albums, Playlists, Followed
  Artists, Recently Played
- Drill into albums and playlists to play individual tracks
- Search Spotify's catalog and play results
- Standard playback controls: play, pause, next, previous, volume
- macOS media-key integration (F7/F8/F9, headphone in-line buttons,
  Now Playing widget)
- **Daemon mode**: close the TUI, music keeps playing. Reopen anytime
  to reattach.

## Requirements

- macOS (Linux/Windows untested in v1)
- Spotify **Premium** account (Spotify gates programmatic playback to
  Premium subscribers)
- Rust 1.85 or newer (edition 2024). Latest stable recommended.

## Setup

### 1. Register a Spotify Developer app

1. Visit https://developer.spotify.com/dashboard.
2. Click **Create app**. Name: `spfy` (or anything). Redirect URI:
   `http://127.0.0.1:8888/login`. Tick **Web API**.
3. Add your Spotify account email under **User Management** (apps default
   to "Development Mode" with a 25-user cap).
4. Copy the **Client ID** from the dashboard.

### 2. Tell spfy your client_id

Either set an environment variable (recommended):

```bash
export SPFY_CLIENT_ID="your-client-id-here"
```

Or add it to your shell rc (`~/.zshrc`, `~/.bashrc`, etc.) so it
persists.

(If `SPFY_CLIENT_ID` is unset, spfy falls back to the author's dev-app
ID, which is in Development Mode and only authorizes a small number of
users — running against your own client_id is the right choice.)

### 3. Build and run

```bash
git clone https://github.com/nimishgj/spotify-cli.git
cd spotify-cli
cargo run --release -p spfy
```

First run opens two browser tabs for OAuth (one for streaming via
librespot, one for the Web API). Click **Agree** on both. Tokens cache
under `~/Library/Application Support/spfy/`; subsequent runs are silent.

## Usage

| Key | Action |
|---|---|
| `j` / `k` (or `↓` / `↑`) | Cursor up/down |
| `h` / `l` (or `←` / `→`, `Tab`/`Shift+Tab`) | Switch library tab |
| `Enter` | Play track / drill into album/playlist |
| `Esc` | Back out of detail or search view |
| `p` / `Space` | Toggle play/pause |
| `n` / `b` | Next / previous track |
| `+` / `-` | Volume up/down |
| `/` | Enter search mode |
| `q` | Quit (daemon keeps playing) |

## Daemon mode

`spfy` automatically runs as a foreground TUI client connected to a
background daemon. The daemon owns the librespot session, the Web API
client, and a library cache. Closing the TUI does NOT stop playback —
you can reopen anytime and attach a fresh TUI to the same daemon.

```bash
spfy            # run TUI; spawns daemon if not already running
spfy --stop     # quit the daemon (stops music)
spfy --daemon   # run only the daemon (rare; usually auto-spawned)
```

## Architecture

Cargo workspace, two crates:

- `core/` (`spfy-core`) — library: auth (librespot OAuth + rspotify), Web
  API wrapper, librespot player worker, IPC protocol, daemon entrypoint.
- `tui/` (`spfy`) — binary: ratatui TUI, App reducer, daemon-spawning
  logic, RemoteApi/RemotePlayer facades over Unix-socket IPC.

Public TUI types only depend on `spfy_core::model` — rspotify and
librespot types stop at the core boundary. The frontend talks to the
daemon over a Unix-domain socket using line-delimited JSON envelopes.

Design and implementation plan:
- `docs/plans/2026-04-26-spfy-design.md`
- `docs/plans/2026-04-26-spfy-implementation.md`

## Logs

```bash
tail -f ~/Library/Application\ Support/spfy/spfy.log
```

Override level: `RUST_LOG=spfy=debug,librespot=warn cargo run -p spfy`.

## Vendored dependencies

`vendored/rspotify-model/` is a local fork of [rspotify-model] 0.16.1 with
two `#[serde(default)]` annotations added to fields Spotify sometimes
omits (`FullTrack.external_ids`, `FullAlbum.external_ids`). License
preserved from upstream.

[rspotify-model]: https://github.com/ramsayleung/rspotify

## Status

v1: read-only library, search, playback, daemon mode, media keys.

**Out of scope for v1**: repeat / shuffle, lyrics, editable queue,
library write operations, Spotify Connect device transfer, podcasts,
Linux/Windows audio backends, packaged .app bundle.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT. See [LICENSE](LICENSE).
