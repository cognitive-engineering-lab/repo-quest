use anyhow::{anyhow, Result};
use async_trait::async_trait;
use octocrab::models::issues::Issue;
use std::path::Path;

use crate::{
  git::{GitRepo, MergeType},
  github::{find_issue, find_pr, FullPullRequest, GithubRepo, PullSelector},
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
pub trait QuestTemplate: Send + Sync + 'static {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs>;
  fn pull_request(&self, selector: &PullSelector) -> Result<FullPullRequest>;
  fn issue(&self, label: &str) -> Result<Issue>;
  fn apply_patch(
    &self,
    repo: &GitRepo,
    base_branch: &str,
    target_branch: &str,
  ) -> Result<MergeType>;
  fn reference_solution_pr_url(&self, stage: &Stage) -> Option<String>;
}

pub struct RepoTemplate(pub GithubRepo);

#[async_trait]
impl QuestTemplate for RepoTemplate {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs> {
    let origin = GithubRepo::instantiate_from_repo(&self.0).await?;
    let origin_git = origin.clone(path)?;
    origin_git.setup_upstream(&self.0)?;
    let config = QuestConfig::load(&origin_git, "upstream")?;
    Ok(InstanceOutputs {
      origin,
      origin_git,
      config,
    })
  }

  fn pull_request(&self, selector: &PullSelector) -> Result<FullPullRequest> {
    let pr = self.0.pr(selector).ok_or(anyhow!("Missing PR"))?;
    Ok((*pr).clone())
  }

  fn issue(&self, label: &str) -> Result<Issue> {
    let issue = self
      .0
      .issue(label)
      .ok_or_else(|| anyhow!("Missing issue for label: {label}"))?;
    Ok((*issue).clone())
  }

  fn apply_patch(
    &self,
    repo: &GitRepo,
    base_branch: &str,
    target_branch: &str,
  ) -> Result<MergeType> {
    repo.cherry_pick(base_branch, target_branch)
  }

  fn reference_solution_pr_url(&self, stage: &Stage) -> Option<String> {
    self
      .0
      .pr(&PullSelector::Branch(
        stage.branch_name(StagePart::Solution),
      ))
      .map(|pr| pr.data.html_url.as_ref().unwrap().to_string())
  }
}

pub struct PackageTemplate(pub QuestPackage);

#[async_trait]
impl QuestTemplate for PackageTemplate {
  async fn instantiate(&self, path: &Path) -> Result<InstanceOutputs> {
    let origin = GithubRepo::instantiate_from_package(&self.0).await?;
    let origin_git = origin.clone(path)?;
    origin_git.write_initial_files(&self.0)?;
    let config = self.0.config.clone();
    Ok(InstanceOutputs {
      origin,
      origin_git,
      config,
    })
  }

  fn pull_request(&self, selector: &PullSelector) -> Result<FullPullRequest> {
    let index = find_pr(selector, &self.0.prs)
      .ok_or_else(|| anyhow!("Missing PR for selector: {selector:?}"))?;
    Ok(self.0.prs[index].clone())
  }

  fn issue(&self, label: &str) -> Result<Issue> {
    let index = find_issue(label, &self.0.issues)
      .ok_or_else(|| anyhow!("Missing issue for label: {label}"))?;
    Ok(self.0.issues[index].clone())
  }

  fn apply_patch(
    &self,
    repo: &GitRepo,
    base_branch: &str,
    target_branch: &str,
  ) -> Result<MergeType> {
    let patch_index = self
      .0
      .patch(&(base_branch.to_string(), target_branch.to_string()))
      .ok_or_else(|| anyhow!("Missing patch in package: {base_branch}..{target_branch}"))?;

    let patches = self.0.patches[..=patch_index]
      .iter()
      .map(|patch| patch.patch.as_str())
      .collect::<Vec<_>>();

    repo.apply_patch(&patches)
  }

  fn reference_solution_pr_url(&self, _stage: &Stage) -> Option<String> {
    None
  }
}
