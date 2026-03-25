use anyhow::{Context as _, Result};
use include_dir::include_dir;
use std::path::Path;

static SKEL_DIR: include_dir::Dir<'_> = include_dir!("assets/skel");

/// Initialize a quest from a skeleton definition.
///
/// The goal of the particular skeleton defintion is to have enough content that
/// it is easy to guess what can be done with the definition without having to
/// read the docs. See `assets/skel/`.
pub fn init(quest_dir: &Path) -> Result<()> {
    super::ensure_empty_dir(quest_dir)?;

    SKEL_DIR
        .extract(quest_dir)
        .with_context(|| format!("Could not initialize {quest_dir:?}."))?;

    Ok(())
}
