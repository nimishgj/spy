use spfy_core::model::{Track, TrackId};

#[test]
fn track_id_round_trips_through_string() {
    let id = TrackId("spotify:track:6rqhFgbbKwnb9MLmUQDhG6".to_string());
    assert_eq!(id.0, "spotify:track:6rqhFgbbKwnb9MLmUQDhG6");
}

#[test]
fn track_struct_can_be_constructed() {
    let t = Track {
        id: TrackId("spotify:track:abc".into()),
        name: "Bohemian Rhapsody".into(),
        artists: vec!["Queen".into()],
        album: "A Night at the Opera".into(),
        duration_ms: 354_000,
    };
    assert_eq!(t.duration_ms, 354_000);
    assert_eq!(t.artists, vec!["Queen".to_string()]);
}
