mod commands;
mod dir;
mod test_cmd;
mod util;

use std::{
    fs::{self, DirEntry},
    os::unix::fs::MetadataExt,
    path::{self, Path, PathBuf},
};

use crate::test_cmd::{TestChapterSelection, test_quest};

use anyhow::{Context as _, Result};
use clap::{Parser, ValueEnum};
use env_logger::Env;

/// repo-quest is an authoring tool for RepoQuest quests.
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

const fn after_help_dir() -> &'static str {
    "If the directory representation path is omitted, the nearest parent \
    directory with a `quest.toml` file will be used."
}

const fn after_help_hist() -> &'static str {
    "If the directory representation path is omitted, the nearest parent \
    directory with a `quest.toml` file will be used. \
\
    If the linear-history representation path is omitted, a `hist` directory \
    relative to the directory representation path will be used."
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Initialize a new quest in the given directory.
    ///
    /// The given directory must either be empty or not exist. Creates the
    /// directory if it does not exist, but will not create parent directories.
    #[command(after_help=after_help_dir())]
    Init {
        /// The directory in which to initialize the quest
        quest: PathBuf,
    },
    /// Bundles a quest definition for use with a RepoQuest Forgejo instance.
    #[command(after_help=after_help_dir())]
    Bundle {
        /// The path to the directory format of the quest to bundle.
        #[arg(long)]
        input: Option<PathBuf>,
        /// The path to which to write the bundle archive.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Displays chapter and commit structure of quest.
    #[command(after_help=after_help_dir())]
    Ls {
        /// The path to the directory format of the quest to bundle.
        #[arg(long)]
        quest: Option<PathBuf>,
    },
    /// Converts a quest from directory format to linear history format.
    #[command(after_help=after_help_hist())]
    Hist {
        /// The path to the directory representation of a quest.
        #[arg(long, value_name = "QUEST_REPO_ROOT")]
        quest: Option<PathBuf>,
        /// The output directory.
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
    #[command(after_help=after_help_hist())]
    Dirs {
        /// The path to the linear history representation of a quest.
        #[arg(long)]
        hist: Option<PathBuf>,
        /// The path to the directory representation of a quest.
        #[arg(long)]
        quest: Option<PathBuf>,
    },
    /// Checks that the directory format of a quest is well-formed.
    #[command(after_help=after_help_dir())]
    Check {
        #[arg(long)]
        /// The path to the quest.
        quest: Option<PathBuf>,
    },
    // CommitHist {
    //     /// The path to the linear history representation of a quest.
    //     ///
    //     /// If omitted, uses the nearest parent directory that contains `.git`.
    //     #[arg(long)]
    //     hist: Option<PathBuf>,
    //     /// The path to the directory representation of a quest.
    //     ///
    //     /// If omitted, uses the nearest parent directory that contains
    //     /// `quest.toml` file.
    //     #[arg(long)]
    //     dir: Option<PathBuf>,
    //     #[arg(long, short)]
    //     message: String,
    // },
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
    /// - Make a change that will need to be propagated forward. (Since it can
    ///   only be propagated forward, make the change to the earliest quest stage
    ///   that needs it.)
    /// - Commit the change.
    /// - Use this command to produce a git repository representing the quest
    ///   stages and set up the needed rebase.
    /// - Complete the rebase.
    /// - Use the `overlay` command to update the working directory of the
    ///   quest definition repository.
    /// - Amend the commit with the forward-propagated changes.
    #[command(after_help=after_help_hist())]
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
    /// Runs the test script configured in as test-cmd in quest.toml for each
    /// commit.
    #[command(after_help=after_help_dir())]
    Test {
        /// The path to the directory representation of a quest.
        ///
        /// If omitted, uses the nearest parent directory that contains a
        /// `quest.toml` file.
        #[arg(long, value_name = "QUEST_REPO_ROOT")]
        quest: Option<PathBuf>,
        /// Skip running the tests on the scaffold commits.
        ///
        /// You can also specify specifically which commits are expected to fail
        /// in quest.toml.
        #[arg(long)]
        skip_scaffold: bool,
        /// Test only the commits for the given chapter.
        ///
        /// Specifying "main" will select the pre-chapter commits in the main
        /// directory even if a chapter named "main" exists.
        #[arg(long, value_name = "CHAPTER")]
        chapter: Option<String>,
        /// Test only the commits for the given chapter and following chapters.
        ///
        /// Specifying the first chapter enables skipping main.
        ///
        /// Specifying "main" will select the a chapter named "main". To run
        /// main and all following chapters, omit the chapter selection
        /// entirely.
        #[arg(long, value_name = "CHAPTER", conflicts_with = "chapter")]
        following_chapters: Option<String>,
    },
}

fn main() -> Result<()> {
    let Args { command } = Args::parse();

    #[cfg(not(debug_assertions))]
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    const QUEST_BRANCH_PREFIX: &str = "quest";
    match command {
        Command::Init { quest } => commands::init(&quest)?,
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
            let quest_tree = commands::quest_tree(&quest)?;
            println!("{quest_tree}");
        }
        Command::Hist { quest, hist } => {
            let dir = match quest {
                Some(dir) => dir,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let hist = match hist {
                Some(hist) => hist,
                None => dir.join("hist"),
            };
            commands::dir_to_hist(&dir, hist, QUEST_BRANCH_PREFIX)?;
        }
        Command::Dirs { quest, hist } => {
            let dir = match quest {
                Some(dir) => dir,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let hist = match hist {
                Some(hist) => hist,
                None => dir.join("hist"),
            };
            commands::overlay(hist, &dir, QUEST_BRANCH_PREFIX)?
        }
        Command::Check { quest } => {
            let quest = match quest {
                Some(quest) => quest,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let _ = dir::parse(&quest)?;
        }
        Command::Propagate {
            quest,
            original,
            changed,
            output,
        } => {
            let rebase_todo = commands::prepare_propagate_repo(
                &quest,
                &original,
                &changed,
                path::absolute(output)?,
            )?;
            println!("{rebase_todo}");
        }
        Command::Test {
            quest,
            skip_scaffold,
            chapter,
            following_chapters,
        } => {
            let dir = match quest {
                Some(dir) => dir,
                None => infer_dir_path(&PathBuf::from("."))?
                    .context("Could not determine quest dir path.")?,
            };
            let chapter_selection = chapter
                .map(TestChapterSelection::OneChapter)
                .or_else(|| following_chapters.map(TestChapterSelection::FollowingChapters))
                .unwrap_or(TestChapterSelection::AllChapters);
            test_quest(&dir, skip_scaffold, chapter_selection)?;
        }
    };

    Ok(())
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
