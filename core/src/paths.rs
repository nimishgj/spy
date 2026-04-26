use std::path::PathBuf;

use directories::ProjectDirs;

use crate::error::{CoreError, Result};

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("io", "spfy", "spfy")
        .ok_or_else(|| CoreError::Io(std::io::Error::other("could not resolve project dirs")))
}

pub fn config_dir() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    let path = dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn librespot_cache_dir() -> Result<PathBuf> {
    let path = config_dir()?.join("librespot");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn rspotify_token_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("rspotify_token.json"))
}

pub fn log_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("spfy.log"))
}
