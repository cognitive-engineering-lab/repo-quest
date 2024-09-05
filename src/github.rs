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
use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use regex::Regex;
use serde_json::json;
use std::{env, fs, process::Command, sync::Arc, time::Duration};
use tokio::{time::timeout, try_join};
use tracing::warn;

use crate::{git::MergeType, utils};

pub struct GithubRepo {
  user: String,
  name: String,
  gh: Arc<Octocrab>,
  prs: Mutex<Option<Vec<PullRequest>>>,
  issues: Mutex<Option<Vec<Issue>>>,
}

pub enum PullSelector {
  Branch(String),
  Label(String),
}

const RESET_LABEL: &str = "reset";

impl GithubRepo {
  pub fn new(user: &str, name: &str) -> Self {
    GithubRepo {
      user: user.to_string(),
      name: name.to_string(),
      gh: octocrab::instance(),
      prs: Mutex::new(None),
      issues: Mutex::new(None),
    }
  }

  pub async fn fetch(&self) -> Result<()> {
    let (pr_handler, issue_handler) = (self.pr_handler(), self.issue_handler());
    let res = try_join!(
      pr_handler.list().state(octocrab::params::State::All).send(),
      issue_handler
        .list()
        .state(octocrab::params::State::All)
        .send()
    );
    let (mut pr_page, mut issue_page) = match res {
      Ok(pages) => pages,
      Err(octocrab::Error::GitHub {
        source: GitHubError {
          status_code: StatusCode::NOT_FOUND,
          ..
        },
        ..
      }) => return Ok(()),
      Err(e) => return Err(e.into()),
    };
    let (prs, mut issues) = (pr_page.take_items(), issue_page.take_items());

    // Pull requests are considered issues, so filter them out
    issues.retain(|issue| issue.pull_request.is_none());

    *self.prs.lock() = Some(prs);
    *self.issues.lock() = Some(issues);
    Ok(())
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
    {
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
    }

    // Unsubscribe from repo notifications to avoid annoying emails.
    {
      let route = format!("/repos/{}/{}/subscription", self.user, self.name);
      let _response = self
        .gh
        .put::<serde_json::Value, _, _>(
          route,
          Some(&json!({
              "subscribed": false,
              "ignored": true
          })),
        )
        .await
        .context("Failed to unsubscribe from repo")?;
    }

    // Copy all issue labels.
    {
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
    }

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

  pub fn prs(&self) -> MappedMutexGuard<'_, Vec<PullRequest>> {
    MutexGuard::map(self.prs.lock(), |opt| opt.as_mut().unwrap())
  }

