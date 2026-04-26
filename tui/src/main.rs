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

    let (tx, mut rx) = event::channel();
    spawn_key_thread(tx.clone());
    spawn_tick(tx.clone());

    let mut app = App::new();

    loop {
        term.draw(|f| spfy::ui::render(f, &mut app))?;

        match rx.recv().await {
            Some(ev) => {
                app.update(ev);
                if app.should_quit {
                    break;
                }
            }
            None => break,
        }
    }

    leave(term)?;
    Ok(())
}
