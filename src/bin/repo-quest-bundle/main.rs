use std::{
    collections::{HashMap, hash_map::Entry},
    fs,
    path::PathBuf,
};

use anyhow::{Context as _, bail};
use axum::http::header::ACCEPT;
use clap::Parser;
use env_logger::Env;
use log::debug;
use octocrab::{
    Octocrab,
    models::{issues::Issue, pulls::PullRequest},
    params::State,
};
use repo_quest::{
    git::GitRepo,
    quest::definition::{
        Comment, GitCommitHash, IssueTemplate, PullRequestComment, PullRequestTemplate,
        QuestDefinitionMetadata, ReviewLineSubject, ReviewSubject, TaskTemplate, Template,
    },
};
use serde::{Deserialize, Serialize};

// This is the metadata given for Quests defined on GitHub.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QuestConfig {
    pub title: String,
    pub author: String,
    pub repo: String,
    pub chapters: Vec<Chapter>,
    pub read_only: Option<Vec<PathBuf>>,
    pub r#final: Option<serde_json::Value>,
    pub final_url: Option<String>,
    pub rq_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Chapter {
    pub label: String,
    pub name: String,
    #[serde(default)]
    pub no_starter: bool,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
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
}

type Result<A> = anyhow::Result<A>;

#[tokio::main]
async fn main() -> Result<()> {
    // take GitHub slug as argument
    let Args {
        token,
        base_uri,
        owner,
        repo: repo_name,
        output,
    } = Args::parse();
    #[cfg(not(debug_assertions))]
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    let workdir = tempfile::tempdir().context("Could not create temporary working directory.")?;
    // reserve tarball
    let tar_file = fs::File::create(output).context("Could not create output file {output}.")?;

    // clone repo from GitHub (as bare repo)
    let git_dir_path = workdir.path().to_path_buf().join("git");

    // TODO: Handle cloning private repos
    debug!("Cloning https://github.com/{owner}/{repo_name} into {git_dir_path:?}.");
    fs::create_dir_all(&git_dir_path)?;
    let repo = GitRepo::init_bare(git_dir_path.clone())?;
    repo.add_remote("origin", &format!("https://github.com/{owner}/{repo_name}"))?;
    repo.fetch("origin")?;

    // get quest metadata from meta branch
    repo.create_tracking_branch("origin/meta", "meta")?;
    let rev = repo.rev_parse("meta:rqst.toml")?;
    debug!("Metadata blob oid is {rev}.");
    let meta = toml::de::from_str::<QuestConfig>(
        &repo
            .cat_blob(&rev)
            .context("Failed to parse quest configuration rqst.toml")?,
    )?;
    debug!("Metadata is {meta:?}.");

    // create local branch for main and for every chapter
    let mut prev_scaffolding_branch = "main".to_string();
    repo.create_tracking_branch("origin/main", "main")?;
    for Chapter {
        name,
        label,
        no_starter,
        ..
    } in &meta.chapters
    {
        let scaffolding_branch = format!("{label}-a");
        if !*no_starter {
            repo.create_tracking_branch(&format!("origin/{label}-a"), &scaffolding_branch)?;
        } else {
            repo.create_branch(&prev_scaffolding_branch, &format!("{label}-a"))?;
            // create empty commit so that a PR can be created from the branch
            repo.create_empty_commit(
                &scaffolding_branch,
                &format!("Initial commit for chapter {name}"),
            )?;
        }
        prev_scaffolding_branch = scaffolding_branch;
        repo.create_tracking_branch(&format!("origin/{label}-b"), &format!("{label}-b"))?;
    }

    // TODO: Should the remote be removed from the repo?

    // Connect to the GitHub API.
    let github = Github::new(&base_uri, owner, repo_name, token)?;

    // Initialize

    // NOTE: GitHub pull requests are all issues, so the number for the pull
    // request is the same as the number for the issue and can be used to look
    // up the issue-relevant data about the pull request (such as non-review
    // comments).

    // Tracks issues (including PR issues) based on the chapter label. Only
    // includes non-PR issues.
    let mut label_to_issue = HashMap::<&str, _>::new();
    // Tracks PRs based on the chapter label. The PRs are identified based on
    // their branch name.
    let mut label_to_pull = HashMap::<&str, _>::new();
    let mut task_ids = HashMap::<String, usize>::new();
    for (task_id, Chapter { label, .. }) in meta.chapters.iter().enumerate() {
        // The task/chapter ids and associate them with the labels. The ids are
        // in chapter order starting from 0.
        task_ids.insert(label.to_string(), task_id);
        // There may be more than one issue with the label, which we need to
        // detect and inform the user about.
        label_to_issue.insert(label, Vec::new());
        // There may be more than one pull request with the branch, which we
        // need to detect and inform the user about.
        //
        // We index on the branch name with the -a removed, which is the chapter
        // label.
        label_to_pull.insert(label, Vec::new());
    }

    // Fetch task information

    // Record issues with labels that match chapter labels.
    debug!("Fetching GitHub issue.");
    let github_issues = github.open_issues().await?;
    for issue in &github_issues {
        for label in &issue.labels {
            if issue.pull_request.is_none() {
                match label_to_issue.entry(&label.name) {
                    Entry::Occupied(mut entry) => entry.get_mut().push(issue),
                    Entry::Vacant(_) => {}
                }
            }
        }
    }

    // Record all PR with branch names that match chaper labels (with the -a
    // suffix), and index them by chapter label.
    debug!("Fetching GitHub pull requests.");
    let github_pulls = github.open_prs().await?;
    for pull in &github_pulls {
        // TODO: confirm that `None` means that it is the same repo.
        let same_owner = pull
            .head
            .user
            .as_ref()
            .is_none_or(|o| o.login == github.owner);
        let same_repo = pull
            .repo
            .as_ref()
            .is_none_or(|r| r.name == github.repo_name);
        if same_owner && same_repo && pull.head.ref_field.ends_with("-a") {
            let (label, _suffix) = pull.head.ref_field.split_at(pull.head.ref_field.len() - 2);
            match label_to_pull.entry(label) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().push(pull);
                }
                Entry::Vacant(_) => {}
            }
        }
    }

    debug!("Extract issue and pull request information for each chapter.");
    let mut tasks = Vec::with_capacity(meta.chapters.len());
    for Chapter { name, label, .. } in &meta.chapters {
        debug!("Extracting info for chapter {name} with label {label}.");
        // The maps were constructed from the labels we are iterating over, so
        // the entries are guaranteed to be there.
        //
        // However, we are not guaranteed to have each label match a single
        // issue. If they don't, we need to tell the user so that they can fix
        // the quest definition.
        let &[matching_issue] = &label_to_issue[label.as_str()].as_slice() else {
            bail!(
                "Chapter {name} with label {label} has {} issues but should have exactly one.",
                label_to_issue[label.as_str()].len(),
            );
        };
        // Again, the maps were constructed from the labels we are iterating
        // over, so the entries are guaranteed to be there.
        //
        // However, we are not guaranteed to have 0 or 1 matching pull requests.
        // If we don't, we need to tell the user so they can fix the quest
        // definition.
        let matching_pulls = &label_to_pull[label.as_str()];
        let matching_pull = match matching_pulls.first() {
            Some(pull) if matching_pulls.len() == 1 => Some(pull),
            None => None,
            _ => bail!(
                "Chapter {name} with label {label} has {} PRs but should have at most one.",
                matching_pulls.len()
            ),
        };

        // Fetch the comment for the matching issue to create the issue template.
        let issue_template = {
            let title = matching_issue.title.clone();
            let body = Template(
                matching_issue
                    .body
                    .clone()
                    .with_context(|| format!("Issue for label {label} has no body."))?,
            );
            let github_comments = github.issue_comments(matching_issue.number).await?;
            let mut comments = Vec::with_capacity(github_comments.len());
            for github_comment in github_comments {
                comments.push(Comment {
                    body: Template(github_comment.body.with_context(|| {
                        format!(
                            "Comment {} for issue {} on {github:?} has no body.",
                            github_comment.id, matching_issue.number,
                        )
                    })?),
                });
            }

            IssueTemplate {
                title,
                body,
                comments,
            }
        };

        // Fetch the comments for the matching pull request to create the pull request template.
        let pr_template = match matching_pull {
            Some(pull) => Some({
                let title = pull
                    .title
                    .clone()
                    .unwrap_or_else(|| matching_issue.title.clone());
                let body = Template(
                    pull.body
                        .clone()
                        .with_context(|| format!("Issue for label {label} has no body."))?,
                );

                // collect all PR-type and issue-type along with the date, and sort
                // by date so that the resulting order matches the order on GitHub
                let github_issue_comments = github.issue_comments(pull.number).await?;
                let github_pr_comments = github.pr_comments(pull.number).await?;
                let mut comments =
                    Vec::with_capacity(github_issue_comments.len() + github_pr_comments.len());
                for github_comment in github_issue_comments {
                    comments.push(PullRequestComment {
                        quote: None,
                        body: Template(github_comment.body.with_context(|| {
                            format!(
                                "Pull request {} comment {} from {github:?} has no body.",
                                pull.number, github_comment.id,
                            )
                        })?),
                    });
                }
                for github_comment in github_pr_comments {
                    let quote = ReviewSubject {
                        commit: GitCommitHash(github_comment.commit_id),
                        file: github_comment.path,
                        old_line: match github_comment.original_line {
                            Some(line) => Some(ReviewLineSubject {
                                start: github_comment.original_start_line,
                                end: line,
                            }),
                            None => None,
                        },
                        new_line: match github_comment.line {
                            Some(line) => Some(ReviewLineSubject {
                                start: github_comment.start_line,
                                end: line,
                            }),
                            None => None,
                        },
                    };

                    comments.push(PullRequestComment {
                        quote: Some(quote),
                        body: Template(github_comment.body),
                    });
                }

                PullRequestTemplate {
                    title,
                    body,
                    comments,
                }
            }),
            None => None,
        };

        tasks.push(TaskTemplate {
            task_id: label.clone(),
            issue_template,
            pr_template,
            scaffolding: (label.to_string() + "-a").into(),
            reference_solution: (label.to_string() + "-b").into(),
        });
    }

    let quest = QuestDefinitionMetadata {
        title: meta.title,
        author: meta.author,
        description: "".to_string(),
        generated_repo_name: github.repo_name,
        tasks,
        task_ids,
    };

    debug!("Writing quest definition metadata to temporary file.");
    // write quest definition to file
    let quest_json = serde_json::to_string(&quest)
        .with_context(|| format!("Could not serialize quest {quest:?}"))?;
    let quest_json_path = workdir.path().to_path_buf().join("data.json");
    fs::write(&quest_json_path, &quest_json)
        .with_context(|| format!("Could not write quest index to file {quest_json_path:?}"))?;

    debug!("Creating bundle archive.");
    // create tarball
    let mut archive = tar::Builder::new(tar_file);
    archive
        .append_path_with_name(&quest_json_path, "data.json")
        .context("Could not add data.json to bundle.")?;
    archive
        .append_dir_all("git", &git_dir_path)
        .context("Could not add git repo to bundle.")?;
    archive.finish().context("Could not finalize archive")?;

    Ok(())
}

