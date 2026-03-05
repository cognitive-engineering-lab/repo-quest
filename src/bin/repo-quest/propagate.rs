use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use itertools::{EitherOrBoth, Itertools as _};
use log::{info, warn};
use repo_quest::git::GitRepo;
use tempfile::*;

use crate::{dir::*, util::rsync};
use repo_quest::git::todo::*;

const OLD_BRANCH_PREFIX: &str = "old";
const NEW_BRANCH_PREFIX: &str = "new";

/// Converts two committed versions of a quest definition into a repository that
/// can be used to propagate changes to one chapter of the quest forward into
/// later chapters via a git rebase. Also produces the necessary git rebase
/// todo-list for doing the propagation.
///
/// The basic branch structure looks like
/// - old/main/00-foo
/// - old/main/01-bar
/// - old/foo/scaffold/00-baz
/// - old/foo/solution/00-something
///
/// Chapters with changes will result in additional branches under "new" instead
/// of "old".
///
/// The branch `main` is used for working on the tree, not for representing a
/// chapter.
pub fn prepare_propagate_repo(
    quest_dir: &Path,
    original: &str,
    changed: &str,
    output_dir: PathBuf,
) -> Result<GitTodoList> {
    let quest_repo = GitRepo::open(quest_dir.to_path_buf())?;

    // Initialize the repository that will host the rebase.
    ensure_empty_dir(&output_dir)?;
    let rebase_repo = GitRepo::init(output_dir)?;

    // Set up tempdirs for copying out the specified versions of the quest definition.
    let old_source_dir = TempDir::new()
        .context("Could not create directory for extracting old quest definition versions.")?;
    let new_source_dir = TempDir::new()
        .context("Could not create directory for extracting new quest definition versions.")?;

    // Copy the specified versions of the quest definition
    quest_repo.copy_tree(original, old_source_dir.path())?;
    quest_repo.copy_tree(changed, new_source_dir.path())?;

    // Parse out the commits of the quests.
    let old_quest_commits = parse(old_source_dir.path())?;
    let new_quest_commits = parse(new_source_dir.path())?;
    info!("{old_quest_commits:?}");
    info!("{new_quest_commits:?}");

    check_compatibility(
        old_source_dir.path(),
        &old_quest_commits,
        new_source_dir.path(),
        &new_quest_commits,
    )?;

    // Build the basic repo out of the "original" version of the quest.
    dirs_to_repo(old_quest_commits, &rebase_repo)?;

    // Augment the repo with the chapters that have changes and produce the
    // git rebase todo-list for propagating the changes.
    let todo = dirs_to_change_branches(new_quest_commits, &rebase_repo)?;

    // Move the current branch back to main.
    rebase_repo.switch_branch("main")?;

    Ok(todo)
}

/// Creates the output dir if it does not exist. Fails with `Err` if the output
/// dir exists but is not empty.
fn ensure_empty_dir(output_dir: &Path) -> Result<()> {
    if !output_dir.exists() {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("Could not create output directory {output_dir:?}."))?;
    } else if !output_dir.is_dir()
        || output_dir
            .read_dir()
            .with_context(|| format!("Cannot read output directory {output_dir:?}."))?
            .next()
            .is_some()
    {
        bail!("Given output output path exists but is not an empty directory.")
    }
    Ok(())
}

/// Checks to make sure that two quests have the same chapter structure.
fn check_compatibility(
    old_source_dir: &Path,
    old_quest_commits: &QuestDefinition,
    new_source_dir: &Path,
    new_quest_commits: &QuestDefinition,
) -> Result<()> {
    let dirname = "main";
    check_optional_commit_dirs_aligned(
        old_source_dir,
        &old_quest_commits.main,
        new_source_dir,
        &new_quest_commits.main,
        dirname,
    )?;
    for chapter in old_quest_commits
        .chapters
        .iter()
        .zip_longest(&new_quest_commits.chapters)
    {
        match chapter {
            EitherOrBoth::Both(
                Chapter {
                    branch_name: old_branch_name,
                    scaffold: old_scaffold,
                    solution: old_solution,
                    ..
                },
                Chapter {
                    branch_name: new_branch_name,
                    scaffold: new_scaffold,
                    solution: new_solution,
                    ..
                },
            ) => {
                if old_branch_name < new_branch_name {
                    bail!(
                        "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal has a {old_branch_name} branch, changed does not."
                    );
                } else if old_branch_name > new_branch_name {
                    bail!(
                        "Propagate does not work with differing commit structures, only different commit content.\n\nChanged has a {new_branch_name} branch, original does not."
                    )
                } else {
                    check_optional_commit_dirs_aligned(
                        old_source_dir,
                        old_scaffold,
                        new_source_dir,
                        new_scaffold,
                        dirname,
                    )?;
                    check_commits_aligned(
                        old_source_dir,
                        old_solution,
                        new_source_dir,
                        new_solution,
                    )?;
                }
            }
            EitherOrBoth::Left(Chapter {
                branch_name: old_branch_name,
                ..
            }) => bail!(
                "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal has a {old_branch_name} branch, changed does not."
            ),
            EitherOrBoth::Right(Chapter {
                branch_name: new_branch_name,
                ..
            }) => bail!(
                "Propagate does not work with differing commit structures, only different commit content.\n\nChanged has a {new_branch_name} branch, original does not."
            ),
        }
    }

    Ok(())
}

