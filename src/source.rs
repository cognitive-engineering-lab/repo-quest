use async_trait::async_trait;
use eyre::{Context, Result, bail, eyre};
use octocrab::models::{
  issues::Issue,
  pulls::{self, PullRequest},
};
use std::path::Path;

use crate::{
  git::{GitRepo, MergeType, UPSTREAM},
  github::{GithubRepo, find_issue, find_pr},
  package::QuestPackage,
  quest::QuestConfig,
  stage::{Stage, StagePart},
};

pub struct InstanceOutputs {
  pub origin: GithubRepo,
  pub origin_git: GitRepo,
  pub config: QuestConfig,
}

#[async_trait]
pub trait QuestSource: Send + Sync + 'static {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs>;
  fn pull_request(&self, branch: &str) -> Option<PullRequest>;
  async fn pull_request_comments(&self, selector: &PullRequest) -> Result<Vec<pulls::Comment>>;
  fn issue(&self, label: &str) -> Result<Issue>;
  fn apply_patch(&self, repo: &GitRepo, from: &str, to: &str) -> Result<MergeType>;
  fn provides_refsol(&self) -> bool;
  fn refsol_url(&self, stage: &Stage) -> Option<String>;
  fn can_skip(&self) -> bool;
}

pub struct RepoSource(pub GithubRepo);

#[async_trait]
impl QuestSource for RepoSource {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs> {
    let origin = GithubRepo::instantiate_from_repo(&self.0)
      .await
      .context("Failed to instantiate Github repo from template")?;
    let origin_git = origin.clone(path).context("Failed to clone Github repo")?;
    origin_git
      .setup_upstream(&self.0)
      .context("Failed to setup upstream")?;
    let config = QuestConfig::load(&origin_git, Some("upstream"))
      .context("Failed to load quest config from upstream")?;
    Ok(InstanceOutputs {
      origin,
      origin_git,
      config,
    })
  }

  fn pull_request(&self, branch: &str) -> Option<PullRequest> {
    let pr = self.0.pr(branch)?;
    Some(pr.clone())
  }

  async fn pull_request_comments(&self, pr: &PullRequest) -> Result<Vec<pulls::Comment>> {
    self.0.pr_comments(pr).await
  }

  fn issue(&self, label: &str) -> Result<Issue> {
    let issue = self
      .0
      .issue(label)
      .ok_or_else(|| eyre!("Missing issue for label: {label}"))?;
    Ok((*issue).clone())
  }

  fn apply_patch(&self, repo: &GitRepo, from: &str, to: &str) -> Result<MergeType> {
    repo.cherry_pick(&format!("{UPSTREAM}/{from}"), &format!("{UPSTREAM}/{to}"))
  }

  fn provides_refsol(&self) -> bool {
    true
  }

  fn refsol_url(&self, stage: &Stage) -> Option<String> {
    self
      .0
      .pr(&stage.branch_name(StagePart::Solution))
      .map(|pr| pr.html_url.as_ref().unwrap().to_string())
  }

  fn can_skip(&self) -> bool {
    true
  }
}

pub struct PackageSource(pub QuestPackage);

#[async_trait]
impl QuestSource for PackageSource {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs> {
    let origin = GithubRepo::instantiate_from_package(&self.0)
      .await
      .context("Failed to instantiate repo from package")?;
    let origin_git = origin.clone(path).context("Failed to clone repo")?;
    origin_git
      .write_initial_files(&self.0)
      .context("Failed to write starter code to new repo")?;
    let config = self.0.config.clone();
    Ok(InstanceOutputs {
      origin,
      origin_git,
      config,
    })
  }

  fn pull_request(&self, branch: &str) -> Option<PullRequest> {
    let index = find_pr(branch, self.0.prs.iter().map(|pr| &pr.data))?;
    Some(self.0.prs[index].data.clone())
  }

  async fn pull_request_comments(&self, pr: &PullRequest) -> Result<Vec<pulls::Comment>> {
    let full_pr = self
      .0
      .prs
      .iter()
      .find(|full_pr| full_pr.data.number == pr.number)
      .ok_or_else(|| eyre!("Missing comments for PR #{}", pr.number))?;
    Ok(full_pr.comments.clone())
  }

  fn issue(&self, label: &str) -> Result<Issue> {
    let index =
      find_issue(label, &self.0.issues).ok_or_else(|| eyre!("Missing issue for label: {label}"))?;
    Ok(self.0.issues[index].clone())
  }

  fn apply_patch(
    &self,
    repo: &GitRepo,
    base_branch: &str,
    target_branch: &str,
  ) -> Result<MergeType> {
    let Some(patch_index) = self
      .0
      .patch(&(base_branch.to_string(), target_branch.to_string()))
    else {
      bail!("Missing patch in package: {base_branch}..{target_branch}")
    };

    let patches = self.0.patches[..=patch_index]
      .iter()
      .map(|patch| patch.patch.as_str())
      .collect::<Vec<_>>();

    repo.apply_patch(&patches)
  }

  fn provides_refsol(&self) -> bool {
    false
  }

  fn refsol_url(&self, _stage: &Stage) -> Option<String> {
    None
  }

  fn can_skip(&self) -> bool {
    false
  }
}
