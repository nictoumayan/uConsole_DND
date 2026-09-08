//! Where the snapshot and (later) session state live on disk.

use anyhow::{Context, Result};
use std::path::PathBuf;

fn project_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "vellum")
        .context("could not resolve a config directory for this platform")?;
    Ok(dirs.data_dir().to_path_buf())
}

/// The cached character payload. Immutable once written — session state will
/// live in a separate file so that re-importing after a level-up never
/// clobbers the HP you are tracking mid-combat.
pub fn snapshot_path(character_id: i64) -> Result<PathBuf> {
    Ok(project_dir()?.join(format!("{character_id}.snapshot.json")))
}

/// The cached portrait bytes, next to the snapshot so `show` stays offline.
pub fn avatar_path(character_id: i64) -> Result<PathBuf> {
    Ok(project_dir()?.join(format!("{character_id}.avatar")))
}

/// The id of the character `vellum show` uses when none is given.
pub fn default_id_path() -> Result<PathBuf> {
    Ok(project_dir()?.join("default_character"))
}

pub fn ensure_dir() -> Result<PathBuf> {
    let dir = project_dir()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}
