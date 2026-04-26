use spfy_core::paths;

#[test]
fn config_dir_resolves_under_home() {
    let dir = paths::config_dir().expect("config dir");
    let s = dir.to_string_lossy();
    assert!(s.contains("spfy"), "expected 'spfy' in {s}");
}

#[test]
fn rspotify_token_path_is_under_config() {
    let token = paths::rspotify_token_path().unwrap();
    let cfg = paths::config_dir().unwrap();
    assert!(token.starts_with(&cfg));
    assert_eq!(token.file_name().unwrap(), "rspotify_token.json");
}
