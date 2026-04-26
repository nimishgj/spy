use std::time::Duration;

use crossterm::event::{self, Event as CtEvent, KeyEvent};
use spfy_core::player::Event as PlayerEvent;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    Player(PlayerEvent),
    LibraryLoaded(LibrarySection),
    LibraryFailed(SectionId, String),
}

#[derive(Debug, Clone, Copy)]
pub enum SectionId {
    Liked,
    Albums,
    Playlists,
    Artists,
    Recent,
}

#[derive(Debug)]
pub enum LibrarySection {
    Liked(Vec<spfy_core::model::Track>),
    Albums(Vec<spfy_core::model::Album>),
    Playlists(Vec<spfy_core::model::Playlist>),
    Artists(Vec<spfy_core::model::Artist>),
    Recent(Vec<spfy_core::model::PlayHistoryEntry>),
}

pub fn channel() -> (mpsc::UnboundedSender<AppEvent>, mpsc::UnboundedReceiver<AppEvent>) {
    mpsc::unbounded_channel()
}

pub fn spawn_key_thread(tx: mpsc::UnboundedSender<AppEvent>) {
    std::thread::spawn(move || loop {
        match event::poll(Duration::from_millis(250)) {
            Ok(true) => match event::read() {
                Ok(CtEvent::Key(k)) => {
                    if tx.send(AppEvent::Key(k)).is_err() {
                        break;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    });
}

pub fn spawn_tick(tx: mpsc::UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;
            if tx.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });
}

pub fn spawn_player_forwarder(
    tx: mpsc::UnboundedSender<AppEvent>,
    mut player_rx: mpsc::UnboundedReceiver<PlayerEvent>,
) {
    tokio::spawn(async move {
        while let Some(ev) = player_rx.recv().await {
            if tx.send(AppEvent::Player(ev)).is_err() {
                break;
            }
        }
    });
}
