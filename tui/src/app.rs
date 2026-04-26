use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use spfy_core::model::{Album, AlbumId, Artist, PlayHistoryEntry, Playlist, PlaylistId, Track, TrackId};

use crate::event::AppEvent;

pub enum UiAction {
    LoadAlbumTracks(AlbumId),
    LoadPlaylistTracks(PlaylistId),
    Play(TrackId),
    PlayContext { uris: Vec<TrackId>, start: usize },
    Toggle,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    Search(String),
}

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

    pub pending: Vec<UiAction>,
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
            pending: Vec::new(),
        }
    }

    pub fn update(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(k) if k.kind == KeyEventKind::Press => self.handle_key(k),
            AppEvent::Tick => self.clear_stale_toast(),
            AppEvent::LibraryLoaded(section) => match section {
                crate::event::LibrarySection::Liked(v) => self.liked = SectionState::Loaded(v),
                crate::event::LibrarySection::Albums(v) => self.albums = SectionState::Loaded(v),
                crate::event::LibrarySection::Playlists(v) => self.playlists = SectionState::Loaded(v),
                crate::event::LibrarySection::Artists(v) => self.artists = SectionState::Loaded(v),
                crate::event::LibrarySection::Recent(v) => self.recent = SectionState::Loaded(v),
            },
            AppEvent::LibraryFailed(id, msg) => {
                use crate::event::SectionId;
                match id {
                    SectionId::Liked => self.liked = SectionState::Failed(msg),
                    SectionId::Albums => self.albums = SectionState::Failed(msg),
                    SectionId::Playlists => self.playlists = SectionState::Failed(msg),
                    SectionId::Artists => self.artists = SectionState::Failed(msg),
                    SectionId::Recent => self.recent = SectionState::Failed(msg),
                }
            }
            AppEvent::DetailLoaded { title, tracks } => {
                let prev = std::mem::replace(
                    &mut self.mode,
                    Mode::Library { tab: LibTab::Liked, list: ListState::default() },
                );
                self.mode = Mode::Detail {
                    title,
                    tracks,
                    list: ListState::default(),
                    back: Box::new(prev),
                };
            }
            AppEvent::DetailFailed(msg) => {
                self.toast = Some((Instant::now(), format!("Failed: {msg}")));
            }
            _ => {}
        }
    }

    fn handle_key(&mut self, k: crossterm::event::KeyEvent) {
        // Esc in Detail mode: go back to previous mode.
        if matches!(k.code, KeyCode::Esc) {
            if let Mode::Detail { back, .. } = &mut self.mode {
                let back_mode = std::mem::replace(
                    back.as_mut(),
                    Mode::Library { tab: LibTab::Liked, list: ListState::default() },
                );
                self.mode = back_mode;
                return;
            }
        }

        // Quit on q/Esc in Library mode (Search has its own handling later).
        if matches!(self.mode, Mode::Library { .. })
            && (matches!(k.code, KeyCode::Char('q')) || matches!(k.code, KeyCode::Esc))
        {
            self.should_quit = true;
            return;
        }

        // q in Detail also quits (Esc already handled above).
        if matches!(self.mode, Mode::Detail { .. }) && matches!(k.code, KeyCode::Char('q')) {
            self.should_quit = true;
            return;
        }

        // Phase 1: snapshot Library state for borrow-free decisions.
        let lib_state = if let Mode::Library { tab, list } = &self.mode {
            Some((*tab, list.selected(), self.section_len(*tab)))
        } else {
            None
        };

        // Phase 2a: handle Enter in Library mode (pushes a UiAction; no
        // mutable borrow of self.mode held while pushing to self.pending).
        if let Some((tab, sel, _len)) = lib_state {
            if matches!(k.code, KeyCode::Enter) {
                match (tab, sel) {
                    (LibTab::Albums, Some(idx)) => {
                        if let SectionState::Loaded(v) = &self.albums {
                            if let Some(a) = v.get(idx) {
                                self.pending.push(UiAction::LoadAlbumTracks(a.id.clone()));
                            }
                        }
                    }
                    (LibTab::Playlists, Some(idx)) => {
                        if let SectionState::Loaded(v) = &self.playlists {
                            if let Some(p) = v.get(idx) {
                                self.pending.push(UiAction::LoadPlaylistTracks(p.id.clone()));
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }
        }

        // Phase 2b: tab/cursor mutation in Library mode.
        if let Mode::Library { tab, list } = &mut self.mode {
            let lib_len = lib_state.map(|(_, _, l)| l);
            match (k.code, k.modifiers) {
                (KeyCode::Tab, m) if !m.contains(KeyModifiers::SHIFT) => *tab = tab.next(),
                (KeyCode::BackTab, _) => *tab = tab.previous(),
                (KeyCode::Tab, m) if m.contains(KeyModifiers::SHIFT) => *tab = tab.previous(),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if let Some(len) = lib_len { move_cursor(list, len, 1); }
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if let Some(len) = lib_len { move_cursor(list, len, -1); }
                }
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

    fn section_len(&self, tab: LibTab) -> usize {
        match tab {
            LibTab::Liked => loaded_len(&self.liked),
            LibTab::Albums => loaded_len(&self.albums),
            LibTab::Playlists => loaded_len(&self.playlists),
            LibTab::Artists => loaded_len(&self.artists),
            LibTab::Recent => loaded_len(&self.recent),
        }
    }
}

fn loaded_len<T>(s: &SectionState<Vec<T>>) -> usize {
    match s {
        SectionState::Loaded(v) => v.len(),
        _ => 0,
    }
}

fn move_cursor(list: &mut ListState, len: usize, delta: i32) {
    if len == 0 {
        list.select(None);
        return;
    }
    let cur = list.selected().unwrap_or(0) as i32;
    let next = (cur + delta).rem_euclid(len as i32) as usize;
    list.select(Some(next));
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
