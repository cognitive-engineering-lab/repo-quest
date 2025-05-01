use std::{
  env::current_dir,
  iter::Skip,
  path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use color_eyre::owo_colors::OwoColorize;
use eyre::{Result, bail};
use rq_core::{
  github::{self, GithubToken},
  package::QuestPackage,
  quest::{NoopEmitter, Quest},
};
use spinner::spinner;

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
  /// Skip to a given chapter.
  Skip { chapter: usize },

  /// Package a quest template into a quest file.
  Pack { path: PathBuf },
}

#[tokio::main]
async fn main() -> Result<()> {
  color_eyre::install()?;
  let args = Cli::parse();

  println!(
    "{}",
    format!("Welcome to RepoQuest v{}!", env!("CARGO_PKG_VERSION")).bold()
  );

  let token = github::get_github_token();
  match token {
    GithubToken::Found(token) => github::init_octocrab(&token)?,
    other => bail!("Failed to get github token: {other:?}"),
  }

  let cwd = current_dir()?;

  match args.command {
    None => ui::ui_main().await,
    Some(Command::Skip { chapter }) => {
      let quest_fut = Quest::load(&cwd, Box::new(NoopEmitter));
      let quest = spinner("Loading quest from current directory...", quest_fut).await?;
      spinner(
        format!("Advancing to Chapter {chapter}..."),
        quest.skip_to_stage(chapter),
      )
      .await?;
      println!("Done!");
      Ok(())
    }
    Some(Command::Pack { path }) => {
      let package = QuestPackage::build(&path).await?;
      let dst = format!("{}.json.gz", package.config.repo);
      package.save(Path::new(&dst))?;
      println!("Successfully generated quest package: {dst}");

      Ok(())
    }
  }
}
