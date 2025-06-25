use eyre::Result;
use tracing_subscriber::{EnvFilter, prelude::*};
use tracing_tree::HierarchicalLayer;

pub mod cli;
mod command;
mod git;
pub mod github;
pub mod package;
pub mod quest;
mod source;
pub mod stage;
pub mod utils;

pub fn init_globals() -> Result<()> {
  color_eyre::install()?;
  tracing_subscriber::registry()
    .with(HierarchicalLayer::default())
    .with(EnvFilter::from_default_env())
    .init();
  github::init_octocrab()?;
  Ok(())
}
