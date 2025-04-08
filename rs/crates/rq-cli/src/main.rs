use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use eyre::Result;
use rq_core::{
  github::{self, GithubToken},
  package::QuestPackage,
};

mod file_completer;
mod spinner;
mod ui;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
  #[command(subcommand)]
  command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
  Pack { path: PathBuf },
}

#[tokio::main]
async fn main() -> Result<()> {
  color_eyre::install()?;
  let args = Cli::parse();
  match args.command {
    None => ui::ui_main().await,
    Some(Command::Pack { path }) => {
      let token = github::get_github_token();
      match token {
        GithubToken::Found(token) => github::init_octocrab(&token).unwrap(),
        other => panic!("Failed to get github token: {other:?}"),
      }
      let package = QuestPackage::build(&path).await?;
      let dst = format!("{}.json.gz", package.config.repo);
      package.save(Path::new(&dst))?;
      println!("Successfully generated quest package: {dst}");

      Ok(())
    }
  }
}
