use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use spfy_core::model::{Album, Artist, PlayHistoryEntry, Playlist, Track};

use crate::event::AppEvent;

pub enum SectionState<T> {
    Idle,
    Loading,
    Loaded(T),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibTab {
    Liked,
    Albums,
    Playlists,
    Artists,
    Recent,
}

impl LibTab {
    pub fn next(self) -> Self {
        match self {
            Self::Liked => Self::Albums,
            Self::Albums => Self::Playlists,
            Self::Playlists => Self::Artists,
            Self::Artists => Self::Recent,
            Self::Recent => Self::Liked,
        }
    }
    pub fn previous(self) -> Self {
        match self {
            Self::Liked => Self::Recent,
            Self::Albums => Self::Liked,
            Self::Playlists => Self::Albums,
            Self::Artists => Self::Playlists,
            Self::Recent => Self::Artists,
        }
    }
}

pub enum Mode {
    Library { tab: LibTab, list: ListState },
    Detail  { title: String, tracks: Vec<Track>, list: ListState, back: Box<Mode> },
    Search  { input: String, results: SectionState<Vec<Track>>, list: ListState },
}

pub struct App {
    pub now_playing: Option<Track>,
    pub is_playing: bool,
    pub position_ms: u32,
    pub volume: u8,

    pub liked:     SectionState<Vec<Track>>,
    pub albums:    SectionState<Vec<Album>>,
    pub playlists: SectionState<Vec<Playlist>>,
    pub artists:   SectionState<Vec<Artist>>,
    pub recent:    SectionState<Vec<PlayHistoryEntry>>,

    pub mode: Mode,
    pub toast: Option<(Instant, String)>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            now_playing: None,
            is_playing: false,
            position_ms: 0,
            volume: 70,
            liked: SectionState::Idle,
            albums: SectionState::Idle,
            playlists: SectionState::Idle,
            artists: SectionState::Idle,
            recent: SectionState::Idle,
            mode: Mode::Library { tab: LibTab::Liked, list: ListState::default() },
            toast: None,
            should_quit: false,
        }
    }

    pub fn update(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(k) if k.kind == KeyEventKind::Press => self.handle_key(k),
            AppEvent::Tick => self.clear_stale_toast(),
            _ => {}
        }
    }

    fn handle_key(&mut self, k: crossterm::event::KeyEvent) {
        // Quit (any non-Search mode)
        if !matches!(self.mode, Mode::Search { .. })
            && (matches!(k.code, KeyCode::Char('q')) || matches!(k.code, KeyCode::Esc))
        {
            self.should_quit = true;
            return;
        }

        // Tab navigation in Library mode
        if let Mode::Library { tab, .. } = &mut self.mode {
            match (k.code, k.modifiers) {
                (KeyCode::Tab, m) if !m.contains(KeyModifiers::SHIFT) => *tab = tab.next(),
                (KeyCode::BackTab, _) => *tab = tab.previous(),
                (KeyCode::Tab, m) if m.contains(KeyModifiers::SHIFT) => *tab = tab.previous(),
                _ => {}
            }
        }
    }

    fn clear_stale_toast(&mut self) {
        if let Some((at, _)) = self.toast {
            if at.elapsed() > Duration::from_secs(5) {
                self.toast = None;
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
