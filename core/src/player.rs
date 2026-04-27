use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::SessionConfig;
use librespot::core::session::Session;
use librespot::core::spotify_uri::SpotifyUri;
use librespot::metadata::audio::item::UniqueFields;
use librespot::playback::audio_backend;
use librespot::playback::config::{AudioFormat, PlayerConfig, VolumeCtrl};
use librespot::playback::mixer::softmixer::SoftMixer;
use librespot::playback::mixer::{Mixer, MixerConfig};
use librespot::playback::player::{Player, PlayerEvent};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig,
};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::model::TrackId;
use crate::paths;
use crate::player::queue::{AdvanceResult, Queue};

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

    let internal_cmd_tx = cmd_tx.clone();
    rt.spawn(async move {
        if let Err(e) = run(creds, cmd_rx, event_tx.clone(), internal_cmd_tx).await {
            let _ = event_tx.send(Event::Error(e.to_string()));
        }
    });

    PlayerHandle {
        cmd_tx,
        event_rx: Some(event_rx),
    }
}

fn load_track(player: &Arc<Player>, id: &TrackId) -> anyhow::Result<()> {
    let uri = SpotifyUri::from_uri(&id.0)
        .map_err(|e| anyhow::anyhow!("bad uri {}: {e:?}", id.0))?;
    player.load(uri, true, 0);
    Ok(())
}

