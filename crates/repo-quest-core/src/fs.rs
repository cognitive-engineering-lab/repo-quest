use anyhow::{Context, Result};
use std::path::Path;

pub use std::fs::File;

pub fn write(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
    path_kind: impl AsRef<str>,
) -> Result<()> {
    let path = path.as_ref();
    std::fs::write(path, contents).with_context(|| {
        format!(
            "failed to write to {}: `{}`",
            path_kind.as_ref(),
            path.display()
        )
    })
}

pub fn read_to_string(path: impl AsRef<Path>, path_kind: impl AsRef<str>) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read {} to string: `{}`",
            path_kind.as_ref(),
            path.display()
        )
    })
}

pub fn create_dir(path: impl AsRef<Path>, path_kind: impl AsRef<str>) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir(path).with_context(|| {
        format!(
            "failed to create {}: `{}`",
            path_kind.as_ref(),
            path.display()
        )
    })
}

pub fn create_dir_all(path: impl AsRef<Path>, path_kind: impl AsRef<str>) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path).with_context(|| {
        format!(
            "failed to create {}: `{}`",
            path_kind.as_ref(),
            path.display()
        )
    })
}

pub fn remove_dir_all(path: impl AsRef<Path>, path_kind: impl AsRef<str>) -> Result<()> {
    let path = path.as_ref();
    std::fs::remove_dir_all(path).with_context(|| {
        format!(
            "failed to remove {}: `{}`",
            path_kind.as_ref(),
            path.display()
        )
    })
}
