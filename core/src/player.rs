use tokio::sync::mpsc;

use crate::model::TrackId;

pub mod queue {
    use crate::model::TrackId;

    #[derive(Default)]
    pub struct Queue {
        items: Vec<TrackId>,
        idx: usize,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum AdvanceResult {
        Loaded(TrackId),
        EndReached,
    }

    impl Queue {
        pub fn set(&mut self, items: Vec<TrackId>, start: usize) -> Option<TrackId> {
            self.items = items;
            self.idx = start.min(self.items.len().saturating_sub(1));
            self.items.get(self.idx).cloned()
        }

        pub fn current(&self) -> Option<&TrackId> {
            self.items.get(self.idx)
        }

        pub fn next(&mut self) -> AdvanceResult {
            if self.idx + 1 >= self.items.len() {
                return AdvanceResult::EndReached;
            }
            self.idx += 1;
            AdvanceResult::Loaded(self.items[self.idx].clone())
        }

        pub fn previous(&mut self) -> AdvanceResult {
            self.idx = self.idx.saturating_sub(1);
            match self.items.get(self.idx) {
                Some(t) => AdvanceResult::Loaded(t.clone()),
                None => AdvanceResult::EndReached,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Cmd {
    Play(TrackId),
    PlayContext { uris: Vec<TrackId>, start: usize },
    Toggle,
    Next,
    Previous,
    Seek(u32),
    SetVolume(u8),
    Quit,
}

#[derive(Debug, Clone)]
pub enum Event {
    Started { track: TrackId, duration_ms: u32 },
    Resumed,
    Paused,
    Position(u32),
    EndOfTrack,
    Stopped,
    Error(String),
}

pub struct PlayerHandle {
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    event_rx: Option<mpsc::UnboundedReceiver<Event>>,
}

impl PlayerHandle {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn take_events(&mut self) -> mpsc::UnboundedReceiver<Event> {
        self.event_rx
            .take()
            .expect("PlayerHandle::take_events called twice")
    }
}