async fn run(
    creds: Credentials,
    mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
    event_tx: mpsc::UnboundedSender<Event>,
    internal_cmd_tx: mpsc::UnboundedSender<Cmd>,
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

    // ---- Audio backend + mixer ----
    let mixer: Arc<dyn Mixer> = Arc::new(
        SoftMixer::open(MixerConfig {
            volume_ctrl: VolumeCtrl::Linear,
            ..MixerConfig::default()
        })
        .map_err(|e| anyhow::anyhow!("mixer: {e}"))?,
    );
    let backend = audio_backend::find(Some("rodio".into()))
        .ok_or_else(|| anyhow::anyhow!("rodio backend missing"))?;
    let player = Player::new(
        PlayerConfig::default(),
        session.clone(),
        mixer.get_soft_volume(),
        move || backend(None, AudioFormat::default()),
    );

    let mut player_events = player.get_player_event_channel();
    let mut queue = Queue::default();
    let mut anchor: Option<(Instant, u32)> = None;
    let mut playing = false;
    let mut tick = tokio::time::interval(Duration::from_millis(500));

    // ---- macOS media-key / Now Playing widget integration ----
    let mut media_controls = match MediaControls::new(PlatformConfig {
        dbus_name: "spfy",
        display_name: "spfy",
        hwnd: None,
    }) {
        Ok(c) => Some(c),
        Err(e) => {
            warn!("media controls init failed: {e:?}");
            None
        }
    };
    if let Some(mc) = media_controls.as_mut() {
        let media_tx = internal_cmd_tx.clone();
        if let Err(e) = mc.attach(move |event: MediaControlEvent| {
            let cmd = match event {
                MediaControlEvent::Play
                | MediaControlEvent::Pause
                | MediaControlEvent::Toggle => Some(Cmd::Toggle),
                MediaControlEvent::Next => Some(Cmd::Next),
                MediaControlEvent::Previous => Some(Cmd::Previous),
                MediaControlEvent::Stop => Some(Cmd::Toggle),
                _ => None,
            };
            if let Some(c) = cmd {
                let _ = media_tx.send(c);
            }
        }) {
            warn!("media controls attach failed: {e:?}");
        }
    }

    loop {
        tokio::select! {
            Some(cmd) = cmd_rx.recv() => match cmd {
                Cmd::Play(id) => {
                    queue.set(vec![id.clone()], 0);
                    if let Err(e) = load_track(&player, &id) {
                        let _ = event_tx.send(Event::Error(e.to_string()));
                    } else {
                        playing = true;
                    }
                }
                Cmd::PlayContext { uris, start } => {
                    if let Some(first) = queue.set(uris, start) {
                        if let Err(e) = load_track(&player, &first) {
                            let _ = event_tx.send(Event::Error(e.to_string()));
                        } else {
                            playing = true;
                        }
                    }
                }
                Cmd::Toggle => {
                    if playing { player.pause(); } else { player.play(); }
                }
                Cmd::Next => {
                    if let AdvanceResult::Loaded(id) = queue.next() {
                        let _ = load_track(&player, &id);
                        playing = true;
                    } else {
                        let _ = event_tx.send(Event::Stopped);
                        playing = false;
                    }
                }
                Cmd::Previous => {
                    if let AdvanceResult::Loaded(id) = queue.previous() {
                        let _ = load_track(&player, &id);
                        playing = true;
                    }
                }
                Cmd::Seek(ms) => {
                    player.seek(ms);
                    anchor = Some((Instant::now(), ms));
                }
                Cmd::SetVolume(v) => {
                    let scaled = (v as u32 * 65535 / 100).min(65535) as u16;
                    mixer.set_volume(scaled);
                }
                Cmd::Quit => break,
            },
            Some(ev) = player_events.recv() => match ev {
                PlayerEvent::TrackChanged { audio_item } => {
                    let id = TrackId(audio_item.track_id.to_uri().unwrap_or_default());
                    let dur = audio_item.duration_ms;
                    anchor = Some((Instant::now(), 0));

                    // Push metadata to macOS Now Playing widget.
                    if let Some(mc) = media_controls.as_mut() {
                        let title = audio_item.name.clone();
                        let (artist, album) = match &audio_item.unique_fields {
                            UniqueFields::Track { artists, album, .. } => {
                                let names: Vec<&str> =
                                    artists.0.iter().map(|a| a.name.as_str()).collect();
                                (names.join(", "), album.clone())
                            }
                            UniqueFields::Local {
                                artists, album, ..
                            } => (
                                artists.clone().unwrap_or_default(),
                                album.clone().unwrap_or_default(),
                            ),
                            UniqueFields::Episode { show_name, .. } => {
                                (show_name.clone(), String::new())
                            }
                        };
                        let cover_url = audio_item
                            .covers
                            .first()
                            .map(|c| c.url.clone());
                        mc.set_metadata(MediaMetadata {
                            title: Some(title.as_str()),
                            artist: Some(artist.as_str()),
                            album: Some(album.as_str()),
                            cover_url: cover_url.as_deref(),
                            duration: Some(Duration::from_millis(dur as u64)),
                        })
                        .ok();
                        mc.set_playback(MediaPlayback::Playing { progress: None }).ok();
                    }

                    let _ = event_tx.send(Event::Started { track: id, duration_ms: dur });
                }
                PlayerEvent::Playing { position_ms, .. } => {
                    anchor = Some((Instant::now(), position_ms));
                    playing = true;
                    if let Some(mc) = media_controls.as_mut() {
                        mc.set_playback(MediaPlayback::Playing { progress: None }).ok();
                    }
                    let _ = event_tx.send(Event::Resumed);
                }
                PlayerEvent::Paused { position_ms, .. } => {
                    anchor = Some((Instant::now(), position_ms));
                    playing = false;
                    if let Some(mc) = media_controls.as_mut() {
                        mc.set_playback(MediaPlayback::Paused { progress: None }).ok();
                    }
                    let _ = event_tx.send(Event::Paused);
                }
                PlayerEvent::EndOfTrack { .. } => {
                    let _ = event_tx.send(Event::EndOfTrack);
                    if let AdvanceResult::Loaded(id) = queue.next() {
                        let _ = load_track(&player, &id);
                    } else {
                        playing = false;
                        if let Some(mc) = media_controls.as_mut() {
                            mc.set_playback(MediaPlayback::Stopped).ok();
                        }
                        let _ = event_tx.send(Event::Stopped);
                    }
                }
                PlayerEvent::Unavailable { .. } => {
                    warn!("track unavailable; skipping");
                    if let AdvanceResult::Loaded(id) = queue.next() {
                        let _ = load_track(&player, &id);
                    }
                }
                _ => {}
            },
            _ = tick.tick() => {
                if playing {
                    if let Some((started, base)) = anchor {
                        let elapsed = (Instant::now() - started).as_millis() as u32;
                        let _ = event_tx.send(Event::Position(base + elapsed));
                    }
                }
            }
        }
    }

    info!("player worker exiting");
    if let Some(mc) = media_controls.as_mut() {
        mc.set_playback(MediaPlayback::Stopped).ok();
    }
    player.stop();
    Ok(())
}
