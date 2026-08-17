use std::{collections::HashMap, path::Path};

use anyhow::{Context as _, Result};
use log::debug;
use repo_quest_core::{
    fs,
    quest::{
        definition::{
            Comment, IssueTemplate, PullRequestTemplate, QuestDefinitionMetadata,
            ReviewLineSubject, ReviewSubject, TaskTemplate,
        },
        template::Template,
    },
};

use crate::{commands::quest_to_hist, util::rsync};

use super::{
    Chapter, CommitKind, Issue, LineSide, PullRequest, PullRequestComment, QuestDefinition,
};

/// Helper to take last element while ensuring modified Vec doesn't get used
/// again by accident.
fn take_last<T>(mut v: Vec<T>) -> Option<T> {
    v.pop()
}

/// Converts a `dir::QuestDefinition` into the bundle format on disk.
pub fn bundle(quest: QuestDefinition, output: &Path) -> Result<()> {
    let workdir = tempfile::tempdir().context("Could not create temporary working directory.")?;

    const QUEST_BRANCH_PREFIX: &str = "quest";

    // Create repo dir
    //
    // This repo has a worktree so that we can easily copy in snapshots. We'll
    // convert it to a bare repo later.
    let git_dir_path = workdir.path().join("repo");
    let repo = quest_to_hist(&quest, git_dir_path.clone(), QUEST_BRANCH_PREFIX)?;

    let last_main = take_last(quest.main).unwrap(); // unwrap: parse checks that main has at least one commit
    let last_main_commit = CommitKind::Main.branch_name(QUEST_BRANCH_PREFIX, &last_main.path);
    // quest_to_hist leaves the repo pointing to "main"
    repo.hard_reset(&last_main_commit)?;

    // Construct metadata and solution/scaffolding commits for each chapter
    let mut task_ids = HashMap::<String, usize>::new();
    let mut tasks = Vec::<TaskTemplate>::new();
    // if the initiail chapter omits the scaffold, use the last main commit in
    // place of a previous solution commit, since there is no previous solution
    let mut last_solution_ref = last_main_commit;
    for (
        task_id,
        Chapter {
            label,
            issue,
            pull_request,
            scaffold,
            solution,
        },
    ) in quest.chapters.into_iter().enumerate()
    {
        let scaffold_branch_name = label.clone();
        let solution_branch_name = format!("{label}-solution");

        let scaffold_commit_kind = CommitKind::Scaffold {
            chapter_label: &label,
        };
        let solution_commit_kind = CommitKind::Solution {
            chapter_label: &label,
        };

        // Create scaffold commit
        let last_scaffold_branch = if let Some(scaffold) = scaffold
            && let Some(scaffold) = take_last(scaffold)
        {
            scaffold_commit_kind.branch_name(QUEST_BRANCH_PREFIX, &scaffold.path)
        } else {
            last_solution_ref
        };
        repo.create_branch(&last_scaffold_branch, &scaffold_branch_name)?;

        // Create solution commit
        let solution_commit = take_last(solution).unwrap(); // unwrap: parse guarantees it has least one commit
        let last_solution_branch =
            solution_commit_kind.branch_name(QUEST_BRANCH_PREFIX, &solution_commit.path);
        repo.create_branch(&last_solution_branch, &solution_branch_name)?;

        // remember the previous solution commit, in case there is no scaffold
        // in the next chapter
        last_solution_ref = last_solution_branch;

        let issue_template = bundle_issue(&label, issue);
        let pr_template = bundle_pull_request(&label, pull_request);

        tasks.push(TaskTemplate {
            task_id: label.clone(),
            issue_template,
            pr_template: Some(pr_template),
            scaffolding: scaffold_branch_name.into(),
            reference_solution: solution_branch_name.into(),
        });

        task_ids.insert(label, task_id);
    }

    // Convert .git in repo to a bare repo
    repo.make_bare()?;

    // Assemble metadata
    let quest_meta = QuestDefinitionMetadata {
        title: quest.title,
        author: quest.author,
	repository: quest.repository,
        description: quest.description,
        generated_repo_name: quest.quest_repo,
        tasks,
        task_ids,
    };

    debug!("Writing quest definition metadata to temporary file.");
    // Write quest definition to file
    let quest_json = serde_json::to_string(&quest_meta)
        .with_context(|| format!("Could not serialize quest {quest_meta:?}"))?;
    let quest_json_path = workdir.path().join("data.json");
    fs::write(&quest_json_path, &quest_json, "quest index")?;

    let bundle_assets_dir = workdir.path().join("assets");
    fs::create_dir_all(&bundle_assets_dir, "bundle assets dir")?;
    debug!("Copy assets into working directory.");
    if let Some(asset_dir) = quest.assets_dir {
        rsync(&asset_dir, &bundle_assets_dir).with_context(|| {
            format!(
                "Could not copy bundle assets from `{}` to `{}`.",
                asset_dir.display(),
                bundle_assets_dir.display()
            )
        })?;
    }

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
    archive
        .append_dir_all("assets", &bundle_assets_dir)
        .context("Could not add git repo to bundle.")?;
    archive.finish().context("Could not finalize archive")?;

    Ok(())
}

fn bundle_pull_request(branch_name: &str, pull_request: PullRequest) -> PullRequestTemplate {
    debug!("Creating pull request data.");
    let (pr_title, pr_body) = match pull_request.primary_issue {
        None => (branch_name.to_string(), String::new()),
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
) -> repo_quest_core::quest::definition::PullRequestComment {
    repo_quest_core::quest::definition::PullRequestComment {
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

fn bundle_issue(branch_name: &str, issue: Issue) -> IssueTemplate {
    debug!("Creating issue data.");
    IssueTemplate {
        title: issue
            .primary_issue
            .meta
            .map_or_else(|| branch_name.to_string(), |m| m.title),
        body: Template(issue.primary_issue.content),
        comments: issue
            .comments
            .into_iter()
            .flatten()
            .map(|comment| Comment {
                body: Template(comment.content),
            })
            .collect(),
    }
}
