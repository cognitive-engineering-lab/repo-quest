#![allow(dead_code)]

use anyhow::{Context, Result};
use futures_util::future::try_join_all;
use http::StatusCode;
use octocrab::{
  issues::IssueHandler,
  models::{
    issues::Issue,
    pulls::{self, PullRequest},
    repos::Branch,
    IssueState,
  },
  pulls::PullRequestHandler,
  repos::RepoHandler,
  GitHubError, Octocrab,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::{sync::OnceCell, time::timeout};

pub struct GithubRepo {
  user: String,
  name: String,
  gh: Arc<Octocrab>,
  prs: OnceCell<Vec<PullRequest>>,
  issues: OnceCell<Vec<Issue>>,
}

impl GithubRepo {
  pub fn new(user: &str, name: &str) -> Self {
    GithubRepo {
      user: user.to_string(),
      name: name.to_string(),
      gh: octocrab::instance(),
      prs: OnceCell::new(),
      issues: OnceCell::new(),
    }
  }

  pub fn remote(&self) -> String {
    format!("git@github.com:{}/{}.git", self.user, self.name)
  }

  pub async fn has_content(&self) -> Result<bool> {
    let result = self.repo_handler().list_commits().send().await;
    match result {
      Err(octocrab::Error::GitHub {
        source: GitHubError {
          status_code: StatusCode::NO_CONTENT,
          ..
        },
        ..
      }) => Ok(false),
      Ok(_) => Ok(true),
      Err(e) => Err(e.into()),
    }
  }

  pub async fn copy_from(&self, base: &GithubRepo) -> Result<()> {
    base
      .repo_handler()
      .generate(&self.name)
      .owner(&self.user)
      .private(true)
      .send()
      .await?;

    // There is some unknown delay between creating a repo from a template and its contents being added.
    // We have to wait until that happens.

    const RETRY_INTERVAL: u64 = 500;
    const RETRY_TIMEOUT: u64 = 5000;

    let strategy = tokio_retry::strategy::FixedInterval::from_millis(RETRY_INTERVAL);
    let has_content = tokio_retry::Retry::spawn(strategy, || async {
      match self.has_content().await {
        Ok(true) => Ok(()),
        _ => Err(()),
      }
    });
    let _ = timeout(Duration::from_millis(RETRY_TIMEOUT), has_content)
      .await
      .context("Repo is still empty after timeout")?;

    let mut page = base.issue_handler().list_labels_for_repo().send().await?;
    let labels = page.take_items();

    let issues = self.issue_handler();
    try_join_all(
      labels
        .into_iter()
        .filter(|label| !label.default)
        .map(|label| {
          issues.create_label(
            label.name,
            label.color,
            label.description.unwrap_or_default(),
          )
        }),
    )
    .await?;

    Ok(())
  }

  pub fn repo_handler(&self) -> RepoHandler {
    self.gh.repos(&self.user, &self.name)
  }

  pub async fn branches(&self) -> Result<Vec<Branch>> {
    let pages = self.repo_handler().list_branches().send().await?;
    let branches = pages.into_iter().collect::<Vec<_>>();
    Ok(branches)
  }

  pub fn pr_handler(&self) -> PullRequestHandler {
    self.gh.pulls(&self.user, &self.name)
  }

  pub async fn prs(&self) -> &[PullRequest] {
    self
      .prs
      .get_or_init(|| async {
        let pages = self.pr_handler().list().send().await.unwrap();
        pages.into_iter().collect::<Vec<_>>()
      })
      .await
  }

  pub async fn pr(&self, ref_field: &str) -> Option<&PullRequest> {
    let prs = self.prs().await;
    prs
      .iter()
      .find(|pr| !matches!(pr.state, Some(IssueState::Closed)) && pr.head.ref_field == ref_field)
  }

  pub fn issue_handler(&self) -> IssueHandler {
    self.gh.issues(&self.user, &self.name)
  }

  pub async fn issues(&self) -> &[Issue] {
    self
      .issues
      .get_or_init(|| async {
        let pages = self.issue_handler().list().send().await.unwrap();
        pages.into_iter().collect::<Vec<_>>()
      })
      .await
  }

  pub async fn issue(&self, label_name: &str) -> Option<&Issue> {
    let issues = self.issues().await;
    issues.iter().find(|issue| {
      !matches!(issue.state, IssueState::Closed)
        && issue.labels.iter().any(|label| label.name == label_name)
    })
  }

  pub async fn copy_pr(&self, base: &GithubRepo, base_pr: &PullRequest, head: &str) -> Result<()> {
    let pulls = self.pr_handler();
    let request = pulls
      .create(
        base_pr
          .title
          .as_ref()
          .expect("Author error: PR missing title"),
        &base_pr.head.ref_field,
        "main", // don't copy base
      )
      .body(
        base_pr
          .body
          .as_ref()
          .expect("Author error: PR missing body"),
      );
    let self_pr = request.send().await?;

    let comment_pages = base
      .pr_handler()
      .list_comments(Some(base_pr.number))
      .send()
      .await?;
    let comments = comment_pages.into_iter().collect::<Vec<_>>();

    for comment in comments {
      self.copy_pr_comment(self_pr.number, &comment, head).await?;
    }

    Ok(())
  }

  pub async fn copy_pr_comment(
    &self,
    pr: u64,
    comment: &pulls::Comment,
    commit: &str,
  ) -> Result<()> {
    let route = format!("/repos/{}/{}/pulls/{pr}/comments", self.user, self.name);
    let comment_json = json!({
      "path": comment.path,
      "commit_id": commit,
      "body": comment.body,
      "line": comment.line
    });
    let _response = self
      .gh
      .post::<_, serde_json::Value>(route, Some(&comment_json))
      .await
      .with_context(|| format!("Failed to copy PR comment: {comment_json:#?}"))?;
    Ok(())
  }

  pub async fn copy_issue(&self, issue: &Issue) -> Result<()> {
    self
      .issue_handler()
      .create(&issue.title)
      .body(issue.body.as_ref().unwrap())
      .labels(
        issue
          .labels
          .iter()
          .map(|label| label.name.clone())
          .collect::<Vec<_>>(),
      )
      .send()
      .await?;
    Ok(())
  }
}
