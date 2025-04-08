//! The core functionality of the RepoQuest system.
//!
//! This is factored into its own crate so it can be reused by the RepoQuest Tauri binary and the rq-cli binary.

mod command;
pub mod git;
pub mod github;
pub mod package;
pub mod quest;
pub mod stage;
mod template;
mod utils;
