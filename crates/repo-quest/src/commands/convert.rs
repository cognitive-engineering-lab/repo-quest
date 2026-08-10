use std::{
    collections::HashMap,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail, ensure};
use itertools::{EitherOrBoth, Itertools};
use log::{debug, info, warn};
use repo_quest_core::{BOT_AUTHOR, git::GitRepo};
use tempfile::TempDir;

use crate::{
    dir::{
        Chapter, ChapterMeta, Commit, CommitMeta, Issue, PullRequest, QuestDefinition, QuestMeta,
        TestExpectation, parse,
    },
    util::rsync,
};
use repo_quest_core::{fs, git::todo::GitTodoList};

const OLD_BRANCH_PREFIX: &str = "quest";
const NEW_BRANCH_PREFIX: &str = "changes";
const QUEST_FILES: [&str; 3] = ["main", "chapters", "quest.toml"];

/// Converts two committed versions of a quest definition into a repository that
/// can be used to propagate changes to one chapter of the quest forward into
/// later chapters via a git rebase. Also produces the necessary git rebase
/// todo-list for doing the propagation.
///
/// The basic branch structure looks like
/// - quest/main/00-foo
/// - quest/main/01-bar
/// - quest/foo/scaffold/00-baz
/// - quest/foo/solution/00-something
///
/// Chapters with changes will result in additional branches under "changes" instead
/// of "quest".
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
    super::ensure_empty_dir(&output_dir)?;
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

    check_chapter_compatibility(
        old_source_dir.path(),
        &old_quest_commits,
        new_source_dir.path(),
        &new_quest_commits,
    )?;

    // Build the basic repo out of the "original" version of the quest.
    dirs_to_repo(&old_quest_commits, &rebase_repo, OLD_BRANCH_PREFIX)?;

    // Augment the repo with the chapters that have changes and produce the
    // git rebase todo-list for propagating the changes.
    let todo = dirs_to_change_branches(&new_quest_commits, &rebase_repo)?;

    // Move the current branch back to main.
    rebase_repo.switch_branch("main")?;

    Ok(todo)
}

/// Checks to make sure that two quests have the same chapter structure.
fn check_chapter_compatibility(
    old_source_dir: &Path,
    old_quest: &QuestDefinition,
    new_source_dir: &Path,
    new_quest: &QuestDefinition,
) -> Result<()> {
    check_commits_aligned(
        old_source_dir,
        &old_quest.main,
        new_source_dir,
        &new_quest.main,
    )?;
    for chapter in old_quest.chapters.iter().zip_longest(&new_quest.chapters) {
        match chapter {
            EitherOrBoth::Both(
                Chapter {
                    label: old_label,
                    scaffold: old_scaffold,
                    solution: old_solution,
                    ..
                },
                Chapter {
                    label: new_label,
                    scaffold: new_scaffold,
                    solution: new_solution,
                    ..
                },
            ) => {
                ensure!(
                    old_label >= new_label,
                    "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal has a {old_label} chapter, changed does not."
                );
                ensure!(
                    old_label <= new_label,
                    "Propagate does not work with differing commit structures, only different commit content.\n\nChanged has a {new_label} chapter, original does not."
                );
                check_optional_commit_dirs_aligned(
                    old_source_dir,
                    old_scaffold.as_ref(),
                    new_source_dir,
                    new_scaffold.as_ref(),
                    old_label,
                )?;
                check_commits_aligned(old_source_dir, old_solution, new_source_dir, new_solution)?;
            }
            EitherOrBoth::Left(Chapter {
                label: old_label, ..
            }) => bail!(
                "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal has a {old_label} chapter, changed does not."
            ),
            EitherOrBoth::Right(Chapter {
                label: new_label, ..
            }) => bail!(
                "Propagate does not work with differing commit structures, only different commit content.\n\nChanged has a {new_label} chapter, original does not."
            ),
        }
    }

    Ok(())
}

fn check_optional_commit_dirs_aligned(
    old_source_dir: &Path,
    old_commits: Option<&Vec<Commit>>,
    new_source_dir: &Path,
    new_commits: Option<&Vec<Commit>>,
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
                ensure!(
                    old_path >= new_path,
                    "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal version has commit `{}` which changed version does not.",
                    old_path.display()
                );
                ensure!(
                    old_path <= new_path,
                    "Propagate does not work with differing commit structures, only different commit content.\n\nChanged version has commit `{}` which old version does not.",
                    new_path.display()
                );
                if old_commit.message != new_commit.message {
                    warn!(
                        "Commit messages differ between commits in `{}`. This will not prevent creation of the rebase repository, but commit messages are not updated by overlay.",
                        old_path.display()
                    );
                }
            }
            EitherOrBoth::Left(old_commit) => {
                let old_path = old_commit.path.strip_prefix(old_source_dir)?;
                bail!(
                    "Propagate does not work with differing commit structures, only different commit content.\n\nOriginal version has commit `{}` which changed version does not.",
                    old_path.display()
                );
            }
            EitherOrBoth::Right(new_commit) => {
                let new_path = new_commit.path.strip_prefix(new_source_dir)?;
                bail!(
                    "Propagate does not work with differing commit structures, only different commit content.\n\nChanged version has commit `{}` which original version does not.",
                    new_path.display()
                );
            }
        }
    }

    Ok(())
}

