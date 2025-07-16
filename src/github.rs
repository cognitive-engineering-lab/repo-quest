use eyre::{Context, Result, bail, ensure};
use futures_util::future::try_join_all;
use http::StatusCode;
use octocrab::{
  GitHubError, Octocrab,
  issues::IssueHandler,
  models::{
    IssueState, Label,
    issues::Issue,
    pulls::{self, PullRequest},
  },
  pulls::PullRequestHandler,
  repos::RepoHandler,
};
use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, fs, path::Path, sync::Arc};
use tokio::try_join;
use tracing::warn;

use crate::{
  command::command,
  git::{Branch, GitRepo, Ref},
  package::QuestPackage,
  utils::{self, RetryError},
};

pub struct GithubRepo {
  user: String,
  name: String,
  gh: Arc<Octocrab>,
  prs: Mutex<Option<Vec<PullRequest>>>,
  issues: Mutex<Option<Vec<Issue>>>,
}

#[derive(Debug)]
pub enum PullSelector {
  Branch(String),
  Label(String),
}

pub fn find_pr<'a>(
  branch: &Branch,
  prs: impl IntoIterator<Item = &'a PullRequest> + 'a,
) -> Option<usize> {
  prs
    .into_iter()
    .position(|pr| pr.head.ref_field == branch.as_str())
}