#[derive(Debug)]
struct Github {
    octocrab: Octocrab,
    owner: String,
    repo_name: String,
}

impl Github {
    pub fn new(
        base_uri: &str,
        owner: String,
        repo_name: String,
        token: Option<String>,
    ) -> Result<Self> {
        let mut builder = octocrab::Octocrab::builder();
        builder = builder
            .base_uri(base_uri)?
            .add_header(ACCEPT, "application/json".to_string());
        if let Some(token) = token {
            debug!("Using provided GitHub token for authenticating.");
            builder = builder.user_access_token(token);
        }
        let octocrab = builder.build()?;

        Ok(Github {
            octocrab,
            owner,
            repo_name,
        })
    }

    async fn all_pages<T, Fut, F>(f: F) -> Result<Vec<T>>
    where
        Fut: Future<Output = Result<(Vec<T>, u32)>>,
        F: Fn(u32) -> Fut,
    {
        let (mut items, num_pages) = f(0).await?;
        let mut page = 1;
        while page < num_pages {
            let (mut item_page, _) = f(page).await?;
            items.append(&mut item_page);
            page += 1;
        }
        Ok(items)
    }

    async fn open_issues_page(&self, page: u32) -> Result<(Vec<Issue>, u32)> {
        let issues_page = self
            .octocrab
            .issues(&self.owner, &self.repo_name)
            .list()
            .state(State::Open)
            .page(page)
            .send()
            .await?;
        let num_pages = issues_page.number_of_pages().unwrap_or(1);
        let issues = issues_page.items;
        Ok((issues, num_pages))
    }