/// Assumes `check_compatibility` succeeded.
///
/// Produces git rebase todo-list
fn dirs_to_change_branches(
    new_quest_commits: &QuestDefinition,
    rebase_repo: &GitRepo,
) -> Result<GitTodoList> {
    let mut todo = GitTodoList::new();
    for (commit_kind, commit) in new_quest_commits.commits_iter() {
        let old_branch_name = commit_kind.branch_name(OLD_BRANCH_PREFIX, &commit.path);
        rebase_repo.switch_branch(&old_branch_name)?;
        let new_branch_name = commit_kind.branch_name(NEW_BRANCH_PREFIX, &commit.path);
        let has_changes = create_commit_if_changed(
            rebase_repo,
            &old_branch_name,
            &new_branch_name,
            commit.message.as_deref(),
            &commit.path,
        )?;
        let old_rev = rebase_repo.rev_parse_short(&old_branch_name)?;
        todo.pick(&old_rev, Some(&old_branch_name));
        if has_changes {
            let new_rev = rebase_repo.rev_parse_short(&new_branch_name)?;
            todo.fixup(&new_rev, Some(&new_branch_name));
        }
        todo.update_branch(&old_branch_name);
    }

    Ok(todo)
}

/// Creates the commits represented by the sequence of directories.
///
/// Each directory's commit has the previous directory's commit as its parent.
fn dirs_to_repo(quest: &QuestDefinition, rebase_repo: &GitRepo, branch_prefix: &str) -> Result<()> {
    for (commit_kind, commit) in quest.commits_iter() {
        debug!("Converting commit {commit_kind:?} {commit:?}.");
        let branch_name = commit_kind.branch_name(branch_prefix, &commit.path);
        create_commit(
            rebase_repo,
            &branch_name,
            commit.message.as_deref(),
            &commit.path,
        )?;
    }

    Ok(())
}

/// Commits to current branch and creates a new branch pointing at commit.
/// Doesn't change branch.
fn create_commit(
    rebase_repo: &GitRepo,
    branch_name: &str,
    message: Option<&str>,
    dir: &Path,
) -> Result<()> {
    info!("Processing `{}`", dir.display());
    rsync(dir, &rebase_repo.dir)?;
    rebase_repo.add_all()?;
    rebase_repo.commit(message.unwrap_or(branch_name), BOT_AUTHOR)?;
    rebase_repo.create_branch("HEAD", branch_name)?;
    Ok(())
}

