mod convert;
mod init;
mod ls;

use std::{fs, path::Path};

use anyhow::{Context as _, Result, ensure};

pub use convert::{dir_to_hist, overlay, prepare_propagate_repo, quest_to_hist};
pub use init::init;
pub use ls::quest_tree;

/// Creates the output dir if it does not exist. Fails with `Err` if the output
/// dir exists but is not empty.
fn ensure_empty_dir(output_dir: &Path) -> Result<()> {
    if !output_dir.exists() {
        fs::create_dir_all(output_dir).with_context(|| {
            format!(
                "Could not create output directory `{}`.",
                output_dir.display()
            )
        })?;
    }

    ensure!(
        output_dir.is_dir(),
        "Output path `{}` is not a directory",
        output_dir.display()
    );

    let is_empty = output_dir
        .read_dir()
        .with_context(|| format!("Cannot read output directory `{}`.", output_dir.display()))?
        .next()
        .is_none();
    ensure!(
        is_empty,
        "Given output path `{}` exists but is not an empty directory.",
        output_dir.display()
    );

    Ok(())
}
