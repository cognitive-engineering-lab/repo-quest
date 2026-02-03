use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use log::{debug, warn};

use super::*;

/// Converts a `dir::QuestDefinition` into the bundle format on disk.
pub fn bundle(quest: QuestDefinition, output: PathBuf) -> Result<()> {
    Ok(())
}