fn check_optional_commit_dirs_aligned(
    old_source_dir: &Path,
    old_commits: &Option<Vec<Commit>>,
    new_source_dir: &Path,
    new_commits: &Option<Vec<Commit>>,
    dirname: &str,
) -> Result<(), anyhow::Error> {
    for main_dirs in old_commits.iter().zip_longest(new_commits) {
        match main_dirs {
            EitherOrBoth::Both(old_main_commits, new_main_commits) => {
                check_commits_aligned(
                    old_source_dir,
                    old_main_commits,
                    new_source_dir,
                    new_main_commits,
                )?;
            }
            EitherOrBoth::Left(_) => bail!(
                "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal has a {dirname} directory, changed does not."
            ),
            EitherOrBoth::Right(_) => bail!(
                "Propagate does not work with differing commit structures, only different commit content.\n\nChanged has a {dirname} directory, original does not."
            ),
        }
    }

    Ok(())
}

fn check_commits_aligned(
    old_source_dir: &Path,
    old_main_commits: &[Commit],
    new_source_dir: &Path,
    new_main_commits: &[Commit],
) -> Result<()> {
    for commits in old_main_commits.iter().zip_longest(new_main_commits) {
        match commits {
            EitherOrBoth::Both(old_commit, new_commit) => {
                let old_path = old_commit.path.strip_prefix(old_source_dir)?;
                let new_path = new_commit.path.strip_prefix(new_source_dir)?;
                if old_path < new_path {
                    bail!(
                        "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal version has commit {old_path:?} which changed version does not.",
                    );
                } else if old_path > new_path {
                    bail!(
                        "Propagate does not work with differing commit structures, only different commit content.\n\nChanged version has commit {new_path:?} which old version does not.",
                    );
                } else if old_commit.message != new_commit.message {
                    warn!(
                        "Commit messages differ between commits in {old_path:?}. This will not prevent creation of the rebase repository, but commit messages are not updated by overlay.",
                    );
                }
            }
            EitherOrBoth::Left(old_commit) => {
                let old_path = old_commit.path.strip_prefix(old_source_dir)?;
                bail!(
                    "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal version has commit {old_path:?} which changed version does not.",
                );
            }
            EitherOrBoth::Right(new_commit) => {
                let new_path = new_commit.path.strip_prefix(new_source_dir)?;
                bail!(
                    "Propagate does not work with differing commit structures, only different commit content.\n\nChanged version has commit {new_path:?} which original version does not.",
                );
            }
        }
    }

    Ok(())
}

/// Assumes check_compatibility succeeded.
///
/// Produces git rebase todo-list
fn dirs_to_change_branches(
    new_quest_commits: QuestDefinition,
    rebase_repo: &GitRepo,
) -> Result<GitTodoList> {
    let mut todo = GitTodoList::new();
    for Commit { path, message } in new_quest_commits.main.into_iter().flatten() {
        let old_branch_name = gen_branch_name(&format!("{OLD_BRANCH_PREFIX}/main"), &path);
        rebase_repo.switch_branch(&old_branch_name)?;
        let new_branch_name = gen_branch_name(&format!("{NEW_BRANCH_PREFIX}/main"), &path);
        let has_changes = create_commit_if_changed(
            rebase_repo,
            &old_branch_name,
            &new_branch_name,
            message,
            &path,
        )?;
        let old_rev = rebase_repo.rev_parse_short(&old_branch_name)?;
        todo.pick(&old_rev, Some(&old_branch_name));
        if has_changes {
            let new_rev = rebase_repo.rev_parse_short(&new_branch_name)?;
            todo.fixup(&new_rev, Some(&new_branch_name));
        }
        todo.update_branch(&old_branch_name);
    }
    for Chapter {
        branch_name,
        scaffold,
        solution,
        ..
    } in new_quest_commits.chapters
    {
        for Commit { path, message } in scaffold.into_iter().flatten() {
            let old_branch_name = gen_branch_name(
                &format!("{OLD_BRANCH_PREFIX}/{branch_name}/scaffold"),
                &path,
            );
            rebase_repo.switch_branch(&old_branch_name)?;
            let new_branch_name = gen_branch_name(
                &format!("{NEW_BRANCH_PREFIX}/{branch_name}/scaffold"),
                &path,
            );
            let has_changes = create_commit_if_changed(
                rebase_repo,
                &old_branch_name,
                &new_branch_name,
                message,
                &path,
            )?;
            let old_rev = rebase_repo.rev_parse_short(&old_branch_name)?;
            todo.pick(&old_rev, Some(&old_branch_name));
            if has_changes {
                let new_rev = rebase_repo.rev_parse_short(&new_branch_name)?;
                todo.fixup(&new_rev, Some(&new_branch_name));
            }
            todo.update_branch(&old_branch_name);
        }
        for Commit { path, message } in solution {
            let old_branch_name = gen_branch_name(
                &format!("{OLD_BRANCH_PREFIX}/{branch_name}/solution"),
                &path,
            );
            rebase_repo.switch_branch(&old_branch_name)?;
            let new_branch_name = gen_branch_name(
                &format!("{NEW_BRANCH_PREFIX}/{branch_name}/solution"),
                &path,
            );
            let has_changes = create_commit_if_changed(
                rebase_repo,
                &old_branch_name,
                &new_branch_name,
                message,
                &path,
            )?;
            let old_rev = rebase_repo.rev_parse_short(&old_branch_name)?;
            todo.pick(&old_rev, Some(&old_branch_name));
            if has_changes {
                let new_rev = rebase_repo.rev_parse_short(&new_branch_name)?;
                todo.fixup(&new_rev, Some(&new_branch_name));
            }
            todo.update_branch(&old_branch_name);
        }
    }

    Ok(todo)
}

