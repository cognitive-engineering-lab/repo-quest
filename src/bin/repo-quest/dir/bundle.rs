use std::{borrow::Cow, collections::HashMap, fs, path::Path, process::Command};

use anyhow::{Context as _, Result};
use log::debug;
use repo_quest::{
    command::RunCommand as _,
    git::GitRepo,
    quest::definition::{
        Comment, IssueTemplate, PullRequestTemplate, QuestDefinitionMetadata, ReviewLineSubject,
        ReviewSubject, TaskTemplate,
    },
    template::Template,
};

use super::*;

/// Converts a `dir::QuestDefinition` into the bundle format on disk.
pub fn bundle(quest: QuestDefinition, output: &Path) -> Result<()> {
    let workdir = tempfile::tempdir().context("Could not create temporary working directory.")?;

    // Create repo dir
    //
    // This repo has a worktree so that we can easily copy in snapshots. We'll
    // convert it to a bare repo later.
    let git_dir_path = workdir.path().to_path_buf().join("repo");
    fs::create_dir_all(&git_dir_path)?;
    let repo = GitRepo::init(git_dir_path.clone())?;

    debug!("Creating initial main branch commits.");
    match quest.main {
        Some(commits) if !commits.is_empty() => {
            create_commits(&git_dir_path, &repo, commits.iter())?;
        }
        _ => repo.commit("Initial commit")?,
    }
    let main_commit = repo.rev_parse("HEAD")?;

    // Bundle each chapter
    let mut task_ids = HashMap::<String, usize>::new();
    let mut tasks = Vec::<TaskTemplate>::new();
    for (
        task_id,
        Chapter {
            branch_name,
            issue,
            pull_request,
            scaffold,
            solution,
        },
    ) in quest.chapters.into_iter().enumerate()
    {
        debug!("Bundling chapter {branch_name}");
        let scaffold_branch_name = format!("{branch_name}-scaffold");
        let solution_branch_name = format!("{branch_name}-solution");

        debug!("Creating scaffold commits");
        // Scaffold commits
        create_commits(&git_dir_path, &repo, scaffold.iter().flatten())?;
        repo.create_branch("main", &scaffold_branch_name)?;

        debug!("Creating solution commits");
        // Solution commits
        create_commits(&git_dir_path, &repo, &solution)?;
        repo.create_branch("main", &solution_branch_name)?;

        let issue_template = bundle_issue(&branch_name, issue);
        let pr_template = bundle_pull_request(&branch_name, pull_request);

        tasks.push(TaskTemplate {
            task_id: branch_name.clone(),
            issue_template,
            pr_template: Some(pr_template),
            scaffolding: scaffold_branch_name.into(),
            reference_solution: solution_branch_name.into(),
        });

        task_ids.insert(branch_name, task_id);
    }

    debug!("Resetting main to main commit");
    repo.hard_reset(&main_commit)?;
    // Convert .git in repo to a bare repo
    repo.make_bare()?;

    // Assemble metadata
    let quest = QuestDefinitionMetadata {
        title: quest.meta.title,
        author: quest.meta.author,
        description: quest.description,
        generated_repo_name: quest.meta.repo,
        tasks,
        task_ids,
    };

    debug!("Writing quest definition metadata to temporary file.");
    // Write quest definition to file
    let quest_json = serde_json::to_string(&quest)
        .with_context(|| format!("Could not serialize quest {quest:?}"))?;
    let quest_json_path = workdir.path().to_path_buf().join("data.json");
    fs::write(&quest_json_path, &quest_json)
        .with_context(|| format!("Could not write quest index to file {quest_json_path:?}"))?;

    debug!("Creating bundle archive.");
    // Create tarball
    let tar_file = fs::File::create(output).context("Could not create output file {output}.")?;
    let gz_file = flate2::write::GzEncoder::new(tar_file, flate2::Compression::default());
    let mut archive = tar::Builder::new(gz_file);
    archive
        .append_path_with_name(&quest_json_path, "data.json")
        .context("Could not add data.json to bundle.")?;
    archive
        .append_dir_all("git", git_dir_path.join(".git"))
        .context("Could not add git repo to bundle.")?;
    archive.finish().context("Could not finalize archive")?;

    Ok(())
}

fn bundle_pull_request(branch_name: &String, pull_request: PullRequest) -> PullRequestTemplate {
    debug!("Creating pull request data.");
    let (pr_title, pr_body) = match pull_request.primary_issue {
        None => (branch_name.to_string(), "".to_string()),
        Some(issue) => (
            match issue.meta {
                None => branch_name.to_string(),
                Some(issue) => issue.title,
            },
            issue.content,
        ),
    };

    PullRequestTemplate {
        title: pr_title,
        body: Template(pr_body),
        comments: pull_request
            .comments
            .into_iter()
            .flatten()
            .map(bundle_pull_request_comment)
            .collect(),
    }
}

fn bundle_pull_request_comment(
    comment: PullRequestComment,
) -> repo_quest::quest::definition::PullRequestComment {
    repo_quest::quest::definition::PullRequestComment {
        quote: comment.meta.map(|m| ReviewSubject {
            commit: None,
            file: m.file,
            old_line: if m.end_line_side == LineSide::Left {
                Some(ReviewLineSubject {
                    start: None,
                    end: m.end_line,
                })
            } else {
                None
            },
            new_line: if m.end_line_side == LineSide::Right {
                Some(ReviewLineSubject {
                    start: None,
                    end: m.end_line,
                })
            } else {
                None
            },
        }),
        body: Template(comment.content),
    }
}

fn bundle_issue(branch_name: &String, issue: Issue) -> IssueTemplate {
    debug!("Creating issue data.");
    IssueTemplate {
        title: issue
            .primary_issue
            .meta
            .map_or(branch_name.to_string(), |m| m.title),
        body: Template(issue.primary_issue.content),
        comments: issue
            .comments
            .into_iter()
            .flatten()
            .map(|comment| Comment {
                body: Template(comment),
            })
            .collect(),
    }
}

fn create_commits<'a, 'b, 'c>(
    git_dir_path: &'a Path,
    repo: &'b GitRepo,
    commits: impl IntoIterator<Item = &'c Commit>,
) -> Result<()> {
    for commit in commits {
        create_commit(git_dir_path, repo, commit)?;
    }

    Ok(())
}

fn create_commit(git_dir_path: &Path, repo: &GitRepo, commit: &Commit) -> Result<()> {
    let message = match &commit.message {
        Some(message) => Cow::Borrowed(message.as_str()),
        None => match commit.path.file_name() {
            None => Cow::Borrowed("solution"),
            Some(name) => name.to_string_lossy(),
        },
    };
    debug!("Creating commit for {:?}.", &commit.path);
    rsync(&commit.path, git_dir_path)?;
    repo.add_all()?;
    repo.commit(message.as_ref())?;
    Ok(())
}

pub fn rsync(from: &Path, to: &Path) -> Result<()> {
    Command::new("rsync")
        .current_dir(from)
        .arg("-a")
        .arg("--delete")
        .arg("--exclude=.git")
        .arg(".")
        .arg(to)
        .run_with_context(|| format!("Could not rsync files from {from:?} to {to:?}."))
}
