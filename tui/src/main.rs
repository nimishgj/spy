use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use spfy::app::App;
use spfy::event::{self, spawn_key_thread, spawn_tick};

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

    let session = spfy_core::auth::login()?;
    let api = std::sync::Arc::new(spfy_core::api::SpotifyApi::new(session.api));

    let (tx, mut rx) = event::channel();
    spawn_key_thread(tx.clone());
    spawn_tick(tx.clone());

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
                    use spfy::app::UiAction;
                    use spfy::event::AppEvent;
                    match action {
                        UiAction::LoadAlbumTracks(id) => {
                            let api2 = api.clone();
                            let tx2 = tx.clone();
                            tokio::spawn(async move {
                                let _ = match api2.album_tracks(&id).await {
                                    Ok(tracks) => tx2.send(AppEvent::DetailLoaded {
                                        title: "Album".to_string(),
                                        tracks,
                                    }),
                                    Err(e) => tx2.send(AppEvent::DetailFailed(e.to_string())),
                                };
                            });
                        }
                        UiAction::LoadPlaylistTracks(id) => {
                            let api2 = api.clone();
                            let tx2 = tx.clone();
                            tokio::spawn(async move {
                                let _ = match api2.playlist_tracks(&id).await {
                                    Ok(tracks) => tx2.send(AppEvent::DetailLoaded {
                                        title: "Playlist".to_string(),
                                        tracks,
                                    }),
                                    Err(e) => tx2.send(AppEvent::DetailFailed(e.to_string())),
                                };
                            });
                        }
                        // Other UiAction variants will be wired in Task 23/24.
                        _ => {}
                    }
                }
            }
            None => break,
        }
    }

    leave(term)?;
    Ok(())
}
