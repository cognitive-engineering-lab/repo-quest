use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use itertools::{EitherOrBoth, Itertools as _};
use log::{info, warn};
use repo_quest::git::GitRepo;
use tempfile::*;

use crate::{dir::*, util::rsync};

const OLD_BRANCH_PREFIX: &str = "old";
const NEW_BRANCH_PREFIX: &str = "new";

/// Converts..
///
/// Branch structure looks like
/// - old/main/baz
/// - old/foo/scaffold/bar
/// - old/foo/solution/baz
/// - new/main-primary
/// - new/main/baz
/// - new/foo/scaffold/bar
/// - new/foo/solution/baz
///
/// The branch main is used for working on the tree, not for representing a
/// chapter.
pub fn propagate(quest_dir: &Path, original: &str, changed: &str) -> Result<()> {
    let old_source_dir = TempDir::new()
        .context("Could not create directory for extracting old quest definition versions.")?;
    let new_source_dir = TempDir::new()
        .context("Could not create directory for extracting new quest definition versions.")?;
    let working_dir = TempDir::new()
        .context("Could not create directory for temporary rebase repository.")?
        .keep();
    let quest_repo = GitRepo::open(quest_dir.to_path_buf())?;
    let rebase_repo = GitRepo::init(working_dir.clone())?;

    quest_repo.copy_tree(original, old_source_dir.path())?;
    quest_repo.copy_tree(changed, new_source_dir.path())?;

    let old_quest_commits = parse_quest_commits(old_source_dir.path())?;
    let new_quest_commits = parse_quest_commits(new_source_dir.path())?;
    info!("{old_quest_commits:?}");
    info!("{new_quest_commits:?}");

    check_compatibility(
        old_source_dir.path(),
        &old_quest_commits,
        new_source_dir.path(),
        &new_quest_commits,
    )?;

    dirs_to_repo(old_quest_commits, &rebase_repo)?;

    let todo = dirs_to_change_branches(new_quest_commits, &rebase_repo)?;

    rebase_repo.switch_branch("main")?;

    for entry in todo {
        println!("{entry}");
    }

    Ok(())
}

fn check_compatibility(
    old_source_dir: &Path,
    old_quest_commits: &QuestCommits,
    new_source_dir: &Path,
    new_quest_commits: &QuestCommits,
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
                ChapterCommits {
                    branch_name: old_branch_name,
                    scaffold: old_scaffold,
                    solution: old_solution,
                },
                ChapterCommits {
                    branch_name: new_branch_name,
                    scaffold: new_scaffold,
                    solution: new_solution,
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
            EitherOrBoth::Left(ChapterCommits {
                branch_name: old_branch_name,
                ..
            }) => bail!(
                "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal has a {old_branch_name} branch, changed does not."
            ),
            EitherOrBoth::Right(ChapterCommits {
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
/// Produces git-todo-list
fn dirs_to_change_branches(
    new_quest_commits: QuestCommits,
    rebase_repo: &GitRepo,
) -> Result<Vec<String>> {
    let mut todo = Vec::new();
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
        let old_rev = rebase_repo.rev_parse(&old_branch_name)?;
        todo.push(format!("pick {old_rev} # {old_branch_name}"));
        if has_changes {
            let new_rev = rebase_repo.rev_parse(&new_branch_name)?;
            todo.push(format!("fixup {new_rev} # {new_branch_name}"));
        }
        todo.push(format!("update-ref refs/heads/{old_branch_name}"));
    }
    for ChapterCommits {
        branch_name,
        scaffold,
        solution,
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
            let old_rev = rebase_repo.rev_parse(&old_branch_name)?;
            todo.push(format!("pick {old_rev} # {old_branch_name}"));
            if has_changes {
                let new_rev = rebase_repo.rev_parse(&new_branch_name)?;
                todo.push(format!("fixup {new_rev} # {new_branch_name}"));
            }
            todo.push(format!("update-ref refs/heads/{old_branch_name}"));
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
            let old_rev = rebase_repo.rev_parse(&old_branch_name)?;
            todo.push(format!("pick {old_rev} # {old_branch_name}"));
            if has_changes {
                let new_rev = rebase_repo.rev_parse(&new_branch_name)?;
                todo.push(format!("fixup {new_rev} # {new_branch_name}"));
            }
            todo.push(format!("update-ref refs/heads/{old_branch_name}"));
        }
    }
    Ok(todo)
}

/// Creates the commits represented by the sequence of directories.
///
/// Each directory's commit has the previous directory's commit as its parent.
fn dirs_to_repo(quest_commits: QuestCommits, rebase_repo: &GitRepo) -> Result<()> {
    let QuestCommits { main, chapters } = quest_commits;

    if let Some(main) = main {
        for Commit { path, message } in main {
            let branch_name = gen_branch_name("old/main", &path);
            create_commit(rebase_repo, &branch_name, message, &path)?;
        }
    }

    for ChapterCommits {
        branch_name,
        scaffold,
        solution,
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
