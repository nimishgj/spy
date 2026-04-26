use rspotify::AuthCodeSpotify;

use crate::error::{CoreError, Result};

pub struct SpotifyApi {
    pub(crate) client: AuthCodeSpotify,
}

impl SpotifyApi {
    pub fn new(client: AuthCodeSpotify) -> Self {
        Self { client }
    }
}

/// Run `f`; if it returns `CoreError::Auth(...)`, run it once more.
/// Useful when an access token has just expired mid-call.
pub fn retry_once_on_auth<T, F>(mut f: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    match f() {
        Err(CoreError::Auth(_)) => f(),
        other => other,
    }
}
