use chrono::Utc;
use rspotify::model::{
    AlbumId as RsAlbumId, ArtistId as RsArtistId, FullTrack, SavedTrack, SimplifiedAlbum,
    SimplifiedArtist, TrackId as RsTrackId, Type,
};
use spfy_core::api::convert::{full_track_to_model, saved_track_to_model};

fn sample_full_track() -> FullTrack {
    FullTrack {
        id: Some(RsTrackId::from_id("6rqhFgbbKwnb9MLmUQDhG6").unwrap()),
        name: "Don't Stop Me Now".into(),
        artists: vec![SimplifiedArtist {
            id: Some(RsArtistId::from_id("1dfeR4HaWDbWqFHLkxsg1d").unwrap()),
            name: "Queen".into(),
            external_urls: Default::default(),
            href: None,
        }],
        album: SimplifiedAlbum {
            id: Some(RsAlbumId::from_id("1GbtB4zTqAsyfZEsm1RZfx").unwrap()),
            name: "Jazz".into(),
            artists: vec![],
            album_group: None,
            album_type: None,
            available_markets: vec![],
            external_urls: Default::default(),
            href: None,
            images: vec![],
            release_date: None,
            release_date_precision: None,
            restrictions: None,
        },
        available_markets: vec![],
        disc_number: 1,
        duration: chrono::TimeDelta::milliseconds(209_000),
        explicit: false,
        external_ids: Default::default(),
        external_urls: Default::default(),
        href: None,
        is_local: false,
        is_playable: Some(true),
        linked_from: None,
        restrictions: None,
        popularity: 0,
        preview_url: None,
        track_number: 1,
        r#type: Type::Track,
    }
}

#[test]
fn full_track_converts_to_model_track() {
    let model = full_track_to_model(sample_full_track());
    assert_eq!(model.name, "Don't Stop Me Now");
    assert_eq!(model.artists, vec!["Queen".to_string()]);
    assert_eq!(model.album, "Jazz");
    assert_eq!(model.duration_ms, 209_000);
    assert!(model.id.0.starts_with("spotify:track:"));
}

#[test]
fn saved_track_unwraps_to_full_track() {
    let st = SavedTrack {
        added_at: Utc::now(),
        track: sample_full_track(),
    };
    let model = saved_track_to_model(st);
    assert_eq!(model.name, "Don't Stop Me Now");
}
