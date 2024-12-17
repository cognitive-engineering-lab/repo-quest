use anyhow::{bail, ensure, Context, Result};
use futures_util::future::try_join_all;
use http::StatusCode;
use octocrab::{
  issues::IssueHandler,
  models::{
    issues::Issue,
    pulls::{self, PullRequest},
    repos::Branch,
    IssueState, Label,
  },
  pulls::PullRequestHandler,
  repos::RepoHandler,
  GitHubError, Octocrab,
};
use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use std::{env, fs, path::Path, sync::Arc, time::Duration};
use tokio::{time::timeout, try_join};
use tracing::warn;

use crate::{
  command::command,
  git::{GitRepo, MergeType},
  package::QuestPackage,
  utils,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct FullPullRequest {
  pub data: PullRequest,
  pub comments: Vec<pulls::Comment>,
}

pub struct GithubRepo {
  user: String,
  name: String,
  gh: Arc<Octocrab>,
  prs: Mutex<Option<Vec<FullPullRequest>>>,
  issues: Mutex<Option<Vec<Issue>>>,
}

#[derive(Debug)]
pub enum PullSelector {
  Branch(String),
  Label(String),
}

pub fn find_pr<'a>(
  selector: &PullSelector,
  prs: impl IntoIterator<Item = &'a FullPullRequest> + 'a,
) -> Option<usize> {
  prs.into_iter().position(|pr| match selector {
    PullSelector::Branch(branch) => &pr.data.head.ref_field == branch,
    PullSelector::Label(label) => pr
      .data
      .labels
      .as_ref()
      .map(|labels| labels.iter().any(|l| &l.name == label))
      .unwrap_or(false),
  })
}

pub fn find_issue<'a>(
  label_name: &str,
  issues: impl IntoIterator<Item = &'a Issue> + 'a,
) -> Option<usize> {
  issues
    .into_iter()
    .position(|issue| issue.labels.iter().any(|label| label.name == label_name))
}

const RESET_LABEL: &str = "reset";

pub async fn load_user() -> Result<String> {
  let user = octocrab::instance()
    .current()
    .user()
    .await
    .context("Failed to query Github connector for current user")?;
  Ok(user.login)
}

/// Checks that the user's SSH keys are configured such that `git clone git@github.com:...` can be run.
pub fn check_ssh() -> Result<()> {
  let output = command("ssh -T git@github.com", Path::new("/")).output()?;
  match output.status.code() {
    // `ssh` exits with status 1 for "success" here, and status 255 for failure
    Some(1) => Ok(()),
    _ => {
      let stderr = String::from_utf8(output.stderr)?;
      if stderr.trim() == "git@github.com: Permission denied (publickey)." {
        bail!("Your machine is not setup for a secure connection to Github. Please follow the instructions here: https://docs.github.com/en/authentication/troubleshooting-ssh/error-permission-denied-publickey");
      } else {
        bail!("Failed to establish a secure connection to Github with error:\n{stderr}")
      }
    }
  }
}

pub enum GitProtocol {
  Ssh,
  Https,
}

