use std::{env, time::Duration};

use crate::{
  git_repo::GitRepo,
  github_repo::GithubRepo,
  stage::{Stage, StagePart, StagePartStatus},
};
use anyhow::{Context, Result};
use dioxus::signals::{SyncSignal, Writable};
use octocrab::{
  models::IssueState,
  params::{issues, pulls, Direction},
};
use regex::Regex;
use tokio::time::sleep;
use tracing::debug;

#[derive(Clone, Debug)]
pub struct QuestState {
  pub stage: Stage,
  pub part: StagePart,
  pub status: StagePartStatus,
}

pub struct Quest {
  user: String,
  quest: String,
  upstream: GithubRepo,
  origin: GithubRepo,
  origin_git: GitRepo,
  pub stages: Vec<Stage>,
  pub state: SyncSignal<Option<QuestState>>,
}

const UPSTREAM_ORG: &str = "cognitive-engineering-lab";

impl Quest {
  pub async fn load(
    user: &str,
    quest: &str,
    state: SyncSignal<Option<QuestState>>,
  ) -> Result<Self> {
    let upstream = GithubRepo::new(UPSTREAM_ORG, quest);
    let origin = GithubRepo::new(user, quest);
    let origin_git = GitRepo::new();
    let stages = Self::infer_stages(&upstream).await?;
    debug!("Inferred state: {state:#?}");

    let q = Quest {
      user: user.to_string(),
      quest: quest.to_string(),
      upstream,
      origin,
      origin_git,
      stages,
      state,
    };
    Ok(q)
  }

  async fn infer_stages(upstream: &GithubRepo) -> Result<Vec<Stage>> {
    let branches = upstream.branches().await?;
    let re = Regex::new(r"^(\d+)\w-([\d\w-]+)$").unwrap();
    let mut stages = branches
      .iter()
      .filter_map(|branch| {
        let cap = re.captures(&branch.name)?;
        let (_, [number, name]) = cap.extract();
        Some((number, name))
      })
      .map(|(number, name)| {
        let number = number.parse::<usize>()?;
        Ok(Stage::new(number, name))
      })
      .collect::<Result<Vec<_>>>()?;
    stages.dedup();
    stages.sort_by_key(|stage| stage.idx());
    Ok(stages)
  }

  async fn infer_state(origin: &GithubRepo, stages: &[Stage]) -> QuestState {
    async fn inner(origin: &GithubRepo, stages: &[Stage]) -> Result<QuestState> {
      let mut pr_page = origin
        .pr_handler()
        .list()
        .state(octocrab::params::State::All)
        .sort(pulls::Sort::Created)
        .direction(Direction::Descending)
        .per_page(10)
        .send()
        .await?;
      let prs = pr_page.take_items();

      let mut issue_page = origin
        .issue_handler()
        .list()
        .state(octocrab::params::State::All)
        .sort(issues::Sort::Created)
        .direction(Direction::Descending)
        .per_page(10)
        .send()
        .await?;
      let issues = issue_page.take_items();

      let (stage, part, finished) = prs
        .iter()
        .filter_map(|pr| {
          let (stage, part) = Stage::parse(&pr.head.ref_field)?;
          let finished = matches!(pr.state, Some(IssueState::Closed))
            && match part {
              StagePart::Solution => {
                let issue = issues.iter().find(|issue| {
                  issue
                    .labels
                    .iter()
                    .any(|label| label.name == stage.issue_label())
                })?;
                matches!(issue.state, IssueState::Closed)
              }
              StagePart::Feature | StagePart::Test => true,
            };
          Some((stage, part, finished))
        })
        .max_by_key(|(stage, part, _)| (stage.clone(), *part))
        .context("No PRs")?;

      Ok(if finished {
        match part.next_part() {
          Some(next_part) => QuestState {
            stage,
            part: next_part,
            status: StagePartStatus::Start,
          },
          None => QuestState {
            stage: stages[stage.idx() + 1].clone(),
            part: StagePart::Feature,
            status: StagePartStatus::Start,
          },
        }
      } else {
        QuestState {
          stage,
          part,
          status: StagePartStatus::Ongoing,
        }
      })
    }

    match inner(origin, stages).await {
      Ok(state) => state,
      Err(_) => QuestState {
        stage: stages[0].clone(),
        part: StagePart::Feature,
        status: StagePartStatus::Start,
      },
    }
  }

  pub async fn infer_state_update(&self) {
    let new_state = Self::infer_state(&self.origin, &self.stages).await;
    let mut state = self.state;
    state.set(Some(new_state));
  }

  pub async fn infer_state_loop(&self) {
    loop {
      self.infer_state_update().await;
      sleep(Duration::from_secs(10)).await;
    }
  }

  fn clone_repo(&self) -> Result<()> {
    let url = format!("git@github.com:{}/{}.git", self.user, self.quest);
    self.origin_git.clone(&url)
  }

  pub async fn create_repo(&self) -> Result<()> {
    self.origin.copy_from(&self.upstream).await?;
    self.clone_repo()?;
    env::set_current_dir(&self.quest)?;
    self.origin_git.initialize(&self.upstream)?;
    Ok(())
  }

  async fn file_pr(&self, target_branch: &str, base_branch: &str) -> Result<()> {
    self.origin_git.checkout_main_and_pull()?;

    self
      .origin_git
      .create_branch_from(target_branch, base_branch)?;

    let head = self.origin_git.head_commit()?;

    let pr = self.upstream.pr(target_branch).await.unwrap();
    self.origin.copy_pr(&self.upstream, pr, &head).await?;

    self.infer_state_update().await;

    Ok(())
  }

  pub async fn file_feature_and_issue(&self, stage_index: usize) -> Result<()> {
    let stage = &self.stages[stage_index];
    let base_branch = if stage_index > 0 {
      let prev_stage = &self.stages[stage_index - 1];
      prev_stage.branch_name(StagePart::Solution)
    } else {
      "main".into()
    };

    self
      .file_pr(&stage.branch_name(StagePart::Feature), &base_branch)
      .await?;

    let issue = self.upstream.issue(&stage.issue_label()).await.unwrap();
    self.origin.copy_issue(issue).await?;

    self.infer_state_update().await;

    Ok(())
  }

  pub async fn file_tests(&self, stage_index: usize) -> Result<()> {
    let stage = &self.stages[stage_index];
    self
      .file_pr(
        &stage.branch_name(StagePart::Test),
        &stage.branch_name(StagePart::Feature),
      )
      .await?;

    self.infer_state_update().await;

    Ok(())
  }

  pub async fn file_solution(&self, stage_index: usize) -> Result<()> {
    let stage = &self.stages[stage_index];
    self
      .file_pr(
        &stage.branch_name(StagePart::Solution),
        &stage.branch_name(StagePart::Test),
      )
      .await?;

    self.infer_state_update().await;

    Ok(())
  }
}
