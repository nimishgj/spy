# spfy

A small Rust TUI Spotify client. Browse your library, search, play music.
Inspired by ncspot.

## Requirements

- macOS (Linux/Windows untested in v1)
- Spotify **Premium** account
- A Spotify Developer app (one-time setup)

## Setup

1. Visit https://developer.spotify.com/dashboard.
2. Click **Create app**. Name `spfy`, redirect URI `http://127.0.0.1:8888/login`,
   API: Web API.
3. Add your Spotify account email to the app's user list (apps default to
   "Development Mode" with a 25-user cap).
4. Open `core/src/auth.rs` and replace the placeholder in `SPFY_CLIENT_ID` with
   the client_id shown on the dashboard.

## Build & run

    cargo run --release -p spfy

First run opens two browser pop-ups for OAuth (one for streaming via
librespot, one for the Web API). Tokens cache under
`~/Library/Application Support/spfy/`. Subsequent runs are silent.

## Smoke test

1. Launch — TUI shows tabs (Liked / Albums / Playlists / Artists / Recent).
2. `j`/`k` (or arrow keys) navigate.
3. `Tab` / `Shift+Tab` cycles tabs.
4. `Enter` on a Liked track — audio plays.
5. `p` pauses, `p` again resumes.
6. `n` / `b` skip / previous.
7. `+` / `-` change volume.
8. `/` enters search; type a query, `Enter`; `↑`/`↓` and `Enter` to play.
9. `Esc` exits search or Detail view.
10. `q` quits cleanly (terminal restored).

## Logs

`~/Library/Application Support/spfy/spfy.log`. Override level with
`RUST_LOG`, e.g.:

    RUST_LOG=spfy=debug,librespot=warn cargo run -p spfy

## Architecture

Cargo workspace, two crates:

- `core/` (`spfy-core`) — auth, Spotify Web API wrapper, librespot player worker.
- `tui/` (`spfy`) — ratatui TUI.

The TUI imports `spfy-core`'s public model types only; `rspotify` and
`librespot` types stop at the `core` boundary. The player worker runs as a
tokio task on the binary's runtime; one extra `std::thread` reads blocking
crossterm key events.

Design and implementation plan: `docs/plans/2026-04-26-spfy-design.md` and
`docs/plans/2026-04-26-spfy-implementation.md`.

## Status

v1: read-only library, search, playback. Out of scope for v1: repeat /
shuffle, lyrics, editable queue, library write operations, Spotify Connect
device transfer, IPC / mpris, podcasts, Linux/Windows audio backends.
