mod dir;
mod github;
mod propagate;
mod util;

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
    /// Converts a quest definition from a collection of directories to a git
    /// repository, and starts a git rebase operation to propagating changes
    /// from one directory to later directories. After the rebase is complete,
    /// use the `overlay` command to convert the repository back.
    ///
    /// Because the working directory for performing the rebase should only be
    /// used for the rebase, it is created as a tempdir and the name of the
    /// directory is printed to stdout.
    ///
    /// In order to determine how to structure the rebase, this command requires
    /// two revesions of the full quest sequence, and so it operates on
    /// committed versions of a quest. A typical use would be:
    ///
    /// - Start from a quest definition repository with no uncommitted changes.
    /// - Make a change taht will need to be propagated forward. (Since it can
    ///   only be propagated forward, make the change to the earliest quest stage
    ///   that needs it.)
    /// - Commit the change.
    /// - Use this command to produce a git repository representing the quest
    ///   stages and set up the needed rebase.
    /// - Complete the rebase.
    /// - Use the `overlay` command to update the working directory of the
    ///   quest definition repository.
    /// - Amend the commit with the forward-propagated changes.
    Propagate {
        /// The quest definition that has a change that requires propagating.
        #[arg(long)]
        quest: PathBuf,
        /// A git ref for the baseline quest definition. (Often `HEAD^`.)
        #[arg(long)]
        original: String,
        /// A git ref for the quest definition with the change needing
        /// propagation. (Often `HEAD`.)
        #[arg(long)]
        changed: String,
    },
    /// Overlay branches from a converted repository back onto the collection of
    /// directories in a quest definition.
    ///
    /// Will only overlay on a git repository with no uncommitted changes.
    ///
    /// See the propagate command for more information.
    Overlay {
        /// The quest definition for which the rebase was used to propagate a
        /// change.
        #[arg(long)]
        quest: PathBuf,
        /// The repository in which a rebase was completed in order to propagate
        /// a change.
        #[arg(long)]
        rebase_repo: PathBuf,
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
        Command::Propagate {
            quest,
            original,
            changed,
        } => {
            let rebase_todo = propagate::prepare_propagate_repo(&quest, &original, &changed)?;
            println!("{rebase_todo}");
        }
        Command::Overlay { quest, rebase_repo } => propagate::overlay(&rebase_repo, &quest)?,
    };

    Ok(())
}
