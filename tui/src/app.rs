use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use spfy_core::model::{
    Album, AlbumId, Artist, PlayHistoryEntry, Playlist, PlaylistId, Track, TrackId,
};

use crate::event::AppEvent;

pub enum UiAction {
    LoadAlbumTracks { id: AlbumId, title: String },
    LoadPlaylistTracks { id: PlaylistId, title: String },
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
    Library {
        tab: LibTab,
        list: ListState,
    },
    Detail {
        title: String,
        tracks: Vec<Track>,
        list: ListState,
        back: Box<Mode>,
    },
    Search {
        input: String,
        results: SectionState<Vec<Track>>,
        list: ListState,
        back: Box<Mode>,
    },
}

pub struct App {
    pub now_playing: Option<Track>,
    pub is_playing: bool,
    pub position_ms: u32,
    pub volume: u8,

    pub liked: SectionState<Vec<Track>>,
    pub albums: SectionState<Vec<Album>>,
    pub playlists: SectionState<Vec<Playlist>>,
    pub artists: SectionState<Vec<Artist>>,
    pub recent: SectionState<Vec<PlayHistoryEntry>>,

    pub mode: Mode,
    pub toast: Option<(Instant, String)>,
    pub fatal: Option<String>,
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
            mode: Mode::Library {
                tab: LibTab::Liked,
                list: ListState::default(),
            },
            toast: None,
            fatal: None,
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
                crate::event::LibrarySection::Playlists(v) => {
                    self.playlists = SectionState::Loaded(v)
                }
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
                    Mode::Library {
                        tab: LibTab::Liked,
                        list: ListState::default(),
                    },
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
            AppEvent::SearchResult(tracks) => {
                if let Mode::Search { results, list, .. } = &mut self.mode {
                    *results = SectionState::Loaded(tracks);
                    list.select(Some(0));
                }
            }
            AppEvent::SearchFailed(msg) => {
                if let Mode::Search { results, .. } = &mut self.mode {
                    *results = SectionState::Failed(msg);
                }
            }
            AppEvent::Player(ev) => {
                use spfy_core::player::Event as P;
                match ev {
                    P::Started {
                        track,
                        name,
                        artists,
                        album,
                        duration_ms,
                    } => {
                        self.now_playing = Some(Track {
                            id: track,
                            name,
                            artists,
                            album,
                            duration_ms,
                        });
                        self.is_playing = true;
                        self.position_ms = 0;
                    }
                    P::Resumed => self.is_playing = true,
                    P::Paused => self.is_playing = false,
                    P::Position(ms) => self.position_ms = ms,
                    P::EndOfTrack => {}
                    P::Stopped => {
                        self.is_playing = false;
                        self.now_playing = None;
                        self.position_ms = 0;
                    }
                    P::Error(msg) => {
                        if msg.contains("Premium") {
                            self.fatal = Some(msg);
                        } else {
                            self.toast = Some((Instant::now(), msg));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    fn find_track_by_id(&self, id: &TrackId) -> Option<Track> {
        let in_section = |s: &SectionState<Vec<Track>>| -> Option<Track> {
            if let SectionState::Loaded(v) = s {
                v.iter().find(|t| t.id == *id).cloned()
            } else {
                None
            }
        };
        if let Some(t) = in_section(&self.liked) {
            return Some(t);
        }
        if let SectionState::Loaded(v) = &self.recent
            && let Some(e) = v.iter().find(|e| e.track.id == *id)
        {
            return Some(e.track.clone());
        }
        if let Mode::Detail { tracks, .. } = &self.mode
            && let Some(t) = tracks.iter().find(|t| t.id == *id)
        {
            return Some(t.clone());
        }
        if let Mode::Search {
            results: SectionState::Loaded(tracks),
            ..
        } = &self.mode
            && let Some(t) = tracks.iter().find(|t| t.id == *id)
        {
            return Some(t.clone());
        }
        None
    }

    fn handle_key(&mut self, k: crossterm::event::KeyEvent) {
        // Esc in Detail mode: go back to previous mode.
        if matches!(k.code, KeyCode::Esc)
            && let Mode::Detail { back, .. } = &mut self.mode
        {
            let back_mode = std::mem::replace(
                back.as_mut(),
                Mode::Library {
                    tab: LibTab::Liked,
                    list: ListState::default(),
                },
            );
            self.mode = back_mode;
            return;
        }

        // Search mode handling: take all input characters; navigate with arrows.
        if matches!(self.mode, Mode::Search { .. }) {
            self.handle_key_search(k);
            return;
        }

        // Quit on q/Esc in Library mode.
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

        // `/` enters Search mode (with current mode preserved for back).
        if matches!(k.code, KeyCode::Char('/')) {
            let prev = std::mem::replace(
                &mut self.mode,
                Mode::Library {
                    tab: LibTab::Liked,
                    list: ListState::default(),
                },
            );
            self.mode = Mode::Search {
                input: String::new(),
                results: SectionState::Idle,
                list: ListState::default(),
                back: Box::new(prev),
            };
            return;
        }

        // Global player keys (gated behind !Search above).
        match k.code {
            KeyCode::Char('p') | KeyCode::Char(' ') => {
                self.pending.push(UiAction::Toggle);
                return;
            }
            KeyCode::Char('n') => {
                self.pending.push(UiAction::Next);
                return;
            }
            KeyCode::Char('b') => {
                self.pending.push(UiAction::Previous);
                return;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.pending.push(UiAction::VolumeUp);
                return;
            }
            KeyCode::Char('-') => {
                self.pending.push(UiAction::VolumeDown);
                return;
            }
            _ => {}
        }

        // Phase 1: snapshot Library state for borrow-free decisions.
        let lib_state = if let Mode::Library { tab, list } = &self.mode {
            Some((*tab, list.selected(), self.section_len(*tab)))
        } else {
            None
        };

        // Phase 2a: handle Enter in Library mode (pushes a UiAction; no
        // mutable borrow of self.mode held while pushing to self.pending).
        if let Some((tab, sel, _len)) = lib_state
            && matches!(k.code, KeyCode::Enter)
        {
            match (tab, sel) {
                (LibTab::Albums, Some(idx)) => {
                    if let SectionState::Loaded(v) = &self.albums
                        && let Some(a) = v.get(idx)
                    {
                        self.pending.push(UiAction::LoadAlbumTracks {
                            id: a.id.clone(),
                            title: a.name.clone(),
                        });
                    }
                }
                (LibTab::Playlists, Some(idx)) => {
                    if let SectionState::Loaded(v) = &self.playlists
                        && let Some(p) = v.get(idx)
                    {
                        self.pending.push(UiAction::LoadPlaylistTracks {
                            id: p.id.clone(),
                            title: p.name.clone(),
                        });
                    }
                }
                (LibTab::Liked, Some(idx)) => {
                    if let SectionState::Loaded(v) = &self.liked
                        && !v.is_empty()
                        && idx < v.len()
                    {
                        let uris: Vec<TrackId> = v.iter().map(|t| t.id.clone()).collect();
                        self.pending
                            .push(UiAction::PlayContext { uris, start: idx });
                    }
                }
                (LibTab::Recent, Some(idx)) => {
                    if let SectionState::Loaded(v) = &self.recent
                        && let Some(e) = v.get(idx)
                    {
                        self.pending.push(UiAction::Play(e.track.id.clone()));
                    }
                }
                _ => {}
            }
            return;
        }

        // Phase 2b: Enter in Detail mode (play tracks list starting at idx).
        if matches!(k.code, KeyCode::Enter)
            && let Mode::Detail { tracks, list, .. } = &self.mode
        {
            if let Some(idx) = list.selected()
                && idx < tracks.len()
            {
                let uris: Vec<TrackId> = tracks.iter().map(|t| t.id.clone()).collect();
                self.pending
                    .push(UiAction::PlayContext { uris, start: idx });
            }
            return;
        }

        // Phase 2b2: cursor navigation in Detail mode.
        if let Mode::Detail { tracks, list, .. } = &mut self.mode {
            let len = tracks.len();
            match k.code {
                KeyCode::Char('j') | KeyCode::Down => move_cursor(list, len, 1),
                KeyCode::Char('k') | KeyCode::Up => move_cursor(list, len, -1),
                _ => {}
            }
            return;
        }

        // Phase 2c: tab/cursor mutation in Library mode.
        if let Mode::Library { tab, list } = &mut self.mode {
            let lib_len = lib_state.map(|(_, _, l)| l);
            match (k.code, k.modifiers) {
                (KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right, m)
                    if !m.contains(KeyModifiers::SHIFT) =>
                {
                    *tab = tab.next();
                    *list = ListState::default();
                }
                (KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left, _) => {
                    *tab = tab.previous();
                    *list = ListState::default();
                }
                (KeyCode::Tab, m) if m.contains(KeyModifiers::SHIFT) => {
                    *tab = tab.previous();
                    *list = ListState::default();
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if let Some(len) = lib_len {
                        move_cursor(list, len, 1);
                    }
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if let Some(len) = lib_len {
                        move_cursor(list, len, -1);
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_key_search(&mut self, k: crossterm::event::KeyEvent) {
        // Snapshot Enter intent: if results loaded + selection valid, play that track.
        if matches!(k.code, KeyCode::Enter) {
            let play_id = if let Mode::Search {
                results: SectionState::Loaded(tracks),
                list,
                ..
            } = &self.mode
            {
                list.selected()
                    .and_then(|idx| tracks.get(idx))
                    .map(|t| t.id.clone())
            } else {
                None
            };
            if let Some(id) = play_id {
                self.pending.push(UiAction::Play(id));
                return;
            }
            // Otherwise dispatch a search if input is non-empty.
            if let Mode::Search { input, results, .. } = &mut self.mode
                && !input.is_empty()
            {
                let q = input.clone();
                *results = SectionState::Loading;
                self.pending.push(UiAction::Search(q));
            }
            return;
        }

        if let Mode::Search {
            input,
            results,
            list,
            back,
        } = &mut self.mode
        {
            match k.code {
                KeyCode::Esc => {
                    let back_mode = std::mem::replace(
                        back.as_mut(),
                        Mode::Library {
                            tab: LibTab::Liked,
                            list: ListState::default(),
                        },
                    );
                    self.mode = back_mode;
                }
                KeyCode::Up => {
                    if let SectionState::Loaded(tracks) = results {
                        let len = tracks.len();
                        if len > 0 {
                            let cur = list.selected().unwrap_or(0) as i32;
                            let next = (cur - 1).rem_euclid(len as i32) as usize;
                            list.select(Some(next));
                        }
                    }
                }
                KeyCode::Down => {
                    if let SectionState::Loaded(tracks) = results {
                        let len = tracks.len();
                        if len > 0 {
                            let cur = list.selected().unwrap_or(0) as i32;
                            let next = (cur + 1).rem_euclid(len as i32) as usize;
                            list.select(Some(next));
                        }
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => {
                    input.push(c);
                }
                _ => {}
            }
        }
    }

    fn clear_stale_toast(&mut self) {
        if let Some((at, _)) = self.toast
            && at.elapsed() > Duration::from_secs(5)
        {
            self.toast = None;
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
