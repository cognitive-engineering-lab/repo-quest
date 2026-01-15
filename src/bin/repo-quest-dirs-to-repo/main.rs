use anyhow::Context as _;
use clap::Parser;
use env_logger::Env;
use repo_quest::{
    command::RunCommand as _,
    git::GitRepo,
    github::meta::{Chapter, QuestConfig},
};
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
    #[arg(short, long, default_value = "Tutorial Name")]
    title: String,
    #[arg(short, long, default_value = "Author Name")]
    author: String,
}

type Result<A> = anyhow::Result<A>;

#[tokio::main]
async fn main() -> Result<()> {
    let Args {
        input,
        output,
        title,
        author,
    } = Args::parse();

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

    let mut chapters = Vec::new();

    repo.commit("Initial commit")?;
    let initial_commit = repo.rev_parse("HEAD")?;
    for dir in dirs {
        let dir_name = dir
            .file_name()
            .with_context(|| format!("Could not get string for filename of {dir:?}"))?
            .to_str()
            .with_context(|| format!("Could not get string for path of filename of {dir:?}"))?;

        let problem_label = dir_name.to_string() + "-a";
        let scaffold_dir = &dir.join("scaffold");
        if scaffold_dir.is_dir() {
            rsync(scaffold_dir, &output)?;
            repo.add_all()?;
        }
        repo.commit(&problem_label)?;
        repo.create_branch("main", &problem_label)?;

        let solution_label = dir_name.to_string() + "-b";
        rsync(&dir.join("solution"), &output)?;
        repo.add_all()?;
        repo.commit(&solution_label)?;
        repo.create_branch("main", &solution_label)?;

        let chapter = Chapter {
            label: dir_name.to_string(),
            name: dir_name.to_string() + " name",
            no_starter: false,
        };
        chapters.push(chapter);
    }
    repo.switch_branch("main")?;
    repo.hard_reset(&initial_commit)?;

    let qc = QuestConfig {
        title,
        author,
        repo: "".to_string(),
        chapters,
        read_only: None,
        r#final: None,
        final_url: None,
        rq_version: "0.3.0".to_string(),
    };
    repo.switch_orphan_branch("meta")?;
    let qc_toml = toml::ser::to_string(&qc)
        .with_context(|| format!("Could not serialize QuestConfig {qc:?}"))?;
    std::fs::write(output.join("meta.toml"), qc_toml)?;
    repo.add_all()?;
    repo.commit("Initial commit of quest metadata")?;

    repo.switch_branch("main")?;

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
