use crate::{git_repo::GitRepo, github_repo::GithubRepo, stage::Stage};
use anyhow::Result;

pub struct Quest {
  upstream: GithubRepo,
  origin: GithubRepo,
  origin_git: GitRepo,
}

impl Quest {
  pub fn new(user: &str, quest: &str) -> Self {
    let upstream = GithubRepo::new("cognitive-engineering-lab", quest);
    let origin = GithubRepo::new(user, quest);
    let origin_git = GitRepo::new();

    Quest {
      upstream,
      origin,
      origin_git,
    }
  }

  pub async fn create_repo(&self) -> Result<()> {
    self.origin.copy_from(&self.upstream).await
  }

  pub fn init_repo(&self) -> Result<()> {
    self.origin_git.initialize(&self.upstream)
  }

  async fn file_pr(&self, target_branch: &str, base_branch: &str) -> Result<()> {
    self
      .origin_git
      .create_branch_from(target_branch, base_branch)?;

    let head = self.origin_git.head_commit()?;

    let pr = self.upstream.pr(target_branch).await.unwrap();
    self.origin.copy_pr(&self.upstream, pr, &head).await?;

    Ok(())
  }

  pub async fn file_feature_and_issue(
    &self,
    next_stage: &Stage,
    prev_stage: Option<&Stage>,
  ) -> Result<()> {
    let base_branch = match prev_stage {
      Some(stage) => stage.solution_pr(),
      None => "main".into(),
    };

    self.file_pr(&next_stage.feature_pr(), &base_branch).await?;

    let issue = self
      .upstream
      .issue(&next_stage.issue_label())
      .await
      .unwrap();
    self.origin.copy_issue(issue).await?;

    Ok(())
  }

  pub async fn file_tests(&self, stage: &Stage) -> Result<()> {
    self.file_pr(&stage.test_pr(), &stage.feature_pr()).await
  }
}
