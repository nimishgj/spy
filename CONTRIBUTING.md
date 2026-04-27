# Contributing to spfy

Bug reports, feature requests, and patches all welcome. A few notes:

## Building

```bash
cargo build --workspace
cargo test --workspace
```

For development, prefer `cargo run -p spfy` (debug, fast link). Use
`--release` only for final smoke tests.

## Project layout

- `core/` — library crate (`spfy-core`). All Spotify integration lives
  here. Public model types do NOT depend on rspotify or librespot.
- `tui/` — binary crate (`spfy`). ratatui TUI and the daemon-spawning
  logic. Talks to the daemon over a Unix domain socket.
- `vendored/rspotify-model/` — local fork of rspotify-model with two
  `#[serde(default)]` patches. Don't modify unless you also document why
  in the README.
- `docs/plans/` — design doc and step-by-step implementation plan.

## Style

- Single-line commit subjects. Conventional-commits style preferred:
  `feat(core): ...`, `fix(tui): ...`, `docs: ...`, `chore: ...`.
- `cargo fmt && cargo clippy --workspace` before sending a PR.
- Don't add features beyond what the issue/PR describes (YAGNI).
- Keep the `core` boundary clean: no rspotify/librespot types in
  `core::model`'s public surface.

## Testing

Most unit tests live as integration tests in `core/tests/` and
`tui/tests/`. End-to-end (real Spotify Premium account) testing is
manual — see the smoke-test checklist in `README.md`.
