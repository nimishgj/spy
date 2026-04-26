use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Spotify API error: {0}")]
    Api(String),

    #[error("Premium account required")]
    NotPremium,

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Player error: {0}")]
    Player(String),

    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
