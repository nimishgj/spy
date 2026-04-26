use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use spfy::app::{App, LibTab, Mode};
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

#[test]
fn tab_cycles_through_library_tabs() {
    let mut app = App::new();
    assert!(matches!(app.mode, Mode::Library { tab: LibTab::Liked, .. }));

    app.update(AppEvent::Key(KeyEvent::new_with_kind(
        KeyCode::Tab, KeyModifiers::NONE, KeyEventKind::Press,
    )));
    assert!(matches!(app.mode, Mode::Library { tab: LibTab::Albums, .. }));
}
