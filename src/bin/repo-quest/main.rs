mod dir;
mod github;

use std::path::PathBuf;

use crate::github::*;

use anyhow::Result;
use clap::Parser;
use env_logger::Env;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Bundles a GitHub-based quest definition for use with a RepoQuest Forgejo
    /// instance.
    #[command(name = "bundle-github")]
    BundleGitHub {
        /// GitHub access token, e.g., `$GITHUB_TOKEN` in a GitHub action.
        #[arg(long)]
        token: Option<String>,
        /// The base URI for the GitHub instance. Defaults to `http://api.github.com`.
        #[arg(long, default_value = "https://api.github.com")]
        base_uri: String,
        /// The owner of the repository (e.g., username or organization name).
        #[arg(long)]
        owner: String,
        /// The name of the repository.
        #[arg(long)]
        repo: String,
        /// The path to which to write the bundle archive.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Bundles a quest definition for use with a RepoQuest Forgejo instance.
    #[command(name = "bundle")]
    BundleDir {
        #[arg(long)]
        input: PathBuf,
        /// The path to which to write the bundle archive.
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args { command } = Args::parse();

    #[cfg(not(debug_assertions))]
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    match command {
        Command::BundleGitHub {
            token,
            base_uri,
            owner,
            repo,
            output,
        } => bundle_github(output, token, base_uri, owner, repo).await?,
        Command::BundleDir { input, output } => {
            let quest = dir::parse(&input)?;
            dir::bundle(quest, &output)?;
        }
    };

    Ok(())
}