    pub async fn open_issues(&self) -> Result<Vec<Issue>> {
        Self::all_pages(|page| self.open_issues_page(page)).await
    }

    async fn issue_comments_page(
        &self,
        issue_number: u64,
        page: u32,
    ) -> Result<(Vec<octocrab::models::issues::Comment>, u32)> {
        let comments_page = self
            .octocrab
            .issues(&self.owner, &self.repo_name)
            .list_comments(issue_number)
            .page(page)
            .send()
            .await?;
        let num_pages = comments_page.number_of_pages().unwrap_or(1);
        let comments = comments_page.items;
        Ok((comments, num_pages))
    }

    pub async fn issue_comments(
        &self,
        issue_number: u64,
    ) -> Result<Vec<octocrab::models::issues::Comment>> {
        Self::all_pages(|page| self.issue_comments_page(issue_number, page)).await
    }

    pub async fn open_prs_page(&self, page: u32) -> Result<(Vec<PullRequest>, u32)> {
        let issues_page = self
            .octocrab
            .pulls(&self.owner, &self.repo_name)
            .list()
            .state(State::Open)
            .page(page)
            .send()
            .await?;
        let num_pages = issues_page.number_of_pages().unwrap_or(1);
        let issues = issues_page.items;
        Ok((issues, num_pages))
    }

    pub async fn open_prs(&self) -> Result<Vec<PullRequest>> {
        Self::all_pages(|page| self.open_prs_page(page)).await
    }

    async fn pr_comments_page(
        &self,
        pr_number: u64,
        page: u32,
    ) -> Result<(Vec<octocrab::models::pulls::Comment>, u32)> {
        let comments_page = self
            .octocrab
            .pulls(&self.owner, &self.repo_name)
            .list_comments(Some(pr_number))
            .page(page)
            .send()
            .await?;
        let num_pages = comments_page.number_of_pages().unwrap_or(1);
        let comments = comments_page.items;
        Ok((comments, num_pages))
    }

    pub async fn pr_comments(
        &self,
        pr_number: u64,
    ) -> Result<Vec<octocrab::models::pulls::Comment>> {
        Self::all_pages(|page| self.pr_comments_page(pr_number, page)).await
    }
}
