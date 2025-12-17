use anyhow::Context as _;
use clap::Parser;
use env_logger::Env;
use repo_quest::command::RunCommand as _;
use repo_quest::git::GitRepo;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Converts a sequence of folders into a repository that can be used as a
/// starting point for a RepoQuest quest definition.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to root of input folders.
    #[arg(short, long)]
    input: PathBuf,
    /// Where to create the git repository.
    #[arg(short, long)]
    output: PathBuf,
}

type Result<A> = anyhow::Result<A>;

#[tokio::main]
async fn main() -> Result<()> {
    let Args { input, output } = Args::parse();

    #[cfg(not(debug_assertions))]
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    let repo = GitRepo::init(output.clone())?;

    // TODO: get list of directories in the directory given by input
    let dir_entries = std::fs::read_dir(&input)?;
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir_entry in dir_entries {
        let dir = dir_entry?;
        if dir.file_type()?.is_dir() {
            dirs.push(dir.path())
        }
    }
    dirs.sort();

    repo.commit("Initial commit")?;
    let initial_commit = repo.rev_parse("HEAD")?;
    for dir in dirs {
        let dir_path = dir
            .file_name()
            .with_context(|| format!("Could not get string for filename of {dir:?}"))?
            .to_str()
            .with_context(|| format!("Could not get string for path of filename of {dir:?}"))?;

        let problem_label = dir_path.to_string() + "-a";
        rsync(&dir.join("scaffold"), &output)?;
        repo.add_all()?;
        repo.commit(&problem_label)?;
        repo.create_branch("main", &problem_label)?;

        let solution_label = dir_path.to_string() + "-b";
        rsync(&dir.join("solution"), &output)?;
        repo.add_all()?;
        repo.commit(&solution_label)?;
        repo.create_branch("main", &solution_label)?;
    }
    repo.switch_branch("main")?;
    repo.hard_reset(&initial_commit)?;

    Ok(())
}

pub fn rsync(from: &Path, to: &Path) -> Result<()> {
    Command::new("rsync")
        .arg("-a")
        .arg("--delete")
        .arg("--exclude=.git")
        .arg(from)
        .arg(to)
        .run_with_context(|| format!("Could not rsync files from {from:?} to {to:?}."))
}