/// Creates the commits represented by the sequence of directories.
///
/// Each directory's commit has the previous directory's commit as its parent.
fn dirs_to_repo(quest_commits: QuestDefinition, rebase_repo: &GitRepo) -> Result<()> {
    let QuestDefinition { main, chapters, .. } = quest_commits;

    if let Some(main) = main {
        for Commit { path, message } in main {
            let branch_name = gen_branch_name("old/main", &path);
            create_commit(rebase_repo, &branch_name, message, &path)?;
        }
    }

    for Chapter {
        branch_name,
        scaffold,
        solution,
        ..
    } in chapters
    {
        if let Some(scaffold) = scaffold {
            let mut scaffold_branches = Vec::with_capacity(scaffold.len());
            for Commit { path, message } in scaffold {
                let branch_name = gen_branch_name(
                    &format!("{OLD_BRANCH_PREFIX}/{branch_name}/scaffold"),
                    &path,
                );
                create_commit(rebase_repo, &branch_name, message, &path)?;
                scaffold_branches.push(branch_name);
            }
        }

        for Commit { path, message } in solution {
            let branch_name = gen_branch_name(
                &format!("{OLD_BRANCH_PREFIX}/{branch_name}/solution"),
                &path,
            );
            create_commit(rebase_repo, &branch_name, message, &path)?;
        }
    }

    Ok(())
}

/// Commits to current branch and creates a new branch pointing at commit.
/// Doesn't change branch.
fn create_commit(
    rebase_repo: &GitRepo,
    branch_name: &str,
    message: Option<String>,
    dir: &Path,
) -> Result<()> {
    info!("Processing {:?}", dir);
    rsync(dir, &rebase_repo.dir)?;
    rebase_repo.add_all()?;
    rebase_repo.commit(message.as_deref().unwrap_or(branch_name))?;
    rebase_repo.create_branch("HEAD", branch_name)?;
    Ok(())
}

/// Generate the branch name that corresponds to a chapter directory in a quest.
///
/// The branch name is prefixed with the section name.
///
/// ```
/// assert_eq!(gen_branch_name("foo/bar", "/tmp/quest/chapter/scaffold/01-baz"), "foo/bar/01-baz")
/// ```
fn gen_branch_name(section_name: &str, dir: &Path) -> String {
    let branch_name = format!(
        "{}/{}",
        section_name,
        &dir.file_name().unwrap().to_string_lossy()
    );
    branch_name
}

/// Makes new branch based on old branch and commits to it.
///
/// Ends on new branch.
fn create_commit_if_changed(
    rebase_repo: &GitRepo,
    old_branch_name: &str,
    branch_name: &str,
    message: Option<String>,
    dir: &Path,
) -> Result<bool> {
    info!("Processing {:?}", dir);
    rsync(dir, &rebase_repo.dir)?;
    if rebase_repo.has_changes()? {
        rebase_repo.add_all()?;
        rebase_repo.create_branch(old_branch_name, branch_name)?;
        rebase_repo.switch_branch(branch_name)?;
        rebase_repo.commit(&format!(
            "fixup! {}",
            message.as_deref().unwrap_or(branch_name)
        ))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn overlay(rebase_repo: &Path, quest: &Path) -> Result<()> {
    todo!()
}