pub fn find_issue<'a>(
  label_name: &str,
  issues: impl IntoIterator<Item = &'a Issue> + 'a,
) -> Option<usize> {
  issues
    .into_iter()
    .position(|issue| issue.labels.iter().any(|label| label.name == label_name))
}

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
      if stderr.contains("git@github.com: Permission denied (publickey).") {
        bail!(
          "Your machine is not setup for a secure connection to Github. Please follow the instructions here: https://docs.github.com/en/authentication/troubleshooting-ssh/error-permission-denied-publickey"
        )
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
pub enum TestResult {
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

  /// Returns true if the repo is registered w/ github
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
      Err(octocrab::Error::GitHub { source, .. })
        if matches!(
          &*source,
          GitHubError {
            status_code: StatusCode::NOT_FOUND,
            ..
          },
        ) =>
      {
        return Ok(false);
      }
      Err(e) => return Err(e.into()),
    };
    let (prs, mut issues) = (pr_page.take_items(), issue_page.take_items());

    // Pull requests are considered issues, so filter them out
    issues.retain(|issue| issue.pull_request.is_none());

    *self.prs.lock() = Some(prs);
    *self.issues.lock() = Some(issues);

    Ok(true)
  }

  pub fn remote(&self, protocol: GitProtocol) -> String {
    match protocol {
      GitProtocol::Https => format!("https://github.com/{}/{}", self.user, self.name),
      GitProtocol::Ssh => format!("git@github.com:{}/{}.git", self.user, self.name),
    }
  }

  async fn test_repo(&self) -> Result<TestResult> {
    let result = self.repo_handler().list_commits().send().await;
    match result {
      Err(octocrab::Error::GitHub { source, .. })
        if matches!(
          *source,
          GitHubError {
            status_code: StatusCode::NO_CONTENT | StatusCode::CONFLICT,
            ..
          }
        ) =>
      {
        Ok(TestResult::NoContent)
      }

      Err(octocrab::Error::GitHub { source, .. })
        if matches!(
          *source,
          GitHubError {
            status_code: StatusCode::NOT_FOUND,
            ..
          }
        ) =>
      {
        Ok(TestResult::NotFound)
      }

      Ok(_) => Ok(TestResult::HasContent),

      Err(e) => {
        if let octocrab::Error::GitHub { source, .. } = &e {
          tracing::debug!("Error: {:?}", source.status_code);
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
  async fn wait_for_content(&self, expected: TestResult) -> Result<()> {
    utils::retry_with_timeout(async || match self.test_repo().await {
      Ok(actual) => {
        if actual == expected {
          Ok(())
        } else {
          Err(RetryError::Wait)
        }
      }
      Err(e) => Err(RetryError::Err(e)),
    })
    .await
  }

  async fn create_labels(&self, labels: &[Label]) -> Result<()> {
    let issues = self.issue_handler();
    let futs = labels.iter().filter(|label| !label.default).map(|label| {
      issues.create_label(
        &label.name,
        &label.color,
        label.description.as_deref().unwrap_or(""),
      )
    });
    try_join_all(futs)
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

  async fn configure(&self, labels: &[Label]) -> Result<()> {
    try_join!(self.unsubscribe(), self.create_labels(labels))?;
    Ok(())
  }

  pub async fn instantiate_from_package(package: &QuestPackage) -> Result<GithubRepo> {
    let user = load_user().await.context("Failed to load user")?;
    let name = &package.config.repo;
    let params = json!({
        "name": name,
        "private": true,
    });
    octocrab::instance()
      .post::<_, serde_json::Value>("/user/repos", Some(&params))
      .await
      .context("Failed to create repo")?;

    let repo = GithubRepo::new(&user, name);

    repo
      .wait_for_content(TestResult::NoContent)
      .await
      .context("Github repo was not properly initialized")?;

    repo
      .configure(&package.labels)
      .await
      .context("Failed to configure repo")?;

    Ok(repo)
  }

  async fn get_repo_labels(&self) -> Result<Vec<Label>> {
    let mut page = self
      .issue_handler()
      .list_labels_for_repo()
      .send()
      .await
      .context("Failed to fetch labels from repo")?;
    Ok(page.take_items())
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
      .wait_for_content(TestResult::HasContent)
      .await
      .context("Github repo was not properly initialized")?;

    let labels = base.get_repo_labels().await?;
    repo.configure(&labels).await?;

    Ok(repo)
  }

  pub fn repo_handler(&self) -> RepoHandler {
    self.gh.repos(&self.user, &self.name)
  }

  pub fn pr_handler(&self) -> PullRequestHandler {
    self.gh.pulls(&self.user, &self.name)
  }

  pub fn prs(&self) -> MappedMutexGuard<'_, Vec<PullRequest>> {
    MutexGuard::map(self.prs.lock(), |opt| {
      opt.as_mut().expect("PRs not populated")
    })
  }

  pub fn pr(&self, branch: &Branch) -> Option<MappedMutexGuard<'_, PullRequest>> {
    let prs = self.prs();
    let idx = find_pr(branch, prs.iter())?;
    Some(MappedMutexGuard::map(prs, |prs| &mut prs[idx]))
  }

  pub async fn pr_comments(&self, pr: &PullRequest) -> Result<Vec<pulls::Comment>> {
    let comment_pages = self
      .pr_handler()
      .list_comments(Some(pr.number))
      .send()
      .await
      .with_context(|| format!("Failed to fetch comments for PR {}", pr.number))?;
    let comments = comment_pages.into_iter().collect::<Vec<_>>();
    Ok(comments)
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

  #[tracing::instrument(skip(self, body, comments))]
  pub async fn file_pr(
    &self,
    title: &str,
    body: &str,
    labels: &[String],
    head_ref: &Branch,
    head_commit: &Ref,
    comments: &[pulls::Comment],
  ) -> Result<PullRequest> {
    let pulls = self.pr_handler();
    let request = pulls
      .create(title, head_ref.as_str(), Branch::main().as_str())
      .body(body);
    let mut pr = request.send().await.context("Failed to create PR")?;

    let add_labels = async {
      if !labels.is_empty() {
        let labels = self
          .issue_handler()
          .add_labels(pr.number, labels)
          .await
          .context("Failed to add labels to PR")?;
        pr.labels = Some(labels);
      }
      Ok::<_, eyre::Error>(())
    };

    let add_comments = try_join_all(
      comments
        .iter()
        .map(|comment| self.copy_pr_comment(pr.number, comment, head_commit)),
    );

    try_join!(add_labels, add_comments)?;

    self.prs.lock().as_mut().unwrap().push(pr.clone());

    Ok(pr)
  }

  async fn copy_pr_comment(&self, pr: u64, comment: &pulls::Comment, commit: &Ref) -> Result<()> {
    let route = format!("/repos/{}/{}/pulls/{pr}/comments", self.user, self.name);
    let comment_json = json!({
      "path": comment.path,
      "commit_id": commit.as_str(),
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

  fn update_md_links(&self, body: &str) -> String {
    let re = Regex::new(r"\{\{ (\S+) (\S+) \}\}").unwrap();
    let mut new_body = body.to_string();
    let substitutions = re.captures_iter(body).filter_map(|cap| {
      let full_match = cap.get(0).unwrap();
      let label = &cap[1];
      let kind = &cap[2];
      let number = match kind {
        "pr" => {
          let Some(pr) = self.pr(&Branch::new(label)) else {
            warn!("No PR for branch {label}");
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
        _ => panic!(
          "Unexpected RepoQuest Markdown link with kind `{kind}`: {}",
          full_match.as_str()
        ),
      };

      Some((full_match.range(), format!("#{number}")))
    });
    utils::replace_many_ranges(&mut new_body, substitutions);

    new_body
  }

  #[tracing::instrument(skip_all, fields(src_issue = src_issue.title))]
  pub async fn copy_issue(&self, src_issue: &Issue) -> Result<Issue> {
    let issue_handler = self.issue_handler();
    let body = src_issue.body.as_ref().unwrap();

    let label_names = src_issue
      .labels
      .iter()
      .map(|label| label.name.clone())
      .collect::<Vec<_>>();
    let req = issue_handler
      .create(&src_issue.title)
      .body(body)
      .labels(label_names);

    let new_issue = req
      .send()
      .await
      .with_context(|| format!("Failed to create issue: {}", src_issue.title))?;

    self.issues.lock().as_mut().unwrap().push(new_issue.clone());

    Ok(new_issue)
  }

  pub async fn update_issue_links(&self, issue: &Issue) -> Result<()> {
    let issue_handler = self.issue_handler();
    let new_body = self.update_md_links(issue.body.as_ref().unwrap());
    let update_req = issue_handler.update(issue.number).body(&new_body);
    update_req
      .send()
      .await
      .with_context(|| format!("Failed to update issue: {}", issue.number))?;

    // letself.issues.lock().as_mut().unwrap().iter().find(|i| i.number == issue.number).unwrap()

    Ok(())
  }

  pub async fn update_pr_links(&self, pr: &PullRequest) -> Result<()> {
    let pr_handler = self.pr_handler();
    let new_body = self.update_md_links(pr.body.as_ref().unwrap());
    let update_req = pr_handler.update(pr.number).body(&new_body);
    update_req
      .send()
      .await
      .with_context(|| format!("Failed to update PR: {}", pr.number))?;
    Ok(())
  }

  #[tracing::instrument(skip_all, fields(number = issue.number, title = issue.title))]
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

  pub async fn wait_for_issue_closed(&self, issue: &Issue) -> Result<()> {
    utils::retry_with_timeout(async || {
      let issue = self
        .issue_handler()
        .get(issue.number)
        .await
        .map_err(|e| RetryError::Err(e.into()))?;
      match issue.state {
        IssueState::Closed => Ok(()),
        _ => Err(RetryError::Wait),
      }
    })
    .await
  }

  #[tracing::instrument(skip_all, fields(number = pr.number, title = pr.title))]
  pub async fn merge_pr(&self, pr: &PullRequest) -> Result<()> {
    let pr_handler = self.pr_handler();

    // Both 405 and 409 seem to happen after a PR is filed and before Github
    // decides a PR is mergeable, so we try on a loop until those errors go away.
    utils::retry_with_timeout(async || {
      let req = pr_handler.merge(pr.number);
      let resp = req.send().await;
      resp.map_err(|e| match e {
        octocrab::Error::GitHub { source, .. }
          if matches!(
            &*source,
            GitHubError {
              status_code: StatusCode::CONFLICT | StatusCode::METHOD_NOT_ALLOWED,
              ..
            }
          ) =>
        {
          RetryError::Wait
        }
        e => {
          eprintln!("This error seems to be flaky, so printing full error in debug mode for diagnostics:\n{e:#?}");
          RetryError::Err(e.into())
        }
      })
    })
    .await
    .with_context(|| format!("Failed to merge PR: {}", pr.number))?;

    Ok(())
  }

  #[tracing::instrument(skip(self))]
  pub async fn delete(&self) -> Result<()> {
    self
      .repo_handler()
      .delete()
      .await
      .context("Failed to delete repo")?;
    Ok(())
  }

  pub async fn set_var(&self, key: &str, val: &str) -> Result<()> {
    let route = format!("/repos/{}/{}/actions/variables", self.user, self.name);
    let var_json = json!({
      "name": key,
      "value": val
    });
    let _response = self
      .gh
      .post::<_, serde_json::Value>(route, Some(&var_json))
      .await
      .with_context(|| format!("Failed to set variable: {key}={val}"))?;
    Ok(())
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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
  let home = match env::home_dir() {
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

fn get_github_token() -> Result<String> {
  let mut result = read_github_token_from_fs();
  if matches!(result, GithubToken::NotFound) {
    result = generate_github_token_from_cli();
  }
  match result {
    GithubToken::Found(token) => Ok(token),
    GithubToken::Error(err) => bail!("Failed with get Github token with error: {err}"),
    GithubToken::NotFound => bail!("Could not find Github token"),
  }
}

pub fn init_octocrab() -> Result<()> {
  let token = get_github_token()?;
  let crab_inst = Octocrab::builder()
    .personal_token(token.to_string())
    .build()
    .context("Failed to build Github connector")?;
  octocrab::initialise(crab_inst);
  Ok(())
}
