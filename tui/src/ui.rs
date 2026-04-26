use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, LibTab, Mode, SectionState};

pub fn render(f: &mut Frame, app: &mut App) {
    if let Some(msg) = app.fatal.clone() {
        let area = f.area();
        let block = Block::default().borders(Borders::ALL).title("Fatal");
        let para = Paragraph::new(format!("{msg}\n\nPress q to quit.")).block(block);
        f.render_widget(para, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_tabs(f, chunks[0], app);
    render_body(f, chunks[1], app);
    render_now_playing(f, chunks[2], app);
    render_help(f, chunks[3]);
}

fn render_tabs(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let titles = ["Liked", "Albums", "Playlists", "Artists", "Recent"];
    let selected = match &app.mode {
        Mode::Library { tab, .. } => match tab {
            LibTab::Liked => 0,
            LibTab::Albums => 1,
            LibTab::Playlists => 2,
            LibTab::Artists => 3,
            LibTab::Recent => 4,
        },
        _ => 0,
    };
    let tabs = Tabs::new(titles.iter().map(|t| Line::from(*t)).collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL))
        .select(selected)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED));
    f.render_widget(tabs, area);
}

fn render_body(f: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    match &mut app.mode {
        Mode::Library { tab, list } => {
            let items: Vec<ListItem> = match tab {
                LibTab::Liked => section_items(&app.liked, |t| {
                    format!("{} — {}", t.name, t.artists.join(", "))
                }),
                LibTab::Albums => section_items(&app.albums, |a| {
                    format!("{} — {}", a.name, a.artists.join(", "))
                }),
                LibTab::Playlists => section_items(&app.playlists, |p| {
                    format!("{} ({} tracks)", p.name, p.track_count)
                }),
                LibTab::Artists => section_items(&app.artists, |a| a.name.clone()),
                LibTab::Recent => section_items(&app.recent, |e| {
                    format!("{} — {}", e.track.name, e.track.artists.join(", "))
                }),
            };
            let list_widget = List::new(items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            f.render_stateful_widget(list_widget, area, list);
        }
        Mode::Detail { title, tracks, list, .. } => {
            let items: Vec<ListItem> = tracks
                .iter()
                .map(|t| ListItem::new(format!("{} — {}", t.name, t.artists.join(", "))))
                .collect();
            let list_widget = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(title.clone()))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            f.render_stateful_widget(list_widget, area, list);
        }
        Mode::Search { input, results, list, .. } => {
            let inner = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(area);
            let prompt = Paragraph::new(format!("/ {input}"))
                .block(Block::default().borders(Borders::ALL).title("Search"));
            f.render_widget(prompt, inner[0]);
            let items: Vec<ListItem> = match results {
                SectionState::Loaded(tracks) => tracks
                    .iter()
                    .map(|t| ListItem::new(format!("{} — {}", t.name, t.artists.join(", "))))
                    .collect(),
                SectionState::Loading => vec![ListItem::new("Loading…")],
                SectionState::Failed(e) => vec![ListItem::new(format!("Error: {e}"))],
                SectionState::Idle => vec![ListItem::new("Type a query and press Enter")],
            };
            let list_widget = List::new(items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            f.render_stateful_widget(list_widget, inner[1], list);
        }
    }
}

fn section_items<T>(state: &SectionState<Vec<T>>, fmt: impl Fn(&T) -> String) -> Vec<ListItem<'static>> {
    match state {
        SectionState::Idle => vec![ListItem::new("Idle")],
        SectionState::Loading => vec![ListItem::new("Loading…")],
        SectionState::Failed(e) => vec![ListItem::new(format!("Error: {e}"))],
        SectionState::Loaded(items) if items.is_empty() => vec![ListItem::new("(empty)")],
        SectionState::Loaded(items) => items.iter().map(|x| ListItem::new(fmt(x))).collect(),
    }
}

fn render_now_playing(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let main_text = match &app.now_playing {
        Some(t) => {
            let icon = if app.is_playing { "▶" } else { "⏸" };
            let pos = format_ms(app.position_ms);
            format!("{icon} {} — {}   {pos}", t.name, t.artists.join(", "))
        }
        None => "(nothing playing)".to_string(),
    };
    let body = if let Some((_, ref msg)) = app.toast {
        format!("{main_text}\n{msg}")
    } else {
        main_text
    };
    let para = Paragraph::new(body).block(Block::default().borders(Borders::ALL));
    f.render_widget(para, area);
}

fn render_help(f: &mut Frame, area: ratatui::layout::Rect) {
    let p = Paragraph::new("j/k navigate · Tab next pane · ⏎ play · p pause · / search · q quit");
    f.render_widget(p, area);
}

fn format_ms(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}
