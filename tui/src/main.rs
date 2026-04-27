use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::net::UnixStream;

use spfy::app::App;
use spfy::event::{self, spawn_key_thread, spawn_tick};
use spfy::remote::{RemoteApi, RemotePlayer, split};
use spfy_core::ipc::{DaemonMsg, Envelope, FrontendMsg, read_envelope, write_envelope};
use tokio::io::BufReader;

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

fn init_logging() -> Result<()> {
    use tracing_subscriber::EnvFilter;

    let log_path = spfy_core::paths::log_path()?;
    let log_file = std::fs::File::create(&log_path)?;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,librespot=warn".into()),
        )
        .with_writer(log_file)
        .with_ansi(false)
        .init();
    Ok(())
}

/// Make sure we have cached credentials (both librespot and rspotify). If
/// not, run the OAuth flow in the foreground so the browser opens visibly,
/// before the daemon spawns into the background with stdio piped to /dev/null.
fn ensure_credentials() -> Result<()> {
    let creds_path = spfy_core::paths::librespot_cache_dir()?.join("credentials.json");
    let token_path = spfy_core::paths::rspotify_token_path()?;
    if !creds_path.exists() || !token_path.exists() {
        spfy_core::auth::login()?;
    }
    Ok(())
}

/// Connect to a running daemon, or spawn `spfy --daemon` and retry until it
/// accepts connections (or 30s have passed).
async fn ensure_daemon_running() -> Result<UnixStream> {
    let socket_path = spfy_core::paths::config_dir()?.join("daemon.sock");
    if let Ok(s) = UnixStream::connect(&socket_path).await {
        return Ok(s);
    }

    let exe = std::env::current_exe()?;
    std::process::Command::new(&exe)
        .arg("--daemon")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()?;

    for _ in 0..300 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(s) = UnixStream::connect(&socket_path).await {
            return Ok(s);
        }
    }
    anyhow::bail!("daemon failed to start within 30s")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("--daemon") => {
            init_logging()?;
            tracing::info!("spfy daemon starting");
            spfy_core::daemon::run().await?;
            tracing::info!("spfy daemon exited");
            Ok(())
        }
        Some("--stop") => stop_daemon().await,
        _ => {
            init_logging()?;
            tracing::info!("spfy starting");
            install_panic_hook();
            // Run OAuth flow in the foreground (visible browser prompts)
            // before we enter the alternate screen and before spawning a
            // detached daemon whose stdio is wired to /dev/null.
            ensure_credentials()?;
            let term = enter()?;
            let result = run_tui(term).await;
            if let Err(e) = &result {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                tracing::error!("tui exited with error: {e}");
            }
            result
        }
    }
}

async fn stop_daemon() -> Result<()> {
    let socket_path = spfy_core::paths::config_dir()?.join("daemon.sock");
    let stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(_) => {
            println!("spfy daemon is not running");
            return Ok(());
        }
    };
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let env = Envelope {
        id: 1,
        msg: FrontendMsg::Shutdown,
    };
    write_envelope(&mut write_half, &env).await?;

    // Wait for ShutdownAck (or socket close).
    let deadline = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => {
                anyhow::bail!("timed out waiting for daemon shutdown ack");
            }
            frame = read_envelope::<DaemonMsg>(&mut reader) => {
                match frame {
                    Ok(Some(env)) => {
                        if env.id == 1 && matches!(env.msg, DaemonMsg::ShutdownAck) {
                            println!("spfy daemon stopped");
                            return Ok(());
                        }
                        // Any other message (player events) — keep waiting.
                    }
                    Ok(None) => {
                        // Socket closed: assume daemon exited.
                        println!("spfy daemon stopped");
                        return Ok(());
                    }
                    Err(e) => anyhow::bail!("read error during shutdown: {e}"),
                }
            }
        }
    }
}

