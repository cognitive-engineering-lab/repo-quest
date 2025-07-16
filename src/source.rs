use async_trait::async_trait;
use eyre::{Context, Result, bail, eyre};
use octocrab::models::{
  issues::Issue,
  pulls::{self, PullRequest},
};
use std::path::Path;

use crate::{
  chapter::{Chapter, ChapterPart},
  git::{Branch, GitRepo, MergeType},
  github::{GitProtocol, GithubRepo, find_issue, find_pr},
  package::QuestPackage,
  quest::QuestConfig,
};

pub struct InstanceOutputs {
  pub origin: GithubRepo,
  pub origin_git: GitRepo,
  pub config: QuestConfig,
}

#[async_trait]
pub trait QuestSource: Send + Sync + 'static {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs>;
  fn pull_request(&self, branch: &Branch) -> Option<PullRequest>;
  async fn pull_request_comments(&self, selector: &PullRequest) -> Result<Vec<pulls::Comment>>;
  fn issue(&self, label: &str) -> Result<Issue>;
  fn apply_patch(&self, repo: &GitRepo, from: &Branch, to: &Branch) -> Result<MergeType>;
  fn provides_refsol(&self) -> bool;
  fn refsol_url(&self, chapter: &Chapter) -> Option<String>;
  fn can_skip(&self) -> bool;
}

#[async_trait]
impl QuestSource for GithubRepo {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs> {
    let origin = GithubRepo::instantiate_from_repo(self)
      .await
      .context("Failed to instantiate Github repo from template")?;
    let origin_git = origin.clone(path).context("Failed to clone Github repo")?;
    origin_git
      .setup_upstream(&self.remote(GitProtocol::Https))
      .context("Failed to setup upstream")?;
    let config = QuestConfig::load(&origin_git, Some("upstream"))
      .context("Failed to load quest config from upstream")?;
    Ok(InstanceOutputs {
      origin,
      origin_git,
      config,
    })
  }

  fn pull_request(&self, branch: &Branch) -> Option<PullRequest> {
    let pr = self.pr(branch)?;
    Some(pr.clone())
  }

  async fn pull_request_comments(&self, pr: &PullRequest) -> Result<Vec<pulls::Comment>> {
    self.pr_comments(pr).await
  }

  fn issue(&self, label: &str) -> Result<Issue> {
    let issue = self
      .issue(label)
      .ok_or_else(|| eyre!("Missing issue for label: {label}"))?;
    Ok((*issue).clone())
  }

  fn apply_patch(&self, repo: &GitRepo, from: &Branch, to: &Branch) -> Result<MergeType> {
    let upstream = repo.upstream()?.expect("Repo is missing upstream");
    repo.cherry_pick_with_fallback(&format!("{upstream}/{from}"), &format!("{upstream}/{to}"))
  }

  fn provides_refsol(&self) -> bool {
    true
  }

  fn refsol_url(&self, chapter: &Chapter) -> Option<String> {
    self
      .pr(&chapter.branch(ChapterPart::Solution))
      .map(|pr| pr.html_url.as_ref().unwrap().to_string())
  }

  fn can_skip(&self) -> bool {
    true
  }
}

#[async_trait]
impl QuestSource for QuestPackage {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs> {
    let origin = GithubRepo::instantiate_from_package(self)
      .await
      .context("Failed to instantiate repo from package")?;
    let origin_git = origin.clone(path).context("Failed to clone repo")?;
    origin_git
      .write_initial_files(self)
      .context("Failed to write starter code to new repo")?;
    let config = self.config.clone();
    Ok(InstanceOutputs {
      origin,
      origin_git,
      config,
    })
  }

  fn pull_request(&self, branch: &Branch) -> Option<PullRequest> {
    let index = find_pr(branch, self.prs.iter().map(|pr| &pr.data))?;
    Some(self.prs[index].data.clone())
  }

  async fn pull_request_comments(&self, pr: &PullRequest) -> Result<Vec<pulls::Comment>> {
    let full_pr = self
      .prs
      .iter()
      .find(|full_pr| full_pr.data.number == pr.number)
      .ok_or_else(|| eyre!("Missing comments for PR #{}", pr.number))?;
    Ok(full_pr.comments.clone())
  }

  fn issue(&self, label: &str) -> Result<Issue> {
    let index =
      find_issue(label, &self.issues).ok_or_else(|| eyre!("Missing issue for label: {label}"))?;
    Ok(self.issues[index].clone())
  }

  fn apply_patch(
    &self,
    repo: &GitRepo,
    base_branch: &Branch,
    target_branch: &Branch,
  ) -> Result<MergeType> {
    let Some(patch_index) = self.patch(&(base_branch.clone(), target_branch.clone())) else {
      bail!("Missing patch in package: {base_branch}..{target_branch}")
    };

    let patches = self.patches[..=patch_index]
      .iter()
      .map(|patch| patch.patch.as_str())
      .collect::<Vec<_>>();

    repo.apply_patch_with_fallback(&patches)
  }

  fn provides_refsol(&self) -> bool {
    false
  }

  fn refsol_url(&self, _chapter: &Chapter) -> Option<String> {
    None
  }

  fn can_skip(&self) -> bool {
    false
  }
}
