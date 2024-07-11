#![allow(warnings)]

use anyhow::{ensure, Context, Result};
use octocrab::{
  issues::IssueHandler,
  models::{
    issues::Issue,
    pulls::{self, PullRequest},
    IssueState,
  },
  pulls::PullRequestHandler,
  repos::RepoHandler,
  GitHubError, Octocrab,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
  fmt,
  io::Write,
  process::{Command, Stdio},
  sync::Arc,
};
use tokio::sync::OnceCell;

struct Repo {
  user: String,
  name: String,
  gh: Arc<Octocrab>,
  prs: OnceCell<Vec<PullRequest>>,
  issues: OnceCell<Vec<Issue>>,
}

impl Repo {
  pub fn new(user: &str, name: &str) -> Self {
    Repo {
      user: user.to_string(),
      name: name.to_string(),
      gh: octocrab::instance(),
      prs: OnceCell::new(),
      issues: OnceCell::new(),
    }
  }

  pub async fn exists(&self) -> Result<bool> {
    match self.gh.repos(&self.user, &self.name).get().await {
      Ok(_) => Ok(true),
      Err(octocrab::Error::GitHub {
        source:
          GitHubError {
            status_code: http::StatusCode::NOT_FOUND,
            ..
          },
        ..
      }) => Ok(false),
      Err(e) => Err(e.into()),
    }
  }

  pub async fn fork_from(&self, base: &Repo) -> Result<()> {
    base.repo_handler().create_fork().send().await?;

    let repo_route = format!("/repos/{}/{}", self.user, self.name);
    let _response = self
      .gh
      .patch::<serde_json::Value, _, _>(
        &repo_route,
        Some(&json!({
          "has_issues": true
        })),
      )
      .await?;

    // TODO: any way to enable actions?

    Ok(())
  }

  pub fn repo_handler(&self) -> RepoHandler {
    self.gh.repos(&self.user, &self.name)
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

  pub async fn issue(&self, title: &str) -> Option<&Issue> {
    let issues = self.issues().await;
    issues
      .iter()
      .find(|issue| !matches!(issue.state, IssueState::Closed) && issue.title == title)
  }

  pub async fn copy_pr(&self, base: &Repo, base_pr: &PullRequest) -> Result<()> {
    let pulls = self.pr_handler();
    let request = pulls
      .create(
        base_pr.title.as_ref().unwrap(),
        &base_pr.head.ref_field,
        &base_pr.base.ref_field,
      )
      .body(base_pr.body.as_ref().unwrap());
    let self_pr = request.send().await?;

    let comment_pages = base
      .pr_handler()
      .list_comments(Some(base_pr.number))
      .send()
      .await?;
    let comments = comment_pages.into_iter().collect::<Vec<_>>();

    for comment in &comments {
      self.copy_pr_comment(self_pr.number, comment).await?;
    }

    Ok(())
  }

  pub async fn copy_pr_comment(&self, pr: u64, comment: &pulls::Comment) -> Result<()> {
    let route = format!("/repos/{}/{}/pulls/{pr}/comments", self.user, self.name);
    let _response = self
      .gh
      .post::<_, serde_json::Value>(route, Some(&comment))
      .await?;
    Ok(())
  }

  pub async fn copy_issue(&self, issue: &Issue) -> Result<()> {
    self
      .issue_handler()
      .create(&issue.title)
      .body(issue.body.as_ref().unwrap())
      .send()
      .await?;
    Ok(())
  }
}

fn get_github_token() -> Result<String> {
  let token_output = Command::new("gh").args(["auth", "token"]).output()?;
  let token = String::from_utf8(token_output.stdout)?;
  let token_clean = token.trim_end().to_string();
  Ok(token_clean)
}

#[tokio::main]
async fn main() -> Result<()> {
  let step = std::env::args()
    .skip(1)
    .next()
    .unwrap()
    .parse::<usize>()
    .unwrap();

  let token = get_github_token()?;
  let crab_inst = Octocrab::builder().personal_token(token).build()?;
  octocrab::initialise(crab_inst);

  let base = Repo::new("cognitive-engineering-lab", "rqst-async");
  let user = octocrab::instance().current().user().await?.login;
  let fork = Repo::new(&user, "rqst-async");

  match step {
    1 => fork.fork_from(&base).await?,
    2 => {
      let pr = base.pr("01a-async-await").await.unwrap();
      fork.copy_pr(&base, pr).await?;

      let issue = base
        .issue("Use chatbot model in place of a fixed response")
        .await
        .unwrap();
      fork.copy_issue(issue).await?;
    }
    3 => {
      let pr = base.pr("01b-async-await").await.unwrap();
      fork.copy_pr(&base, pr).await?;
    }
    _ => todo!(),
  }

  Ok(())
}
