use eyre::Result;
use tracing_subscriber::{EnvFilter, prelude::*};
use tracing_tree::HierarchicalLayer;

pub mod chapter;
pub mod cli;
pub mod command;
pub mod git;
pub mod github;
pub mod package;
pub mod quest;
pub mod source;
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
