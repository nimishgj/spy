use chrono::{Duration, Utc};
use spfy_core::auth::{persist_rspotify_token, read_rspotify_token, StoredToken};

#[test]
fn token_json_round_trips_via_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token.json");

    let original = StoredToken {
        access_token: "AT-abc".into(),
        refresh_token: Some("RT-xyz".into()),
        expires_at: Utc::now() + Duration::seconds(3600),
        scopes: vec!["streaming".into(), "user-library-read".into()],
    };

    persist_rspotify_token(&path, &original).unwrap();
    let loaded = read_rspotify_token(&path).unwrap().expect("token present");

    assert_eq!(loaded.access_token, original.access_token);
    assert_eq!(loaded.refresh_token, original.refresh_token);
    assert_eq!(loaded.scopes, original.scopes);
}

#[test]
fn missing_token_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.json");
    let loaded = read_rspotify_token(&path).unwrap();
    assert!(loaded.is_none());
}