#[derive(PartialEq, Eq, Debug)]
pub enum TestRepoResult {
  HasContent,
  NoContent,
  NotFound,
}

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

  pub async fn load(user: &str, name: &str) -> Result<Self> {
    let repo = GithubRepo::new(user, name);
    ensure!(repo.fetch().await?, "Not found");
    Ok(repo)
  }

  /// Returns true if repo
  pub async fn fetch(&self) -> Result<bool> {
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
      }) => return Ok(false),
      Err(e) => return Err(e.into()),
    };
    let (prs, mut issues) = (pr_page.take_items(), issue_page.take_items());

    let full_prs = try_join_all(prs.into_iter().map(|pr| async move {
      let comment_pages = self
        .pr_handler()
        .list_comments(Some(pr.number))
        .send()
        .await
        .with_context(|| format!("Failed to fetch comments for PR {}", pr.number))?;
      let comments = comment_pages.into_iter().collect::<Vec<_>>();
      Ok::<_, anyhow::Error>(FullPullRequest { data: pr, comments })
    }))
    .await?;

    // Pull requests are considered issues, so filter them out
    issues.retain(|issue| issue.pull_request.is_none());

    *self.prs.lock() = Some(full_prs);
    *self.issues.lock() = Some(issues);

    Ok(true)
  }

  pub fn remote(&self, protocol: GitProtocol) -> String {
    match protocol {
      GitProtocol::Https => format!("https://github.com/{}/{}", self.user, self.name),
      GitProtocol::Ssh => format!("git@github.com:{}/{}.git", self.user, self.name),
    }
  }

  pub async fn test_repo(&self) -> Result<TestRepoResult> {
    let result = self.repo_handler().list_commits().send().await;
    match result {
      Err(octocrab::Error::GitHub {
        source:
          GitHubError {
            status_code: StatusCode::NO_CONTENT | StatusCode::CONFLICT,
            ..
          },
        ..
      }) => Ok(TestRepoResult::NoContent),
      Err(octocrab::Error::GitHub {
        source: GitHubError {
          status_code: StatusCode::NOT_FOUND,
          ..
        },
        ..
      }) => Ok(TestRepoResult::NotFound),
      Ok(_) => Ok(TestRepoResult::HasContent),
      Err(e) => {
        if let octocrab::Error::GitHub {
          source: GitHubError { status_code, .. },
          ..
        } = &e
        {
          tracing::debug!("Error: {status_code:?}");
        }

        Err(e.into())
      }
    }
  }

  pub fn clone(&self, path: &Path) -> Result<GitRepo> {
    let remote = self.remote(GitProtocol::Ssh);
    GitRepo::clone(&path.join(&self.name), &remote)
  }

  // There is some unknown delay between creating a repo from a template and its contents being added.
  // We have to wait until that happens
  async fn wait_for_content(&self, expected: TestRepoResult) -> Result<()> {
    const RETRY_INTERVAL: u64 = 500;
    const RETRY_TIMEOUT: u64 = 5000;

    let strategy = tokio_retry::strategy::FixedInterval::from_millis(RETRY_INTERVAL);
    let has_content = tokio_retry::Retry::spawn(strategy, || async {
      match self.test_repo().await {
        Ok(actual) if expected == actual => Ok(()),
        result => {
          tracing::debug!("wait status: {result:?}");
          Err(result)
        }
      }
    });
    let _ = timeout(Duration::from_millis(RETRY_TIMEOUT), has_content)
      .await
      .context("Repo is still empty after timeout")?;

    Ok(())
  }

  async fn create_labels(&self, labels: &[Label]) -> Result<()> {
    let issues = self.issue_handler();
    try_join_all(labels.iter().filter(|label| !label.default).map(|label| {
      issues.create_label(
        &label.name,
        &label.color,
        label.description.as_deref().unwrap_or(""),
      )
    }))
    .await
    .context("Failed to create labels")?;
    Ok(())
  }

  async fn unsubscribe(&self) -> Result<()> {
    let route = format!("/repos/{}/{}/subscription", self.user, self.name);
    self
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
    Ok(())
  }

  pub async fn instantiate_from_package(package: &QuestPackage) -> Result<GithubRepo> {
    let user = load_user().await.context("Failed to load user")?;
    let params = json!({
        "name": &package.config.repo,
        "private": true,
    });
    octocrab::instance()
      .post::<_, serde_json::Value>("/user/repos", Some(&params))
      .await
      .context("Failed to create repo")?;
    let repo = GithubRepo::new(&user, &package.config.repo);
    repo
      .wait_for_content(TestRepoResult::NoContent)
      .await
      .context("Github repo was not properly initialized")?;
    repo
      .unsubscribe()
      .await
      .context("Failed to unsubscribe from repo")?;
    repo
      .create_labels(&package.labels)
      .await
      .context("Failed to transfer package labels to repo")?;
    Ok(repo)
  }

  pub async fn instantiate_from_repo(base: &GithubRepo) -> Result<GithubRepo> {
    let user = load_user().await?;
    let name = &base.name;
    base
      .repo_handler()
      .generate(name)
      .owner(&user)
      // TODO: make this configurable? Right now we don't want privacy so we can see learner progress
      // .private(true)
      .send()
      .await
      .with_context(|| format!("Failed to clone template repo {}/{}", base.user, base.name))?;

    let repo = GithubRepo::new(&user, name);
    repo
      .wait_for_content(TestRepoResult::HasContent)
      .await
      .context("Github repo was not properly initialized")?;

    // Unsubscribe from repo notifications to avoid annoying emails.
    repo
      .unsubscribe()
      .await
      .context("Failed to unsubscribe from repo")?;

    // Copy all issue labels.
    let mut page = base
      .issue_handler()
      .list_labels_for_repo()
      .send()
      .await
      .context("Failed to fetch labels from upstream repo")?;
    let labels = page.take_items();
    repo
      .create_labels(&labels)
      .await
      .context("Failed to transfer upstream labels to repo")?;

    Ok(repo)
  }

  pub fn repo_handler(&self) -> RepoHandler {
    self.gh.repos(&self.user, &self.name)
  }

  pub async fn branches(&self) -> Result<Vec<Branch>> {
    let pages = self
      .repo_handler()
      .list_branches()
      .send()
      .await
      .context("Failed to fetch branches")?;
    let branches = pages.into_iter().collect::<Vec<_>>();
    Ok(branches)
  }

  pub fn pr_handler(&self) -> PullRequestHandler {
    self.gh.pulls(&self.user, &self.name)
  }

  pub fn prs(&self) -> MappedMutexGuard<'_, Vec<FullPullRequest>> {
    MutexGuard::map(self.prs.lock(), |opt| {
      opt.as_mut().expect("PRs not populated")
    })
  }

  pub fn pr(&self, selector: &PullSelector) -> Option<MappedMutexGuard<'_, FullPullRequest>> {
    let prs = self.prs();
    let idx = find_pr(selector, prs.iter())?;
    Some(MappedMutexGuard::map(prs, |prs| &mut prs[idx]))
  }

  pub fn issue_handler(&self) -> IssueHandler {
    self.gh.issues(&self.user, &self.name)
  }

  pub fn issues(&self) -> MappedMutexGuard<'_, Vec<Issue>> {
    MutexGuard::map(self.issues.lock(), |opt| {
      opt.as_mut().expect("Issues not populated")
    })
  }

  pub fn issue(&self, label_name: &str) -> Option<MappedMutexGuard<'_, Issue>> {
    let issues = self.issues();
    let idx = find_issue(label_name, issues.iter())?;
    Some(MappedMutexGuard::map(issues, |issues| &mut issues[idx]))
  }

  pub async fn copy_pr(
    &self,
    pr: &FullPullRequest,
    head: &str,
    merge_type: MergeType,
  ) -> Result<PullRequest> {
    let pulls = self.pr_handler();
    let mut body = pr
      .data
      .body
      .as_ref()
      .expect("Author error: PR missing body")
      .clone();

    let is_reset = match merge_type {
      MergeType::SolutionReset => {
        body.push_str(r#"

Note: due to a merge conflict, this PR is a hard reset to the reference solution, and may have overwritten your previous changes."#);
        true
      }

      MergeType::StarterReset => {
        body.push_str(r#"

Note: due to a merge conflict, this PR is a hard reset to the starter code, and may have overwritten your previous changes."#);
        true
      }

      MergeType::Success => false,
    };

    let request = pulls
      .create(
        pr.data
          .title
          .as_ref()
          .expect("Author error: PR missing title"),
        &pr.data.head.ref_field,
        "main", // don't copy base
      )
      .body(body);
    let self_pr = request.send().await.context("Failed to create new PR")?;

    // TODO: lots of parallelism below we should exploit

    let mut labels = match &pr.data.labels {
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
      .await
      .context("Failed to add labels to PR")?;

    for comment in &pr.comments {
      self
        .copy_pr_comment(self_pr.number, comment, head)
        .await
        .context("Failed to add comment to PR")?;
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
          pr.data.number
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
      .await
      .with_context(|| format!("Failed to create issue: {}", issue.title))?;
    Ok(issue)
  }

  pub async fn close_issue(&self, issue: &Issue) -> Result<()> {
    self
      .issue_handler()
      .update(issue.number)
      .state(IssueState::Closed)
      .send()
      .await
      .with_context(|| format!("Failed to close issue: {}", issue.number))?;
    Ok(())
  }

  pub async fn merge_pr(&self, pr: &PullRequest) -> Result<()> {
    self
      .pr_handler()
      .merge(pr.number)
      .send()
      .await
      .with_context(|| format!("Failed to merge PR: {}", pr.number))?;
    Ok(())
  }

  pub async fn delete(&self) -> Result<()> {
    self
      .repo_handler()
      .delete()
      .await
      .context("Failed to delete repo")?;
    Ok(())
  }
}

#[derive(Serialize, Deserialize, Type, Debug, Clone)]
#[serde(tag = "type", content = "value")]
pub enum GithubToken {
  Found(String),
  NotFound,
  Error(String),
}

macro_rules! token_try {
  ($e:expr) => {{
    match $e {
      Ok(x) => x,
      Err(e) => return GithubToken::Error(format!("{e:?}")),
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
  let res = command("gh auth token", &env::current_dir().unwrap()).output();
  match res {
    Ok(token_output) if token_output.status.success() => {
      let token = token_try!(String::from_utf8(token_output.stdout));
      let token_clean = token.trim_end().to_string();
      GithubToken::Found(token_clean)
    }
    _ => GithubToken::NotFound,
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
    .build()
    .context("Failed to build Github connector")?;
  octocrab::initialise(crab_inst);
  Ok(())
}
