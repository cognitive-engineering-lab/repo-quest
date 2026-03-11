mod dir;
mod github;
mod propagate;
mod util;

use std::{
    borrow::Cow,
    fs::{self, DirEntry},
    os::unix::fs::MetadataExt,
    path::{self, Path, PathBuf},
};

use crate::{dir::QuestDefinition, github::*, propagate::dir_to_hist};

use anyhow::{Context as _, Result};
use clap::{Parser, ValueEnum};
use env_logger::Env;
use termtree::Tree;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum QuestFormat {
    Dir,
    Hist,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Initialize a new quest in the given directory.
    ///
    /// The given directory must either be empty or not exist. Creates the
    /// directory if it does not exist, but will not create parent directories.
    Init {
        /// The directory in which to initialize the quest
        dir: PathBuf,
    },
    /// Bundles a quest definition for use with a RepoQuest Forgejo instance.
    Bundle {
        /// The path to the directory format of the quest to bundle.
        #[arg(long)]
        input: Option<PathBuf>,
        /// The path to which to write the bundle archive.
        #[arg(short, long)]
        output: PathBuf,
    },
    Ls {
        /// The path to the directory format of the quest to bundle.
        #[arg(long)]
        quest: Option<PathBuf>,
    },
    /// Converts a quest from directory format to linear history format.
    DirToHist {
        /// The path to the directory representation of a quest.
        ///
        /// If omitted, uses the nearest parent directory that contains a
        /// `quest.toml` file.
        #[arg(long, value_name = "QUEST_REPO_ROOT")]
        dir: Option<PathBuf>,
        /// The output directory. If omitted, uses a `hist` directory relative
        /// to the quest directory.
        ///
        /// Will be created if it does not exist. If it does exist, it will be
        /// overwritten.
        #[arg(long, value_name = "OUTPUT_DIR")]
        hist: Option<PathBuf>,
    },
    /// Overlay branches from a converted repository back onto the collection of
    /// directories in a quest definition.
    ///
    /// Will only overlay on a git repository with no uncommitted changes.
    ///
    /// See the propagate command for more information.
    HistToDir {
        /// The path to the linear history representation of a quest.
        ///
        /// If omitted, uses the nearest parent directory that contains `.git`.
        #[arg(long)]
        hist: Option<PathBuf>,
        /// The path to the directory representation of a quest.
        ///
        /// If omitted, uses the nearest parent directory that contains
        /// `quest.toml` file.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Checks that the directory format of a quest is well-formed.
    Check {
        #[arg(long)]
        /// The path to the quest.
        ///
        /// If omitted uses the nearest parent directory with a `quest.toml`
        /// file.
        quest: Option<PathBuf>,
    },
    CommitHist {
        /// The path to the linear history representation of a quest.
        ///
        /// If omitted, uses the nearest parent directory that contains `.git`.
        #[arg(long)]
        hist: Option<PathBuf>,
        /// The path to the directory representation of a quest.
        ///
        /// If omitted, uses the nearest parent directory that contains
        /// `quest.toml` file.
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long, short)]
        message: String,
    },
    /// Converts a quest definition from a collection of directories to a git
    /// repository, and starts a git rebase operation to propagating changes
    /// from one directory to later directories. After the rebase is complete,
    /// use the `overlay` command to convert the repository back.
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
        #[arg(long, value_name = "QUEST_REPO_ROOT")]
        quest: PathBuf,
        /// A git ref for the baseline quest definition. (Often `HEAD^`.)
        #[arg(long, value_name = "GIT_REF")]
        original: String,
        /// A git ref for the quest definition with the change needing
        /// propagation. (Often `HEAD`.)
        #[arg(long, value_name = "GIT_REF")]
        changed: String,
        /// The output directory. Will be created if it does not exist. If it
        /// does exist, it must be empty.
        #[arg(long, value_name = "OUTPUT_DIR")]
        output: PathBuf,
    },
    // Rename {},
    // Split {},
    // Merge {},
    /// Bundles a GitHub-based quest definition for use with a RepoQuest Forgejo
    /// instance.
    ///
    /// WARNING: This command is deprecated. Convert your quest to the
    /// directory-based format.
    #[deprecated]
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args { command } = Args::parse();

    #[cfg(not(debug_assertions))]
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    match command {
        Command::Init { dir } => todo!(),
        Command::Bundle { input, output } => {
            let input = match input {
                Some(input) => input,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let quest = dir::parse(&input)?;
            dir::bundle(quest, &output)?;
        }
        Command::Ls { quest } => {
            let quest = match quest {
                Some(quest) => quest,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let quest = dir::parse(&quest)?;
            let quest_tree = quest_tree(&quest)?;
            println!("{quest_tree}");
        }
        Command::DirToHist { dir, hist } => {
            let dir = match dir {
                Some(dir) => dir,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let hist = match hist {
                Some(hist) => hist,
                None => dir.join("hist"),
            };
            dir_to_hist(&dir, hist)?;
        }
        Command::HistToDir { dir, hist } => {
            let dir = match dir {
                Some(dir) => dir,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let hist = match hist {
                Some(hist) => hist,
                None => infer_hist_path(&PathBuf::from("."))?
                    .context("Could not determine quest hist path.")?,
            };
            propagate::overlay(hist, &dir)?
        }
        Command::Check { quest } => {
            let quest = match quest {
                Some(quest) => quest,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let _ = dir::parse(&quest)?;
        }
        Command::CommitHist { hist, dir, message } => todo!(),
        Command::Propagate {
            quest,
            original,
            changed,
            output,
        } => {
            let rebase_todo = propagate::prepare_propagate_repo(
                &quest,
                &original,
                &changed,
                path::absolute(output)?,
            )?;
            println!("{rebase_todo}");
        }
        Command::BundleGitHub {
            token,
            base_uri,
            owner,
            repo,
            output,
        } => bundle_github(output, token, base_uri, owner, repo).await?,
    };

    Ok(())
}

fn quest_tree(quest: &'_ QuestDefinition) -> Result<Tree<Cow<'_, str>>> {
    let mut quest_tree = Tree::new(Cow::Borrowed(quest.title.as_str()));
    if let Some(main) = &quest.main {
        let mut main_tree = Tree::new(Cow::Borrowed("main"));
        for commit in main {
            let leaf = Tree::new(commit.path.file_name().unwrap().to_string_lossy());
            main_tree.push(leaf);
        }
        quest_tree.push(main_tree);
    }

    for chapter in &quest.chapters {
        let mut chapter_tree = Tree::new(Cow::Borrowed(chapter.branch_name.as_str()));
        if let Some(scaffold) = &chapter.scaffold {
            let mut scaffold_tree = Tree::new(Cow::Borrowed("scaffold"));
            for commit in scaffold {
                let leaf = Tree::new(commit.path.file_name().unwrap().to_string_lossy());
                scaffold_tree.push(leaf);
            }
            chapter_tree.push(scaffold_tree);
        }

        let mut solution_tree = Tree::new(Cow::Borrowed("solution"));
        for commit in &chapter.solution {
            let leaf = Tree::new(commit.path.file_name().unwrap().to_string_lossy());
            solution_tree.push(leaf);
        }
        chapter_tree.push(solution_tree);
        quest_tree.push(chapter_tree);
    }
    Ok(quest_tree)
}

fn find_parent_dir_containing(
    cur: &Path,
    p: impl Fn(&DirEntry) -> Result<bool>,
) -> Result<Option<PathBuf>> {
    fn find(
        mut cur: PathBuf,
        dev: u64,
        p: impl Fn(&DirEntry) -> Result<bool>,
    ) -> Result<Option<PathBuf>> {
        for entry in cur.read_dir()? {
            if p(&entry?)? {
                return Ok(Some(cur));
            }
        }
        if cur.pop() {
            // stop at filesystem boundary, like git does
            if cur.metadata()?.dev() == dev {
                find(cur, dev, p)
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    // search actual directory structure, like git does
    let cur = fs::canonicalize(cur)?;
    let dev = cur.metadata()?.dev();
    find(cur, dev, p)
}

fn infer_dir_path(cur: &Path) -> Result<Option<PathBuf>> {
    fn p(entry: &DirEntry) -> Result<bool> {
        Ok(entry.file_name() == "quest.toml" && entry.file_type()?.is_file())
    }
    find_parent_dir_containing(cur, p)
}

fn infer_hist_path(cur: &Path) -> Result<Option<PathBuf>> {
    fn p(entry: &DirEntry) -> Result<bool> {
        Ok(entry.file_name() == ".git")
    }
    find_parent_dir_containing(cur, p)
}