/// Makes new branch based on old branch and commits to it.
///
/// Ends on new branch.
fn create_commit_if_changed(
    rebase_repo: &GitRepo,
    old_branch_name: &str,
    branch_name: &str,
    message: Option<&str>,
    dir: &Path,
) -> Result<bool> {
    info!("Processing `{}`", dir.display());
    rsync(dir, &rebase_repo.dir)?;
    let root = &[Path::new(".")];
    if rebase_repo.changes(root)?.is_some()
        || rebase_repo.staged_changes(root)?.is_some()
        || !rebase_repo.untracked(root)?.is_empty()
    {
        rebase_repo.add_all()?;
        rebase_repo.create_branch(old_branch_name, branch_name)?;
        rebase_repo.switch_branch(branch_name)?;
        rebase_repo.commit(
            &format!("fixup! {}", message.unwrap_or(branch_name)),
            BOT_AUTHOR,
        )?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn overlay(hist: PathBuf, dir: &Path, branch_prefix: &str) -> Result<()> {
    // 1. make sure there's nothing uncommitted in dir
    let dir_repo = GitRepo::open(dir.to_path_buf()).with_context(|| {
        format!(
            "Cannot overwrite the quest in `{}`: it is not a git repository.",
            dir.display()
        )
    })?;
    let quest_files = &QUEST_FILES.map(Path::new);
    let untracked_files = dir_repo.untracked(quest_files)?;
    let changes = dir_repo.changes(quest_files)?;
    let staged_changes = dir_repo.staged_changes(quest_files)?;
    ensure!(
        changes.is_none() && staged_changes.is_none() && untracked_files.is_empty(),
        "Cannot overwrite quest in `{}`, there are uncommitted changes that would be affected:\n\nChanges:\n{}\n\nStaged changes:\n{}\n\nUntracked files:\n{}",
        dir.display(),
        changes.unwrap_or_default(),
        staged_changes.unwrap_or_default(),
        untracked_files,
    );
    // 2. parse quest from dir
    let original_quest = parse(dir)?;

    // 3. remove chapters and main folders.
    let main_dir = dir.join("main");
    fs::remove_dir_all(&main_dir, "main dir")?;
    fs::create_dir(&main_dir, "main dir")?;
    let all_chapters_dir = dir.join("chapters");
    fs::remove_dir_all(&all_chapters_dir, "chapters dir")?;
    fs::create_dir(&all_chapters_dir, "chapters dir")?;

    // 4a. recreate folders based on hist format
    // 4b. copy back in issues, etc., from matching chapters
    let hist_repo = GitRepo::open(hist)?;
    let branches = hist_repo.topo_branches()?;
    debug!("Creating main commits.");
    let main = {
        let main = dirify_branches(
            &main_dir,
            &hist_repo,
            branches.iter().map(std::string::String::as_str),
            original_quest.main.clone(),
            &format!("{branch_prefix}/main/"),
        )?;
        // create an empty initial commit for main if none is provided by the
        // hist repo
        if main.is_empty() {
            fs::create_dir_all(main_dir.join("initial-commit"), "initial commit dir")?;
            vec![CommitMeta {
                label: "initial-commit".to_string(),
                expected: TestExpectation::Pass,
            }]
        } else {
            main
        }
    };

    let original_chapters: HashMap<_, _> = original_quest
        .chapters
        .iter()
        .map(|chapter| (chapter.label.as_str(), chapter))
        .collect();
    let mut chapters = Vec::new();
    let chapter_branch_prefix = format!("{branch_prefix}/chapter/");
    for (chapter_label, chapter_branches) in &branches
        .iter()
        .filter(|b| b.starts_with(&chapter_branch_prefix))
        .chunk_by(|branch| branch.split('/').dropping(2).next())
    {
        let chapter_branches: Vec<_> = chapter_branches.collect();
        let chapter_meta = process_chapter(
            branch_prefix,
            &all_chapters_dir,
            &hist_repo,
            &original_chapters,
            chapter_label,
            &chapter_branches,
        )?;
        chapters.push(chapter_meta);
    }

    // 6. recreate quest.toml with update chapters/commits

    debug!("Creating quest.toml.");
    // only add the directory to the metadata if the chapter is new or the
    // directory existed before
    let meta = QuestMeta {
        main,
        chapters,
        ..original_quest.meta()
    };

    fs::write(
        dir.join("quest.toml"),
        &toml::ser::to_string_pretty(&meta)
            .with_context(|| format!("Failed to serialize quest metadata {meta:?}."))?,
        "quest metadata",
    )?;

    Ok(())
}

fn process_chapter(
    branch_prefix: &str,
    all_chapters_dir: &Path,
    hist_repo: &GitRepo,
    original_chapters: &HashMap<&str, &Chapter>,
    chapter_label: Option<&str>,
    chapter_branches: &[&String],
) -> Result<ChapterMeta> {
    info!("Processing chapter {chapter_label:?}.");
    let chapter_label = chapter_label.with_context(|| {
        format!("Error getting chapter label from branch names: {chapter_branches:?}.")
    })?;
    let chapter_dir = all_chapters_dir.join(chapter_label);
    fs::create_dir(&chapter_dir, "chapter dir")?;

    // if there is a matching original chapter, preserve the issue/pr
    //
    // TODO: would it be better to just save the files somewhere and copy them back?
    let original_chapter = original_chapters.get(chapter_label);
    if let Some(original_chapter) = original_chapter {
        debug!("Recreating issues.");
        write_issue(&chapter_dir, &original_chapter.issue)?;
        debug!("Recreating prs.");
        write_pr(&chapter_dir, &original_chapter.pull_request)?;
    }

    debug!("Creating scaffold commits for {chapter_label}.");
    let scaffold = dirify_branches(
        &chapter_dir.join("scaffold"),
        hist_repo,
        chapter_branches.iter().map(|s| s.as_str()),
        original_chapter
            .and_then(|chapter| chapter.scaffold.clone())
            .unwrap_or_default(),
        &format!("{branch_prefix}/chapter/{chapter_label}/scaffold/"),
    )?;

    debug!("Creating solution commits for {chapter_label}.");
    let solution = dirify_branches(
        &chapter_dir.join("solution"),
        hist_repo,
        chapter_branches.iter().map(|s| s.as_str()),
        original_chapter.map_or_else(Vec::new, |chapter| chapter.solution.clone()),
        &format!("{branch_prefix}/chapter/{chapter_label}/solution/"),
    )?;

    // only add the directory to the metadata if the chapter is new or the
    // directory existed before
    let scaffold = if let Some(original_chapter) = original_chapters.get(&chapter_label)
        && original_chapter.scaffold.is_none()
        && scaffold.is_empty()
    {
        None
    } else {
        Some(scaffold)
    };

    Ok(ChapterMeta {
        label: chapter_label.to_string(),
        scaffold,
        solution,
    })
}

fn write_pr(chapter_dir: &Path, pull_request: &PullRequest) -> Result<()> {
    let file = chapter_dir.join("pr.md");
    if let Some(primary_issue) = &pull_request.primary_issue {
        write!(
            &fs::File::create_new(file).with_context(|| format!(
                "Failed to create primary issue for `{}`.",
                chapter_dir.display()
            ))?,
            "+++\n{}+++\n{}",
            toml::ser::to_string_pretty(&primary_issue.meta)?,
            primary_issue.content
        )
        .with_context(|| {
            format!(
                "Failed to write primary issue for {}.",
                chapter_dir.display()
            )
        })?;
    }

    if let Some(comments) = &pull_request.comments {
        let issue_comments_dir = chapter_dir.join("pr");
        fs::create_dir_all(&issue_comments_dir, "issue comments dir")?;
        for comment in comments {
            write!(
                &fs::File::create_new(&comment.path).with_context(|| format!(
                    "Failed to create comment `{}`.",
                    comment.path.display()
                ))?,
                "+++\n{}+++\n{}",
                toml::ser::to_string_pretty(&comment.meta)?,
                comment.content
            )
            .with_context(|| format!("Failed to write comment `{}`.", comment.path.display()))?;
        }
    }

    Ok(())
}

fn write_issue(chapter_dir: &Path, issue: &Issue) -> Result<()> {
    let file = chapter_dir.join("issue.md");
    write!(
        &fs::File::create_new(file)?,
        "+++\n{}+++\n{}",
        toml::ser::to_string_pretty(&issue.primary_issue.meta)?,
        issue.primary_issue.content
    )
    .with_context(|| {
        format!(
            "Failed to write primary issue for `{}`.",
            chapter_dir.display()
        )
    })?;

    if let Some(comments) = &issue.comments {
        let issue_comments_dir = chapter_dir.join("issue");
        fs::create_dir_all(&issue_comments_dir, "issue comments dir")?;
        for comment in comments {
            fs::write(&comment.path, &comment.content, "comment")?;
        }
    }

    Ok(())
}

fn dirify_branches<'a>(
    output_dir: &Path,
    repo: &GitRepo,
    branches: impl Iterator<Item = &'a str>,
    original_commits: Vec<Commit>,
    branch_prefix: &str,
) -> Result<Vec<CommitMeta>, anyhow::Error> {
    let mut original_commits: HashMap<String, Commit> = original_commits
        .into_iter()
        .map(|commit| {
            (
                commit
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                commit,
            )
        })
        .collect();
    let mut main = Vec::new();
    for branch in branches {
        if let Some(commit_label) = branch.strip_prefix(branch_prefix) {
            if commit_label.contains('/') {
                bail!("Malformed commit branch name: {branch} contains / in label.");
            }
            fs::create_dir_all(
                output_dir.join(commit_label),
                format!("chapter commit dir for {branch}"),
            )?;
            repo.copy_tree(branch, &output_dir.join(commit_label))?;
            let msg = repo.commit_message(branch)?;
            fs::write(
                output_dir.join(format!("{commit_label}.txt")),
                msg,
                format!("commit message for {branch}"),
            )?;
            // Preserve original metadata for this commit if it exists.
            if let Some(original_commit) = original_commits.remove(commit_label) {
                main.push(original_commit.into_commit_meta());
            } else {
                main.push(CommitMeta {
                    label: commit_label.to_string(),
                    expected: TestExpectation::Pass,
                });
            }
        }
    }

    Ok(main)
}

pub fn dir_to_hist(quest_dir: &Path, output_dir: PathBuf, branch_prefix: &str) -> Result<GitRepo> {
    // Parse out the commits of the quests.
    let quest = parse(quest_dir)?;
    quest_to_hist(&quest, output_dir, branch_prefix)
}

/// Converts the quest definition to a linear-histroy representation.
///
/// Leaves the repository on the "main" branch, which is not part of the
/// linear-history representation.
pub fn quest_to_hist(
    quest: &QuestDefinition,
    output_dir: PathBuf,
    branch_prefix: &str,
) -> Result<GitRepo> {
    super::ensure_empty_dir(&output_dir)?;
    let output_repo = GitRepo::init(output_dir)?;

    debug!("{quest:?}");

    // Build the basic repo out of the "original" version of the quest.
    dirs_to_repo(quest, &output_repo, branch_prefix)?;

    // Move the current branch back to main.
    output_repo.switch_branch("main")?;

    Ok(output_repo)
}
