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
