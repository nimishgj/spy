use std::path::PathBuf;

use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::SessionConfig;
use librespot::core::session::Session;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tracing::info;

use crate::model::TrackId;
use crate::paths;

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

pub fn spawn(creds: Credentials, rt: Handle) -> PlayerHandle {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Cmd>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<Event>();

    rt.spawn(async move {
        if let Err(e) = run(creds, cmd_rx, event_tx.clone()).await {
            let _ = event_tx.send(Event::Error(e.to_string()));
        }
    });

    PlayerHandle {
        cmd_tx,
        event_rx: Some(event_rx),
    }
}

async fn run(
    creds: Credentials,
    mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
    event_tx: mpsc::UnboundedSender<Event>,
) -> anyhow::Result<()> {
    let cache_dir = paths::librespot_cache_dir()?;
    let cache = Cache::new(
        Some::<PathBuf>(cache_dir.clone()),
        None,
        Some(cache_dir),
        None,
    )
    .map_err(|e| anyhow::anyhow!("cache: {e}"))?;
    let session = Session::new(SessionConfig::default(), Some(cache));

    info!("connecting to Spotify");
    if let Err(e) = session.connect(creds, true).await {
        let msg = e.to_string();
        if msg.to_lowercase().contains("premium")
            || msg.to_lowercase().contains("badcredentials")
        {
            let _ = event_tx.send(Event::Error("Premium account required".into()));
        } else {
            let _ = event_tx.send(Event::Error(format!("connect failed: {msg}")));
        }
        return Ok(());
    }

    // Continuation in Task 15.
    info!("session connected; player setup deferred to Task 15");
    let _ = event_tx;
    let _ = cmd_rx;
    let _ = session;
    Ok(())
}
