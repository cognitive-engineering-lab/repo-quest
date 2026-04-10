use std::{path::Path, process::Command};

use anyhow::Result;
use repo_quest_core::command::RunCommand as _;

pub fn rsync(from: &Path, to: &Path) -> Result<()> {
    let to = std::fs::canonicalize(to)?;
    Command::new("rsync")
        .current_dir(from)
        .arg("-a")
        .arg("--delete")
        .arg("--exclude=.git")
        .arg(".")
        .arg(&to)
        .run_with_context(|| format!("Could not rsync files from {from:?} to {to:?}."))
}
