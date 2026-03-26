mod convert;
mod init;

use std::{fs, path::Path};

use anyhow::{Context as _, Result, bail};

pub use convert::{dir_to_hist, overlay, prepare_propagate_repo, quest_to_hist};
pub use init::init;

/// Creates the output dir if it does not exist. Fails with `Err` if the output
/// dir exists but is not empty.
fn ensure_empty_dir(output_dir: &Path) -> Result<()> {
    if !output_dir.exists() {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("Could not create output directory {output_dir:?}."))?;
    } else if !output_dir.is_dir()
        || output_dir
            .read_dir()
            .with_context(|| format!("Cannot read output directory {output_dir:?}."))?
            .next()
            .is_some()
    {
        bail!("Given output output path exists but is not an empty directory.")
    }
    Ok(())
}