  pub fn pr(&self, selector: &PullSelector) -> Option<MappedMutexGuard<'_, PullRequest>> {
    let prs = self.prs();
    let idx = prs.iter().position(|pr| match selector {
      PullSelector::Branch(branch) => &pr.head.ref_field == branch,
      PullSelector::Label(label) => pr
        .labels
        .as_ref()
        .map(|labels| labels.iter().any(|l| &l.name == label))
        .unwrap_or(false),
    })?;
    Some(MappedMutexGuard::map(prs, |prs| &mut prs[idx]))
  }

  pub fn issue_handler(&self) -> IssueHandler {
    self.gh.issues(&self.user, &self.name)
  }

  pub fn issues(&self) -> MappedMutexGuard<'_, Vec<Issue>> {
    MutexGuard::map(self.issues.lock(), |opt| opt.as_mut().unwrap())
  }

  pub fn issue(&self, label_name: &str) -> Option<MappedMutexGuard<'_, Issue>> {
    let issues = self.issues();
    let idx = issues
      .iter()
      .position(|issue| issue.labels.iter().any(|label| label.name == label_name))?;
    Some(MappedMutexGuard::map(issues, |issues| &mut issues[idx]))
  }

  pub async fn copy_pr(
    &self,
    base: &GithubRepo,
    base_pr: &PullRequest,
    head: &str,
    merge_type: MergeType,
  ) -> Result<PullRequest> {
    let pulls = self.pr_handler();
    let mut body = base_pr
      .body
      .as_ref()
      .expect("Author error: PR missing body")
      .clone();

    let is_reset = matches!(merge_type, MergeType::HardReset);
    if is_reset {
      body.push_str(r#"
      
Note: due to a merge conflict, this PR is a hard reset to the reference solution, and may have overwritten your previous changes."#);
    }

    let request = pulls
      .create(
        base_pr
          .title
          .as_ref()
          .expect("Author error: PR missing title"),
        &base_pr.head.ref_field,
        "main", // don't copy base
      )
      .body(body);
    let self_pr = request.send().await?;

    // TODO: lots of parallelism below we should exploit

    let mut labels = match &base_pr.labels {
      Some(labels) => labels
        .iter()
        .map(|label| label.name.clone())
        .collect::<Vec<_>>(),
      None => Vec::new(),
    };
    if is_reset {
      labels.push(RESET_LABEL.into());
    }
    self
      .issue_handler()
      .add_labels(self_pr.number, &labels)
      .await?;

    let comment_pages = base
      .pr_handler()
      .list_comments(Some(base_pr.number))
      .send()
      .await?;
    let comments = comment_pages.into_iter().collect::<Vec<_>>();

    for comment in comments {
      self.copy_pr_comment(self_pr.number, &comment, head).await?;
    }

    Ok(self_pr)
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

  fn process_issue_body(&self, body: &str) -> String {
    let re = Regex::new(r"\{\{ (\S+) (\S+) \}\}").unwrap();
    let mut new_body = body.to_string();
    let substitutions = re.captures_iter(body).filter_map(|cap| {
      let full_match = cap.get(0).unwrap();
      let label = &cap[1];
      let kind = &cap[2];
      let number = match kind {
        "pr" => {
          let Some(pr) = self.pr(&PullSelector::Label(label.to_string())) else {
            warn!("No PR with label {label}");
            return None;
          };
          pr.number
        }
        "issue" => {
          let Some(issue) = self.issue(label) else {
            warn!("No issue with label {label}");
            return None;
          };
          issue.number
        }
        _ => unimplemented!(),
      };

      Some((full_match.range(), format!("#{number}")))
    });
    utils::replace_many_ranges(&mut new_body, substitutions);

    // todo!()
    new_body
  }

  pub async fn copy_issue(&self, issue: &Issue) -> Result<Issue> {
    let body = issue.body.as_ref().unwrap();
    let body_processed = self.process_issue_body(body);
    let issue = self
      .issue_handler()
      .create(&issue.title)
      .body(body_processed)
      .labels(
        issue
          .labels
          .iter()
          .map(|label| label.name.clone())
          .collect::<Vec<_>>(),
      )
      .send()
      .await?;
    Ok(issue)
  }

  pub async fn close_issue(&self, issue: &Issue) -> Result<()> {
    self
      .issue_handler()
      .update(issue.number)
      .state(IssueState::Closed)
      .send()
      .await?;
    Ok(())
  }

  pub async fn merge_pr(&self, pr: &PullRequest) -> Result<()> {
    self.pr_handler().merge(pr.number).send().await?;
    Ok(())
  }

  pub async fn delete(&self) -> Result<()> {
    self.repo_handler().delete().await?;
    Ok(())
  }
}

#[derive(Debug)]
pub enum GithubToken {
  Found(String),
  NotFound,
  Error(anyhow::Error),
}

macro_rules! token_try {
  ($e:expr) => {{
    match $e {
      Ok(x) => x,
      Err(e) => return GithubToken::Error(e.into()),
    }
  }};
}

fn read_github_token_from_fs() -> GithubToken {
  let home = match home::home_dir() {
    Some(dir) => dir,
    None => return GithubToken::NotFound,
  };
  let path = home.join(".rqst-token");
  if path.exists() {
    let token = token_try!(fs::read_to_string(path));
    GithubToken::Found(token.trim_end().to_string())
  } else {
    GithubToken::NotFound
  }
}

fn generate_github_token_from_cli() -> GithubToken {
  let shell = env::var("SHELL").unwrap();
  let which_status = Command::new(&shell).args(["-c", "which gh"]).status();
  match which_status {
    Ok(status) => {
      if status.success() {
        let token_output = token_try!(Command::new(shell)
          .args(["-c", "gh auth token"])
          .output()
          .context("Failed to run `gh auth token`"));
        let token = token_try!(String::from_utf8(token_output.stdout));
        let token_clean = token.trim_end().to_string();
        GithubToken::Found(token_clean)
      } else {
        GithubToken::NotFound
      }
    }
    Err(err) => GithubToken::Error(err.into()),
  }
}

pub fn get_github_token() -> GithubToken {
  match read_github_token_from_fs() {
    GithubToken::NotFound => generate_github_token_from_cli(),
    result => result,
  }
}

pub fn init_octocrab(token: &str) -> Result<()> {
  let crab_inst = Octocrab::builder()
    .personal_token(token.to_string())
    .build()?;
  octocrab::initialise(crab_inst);
  Ok(())
}
