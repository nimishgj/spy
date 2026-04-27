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
    assert!(matches!(
        app.mode,
        Mode::Library {
            tab: LibTab::Liked,
            ..
        }
    ));

    app.update(AppEvent::Key(KeyEvent::new_with_kind(
        KeyCode::Tab,
        KeyModifiers::NONE,
        KeyEventKind::Press,
    )));
    assert!(matches!(
        app.mode,
        Mode::Library {
            tab: LibTab::Albums,
            ..
        }
    ));
}

#[test]
fn j_moves_cursor_down_when_section_loaded() {
    use spfy_core::model::{Track, TrackId};
    let mut app = App::new();
    app.liked = spfy::app::SectionState::Loaded(vec![
        Track {
            id: TrackId("a".into()),
            name: "A".into(),
            artists: vec![],
            album: "".into(),
            duration_ms: 0,
        },
        Track {
            id: TrackId("b".into()),
            name: "B".into(),
            artists: vec![],
            album: "".into(),
            duration_ms: 0,
        },
    ]);
    app.update(key('j'));
    if let Mode::Library { list, .. } = &app.mode {
        assert_eq!(list.selected(), Some(1));
    } else {
        panic!()
    }
}