async fn run_tui(mut term: Tui) -> Result<()> {
    let stream = ensure_daemon_running().await?;
    let (api, mut player) = split(stream);
    let api = Arc::new(api);

    let (tx, mut rx) = event::channel();
    spawn_key_thread(tx.clone());
    spawn_tick(tx.clone());
    spfy::event::spawn_player_forwarder(tx.clone(), player.take_events());

    let mut app = App::new();

    app.liked = spfy::app::SectionState::Loading;
    app.albums = spfy::app::SectionState::Loading;
    app.playlists = spfy::app::SectionState::Loading;
    app.artists = spfy::app::SectionState::Loading;
    app.recent = spfy::app::SectionState::Loading;

    {
        use spfy::event::{AppEvent, LibrarySection, SectionId};
        let api2 = api.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let _ = match api2.liked_tracks().await {
                Ok(v) => tx2.send(AppEvent::LibraryLoaded(LibrarySection::Liked(v))),
                Err(e) => tx2.send(AppEvent::LibraryFailed(SectionId::Liked, e.to_string())),
            };
        });
        let api2 = api.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let _ = match api2.saved_albums().await {
                Ok(v) => tx2.send(AppEvent::LibraryLoaded(LibrarySection::Albums(v))),
                Err(e) => tx2.send(AppEvent::LibraryFailed(SectionId::Albums, e.to_string())),
            };
        });
        let api2 = api.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let _ = match api2.playlists().await {
                Ok(v) => tx2.send(AppEvent::LibraryLoaded(LibrarySection::Playlists(v))),
                Err(e) => tx2.send(AppEvent::LibraryFailed(SectionId::Playlists, e.to_string())),
            };
        });
        let api2 = api.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let _ = match api2.followed_artists().await {
                Ok(v) => tx2.send(AppEvent::LibraryLoaded(LibrarySection::Artists(v))),
                Err(e) => tx2.send(AppEvent::LibraryFailed(SectionId::Artists, e.to_string())),
            };
        });
        let api2 = api.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let _ = match api2.recently_played().await {
                Ok(v) => tx2.send(AppEvent::LibraryLoaded(LibrarySection::Recent(v))),
                Err(e) => tx2.send(AppEvent::LibraryFailed(SectionId::Recent, e.to_string())),
            };
        });
    }

    loop {
        term.draw(|f| spfy::ui::render(f, &mut app))?;

        match rx.recv().await {
            Some(ev) => {
                app.update(ev);
                if app.should_quit {
                    break;
                }
                let actions: Vec<_> = std::mem::take(&mut app.pending);
                for action in actions {
                    dispatch_action(&api, &player, &tx, &mut app, action);
                }
            }
            None => break,
        }
    }

    // NOTE: do NOT send Cmd::Quit here — the daemon must keep running so
    // music continues after the TUI closes. `spfy --stop` is the only
    // command that quits the daemon.
    leave(term)?;
    Ok(())
}

fn dispatch_action(
    api: &Arc<RemoteApi>,
    player: &RemotePlayer,
    tx: &tokio::sync::mpsc::UnboundedSender<spfy::event::AppEvent>,
    app: &mut App,
    action: spfy::app::UiAction,
) {
    use spfy::app::UiAction;
    use spfy::event::AppEvent;
    match action {
        UiAction::LoadAlbumTracks { id, title } => {
            let api2 = api.clone();
            let tx2 = tx.clone();
            tokio::spawn(async move {
                let _ = match api2.album_tracks(&id).await {
                    Ok(tracks) => tx2.send(AppEvent::DetailLoaded { title, tracks }),
                    Err(e) => tx2.send(AppEvent::DetailFailed(e.to_string())),
                };
            });
        }
        UiAction::LoadPlaylistTracks { id, title } => {
            let api2 = api.clone();
            let tx2 = tx.clone();
            tokio::spawn(async move {
                let _ = match api2.playlist_tracks(&id).await {
                    Ok(tracks) => tx2.send(AppEvent::DetailLoaded { title, tracks }),
                    Err(e) => tx2.send(AppEvent::DetailFailed(e.to_string())),
                };
            });
        }
        UiAction::Play(id) => {
            player.send(spfy_core::player::Cmd::Play(id));
        }
        UiAction::PlayContext { uris, start } => {
            player.send(spfy_core::player::Cmd::PlayContext { uris, start });
        }
        UiAction::Toggle => {
            player.send(spfy_core::player::Cmd::Toggle);
        }
        UiAction::Next => {
            player.send(spfy_core::player::Cmd::Next);
        }
        UiAction::Previous => {
            player.send(spfy_core::player::Cmd::Previous);
        }
        UiAction::VolumeUp => {
            app.volume = (app.volume + 5).min(100);
            player.send(spfy_core::player::Cmd::SetVolume(app.volume));
        }
        UiAction::VolumeDown => {
            app.volume = app.volume.saturating_sub(5);
            player.send(spfy_core::player::Cmd::SetVolume(app.volume));
        }
        UiAction::Search(q) => {
            let api2 = api.clone();
            let tx2 = tx.clone();
            tokio::spawn(async move {
                let _ = match api2.search_tracks(&q, 10).await {
                    Ok(tracks) => tx2.send(AppEvent::SearchResult(tracks)),
                    Err(e) => tx2.send(AppEvent::SearchFailed(e.to_string())),
                };
            });
        }
    }
}
