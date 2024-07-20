use std::{collections::HashMap, env, fs, path::Path, process::Command, time::Duration};

use crate::{
  git_repo::GitRepo,
  github_repo::GithubRepo,
  stage::{Stage, StageConfig, StagePart, StagePartStatus},
};
use anyhow::{ensure, Context, Result};
use dioxus::signals::{SyncSignal, Writable};
use futures_util::future::try_join;
use http::StatusCode;
use octocrab::{
  models::{pulls::PullRequest, IssueState},
  params::{issues, pulls, Direction},
  GitHubError,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuestConfig {
  pub title: String,
  pub author: String,
  pub repo: String,
  pub stages: Vec<StageConfig>,
}

#[derive(Clone, Debug)]
pub struct QuestState {
  pub stage: Stage,
  pub part: StagePart,
  pub status: StagePartStatus,
}

pub struct Quest {
  user: String,
  upstream: GithubRepo,
  origin: GithubRepo,
  origin_git: GitRepo,
  stage_index: HashMap<String, usize>,

  pub config: QuestConfig,
  pub state_signal: SyncSignal<Option<QuestState>>,
  pub stages: Vec<Stage>,
}

pub fn load_config_from_current_dir() -> Result<QuestConfig> {
  let output = Command::new("git")
    .args(["rev-parse", "--show-toplevel"])
    .output()
    .context("git failed")?;
  ensure!(
    output.status.success(),
    "git exited with non-zero status code"
  );
  let stdout = String::from_utf8(output.stdout)?.trim().to_string();

  let config_path = Path::new(&stdout).join(".rqst.toml");
  let config_contents = fs::read_to_string(&config_path)
    .with_context(|| format!("Failed to read config: {}", config_path.display()))?;
  let config = toml::de::from_str::<QuestConfig>(&config_contents)?;

  Ok(config)
}

pub async fn load_config_from_remote(owner: &str, repo: &str) -> Result<QuestConfig> {
  let items = octocrab::instance()
    .repos(owner, repo)
    .get_content()
    .path(".rqst.toml")
    .r#ref("main")
    .send()
    .await?;
  let config_contents = items.items[0].decoded_content().expect("Missing content");
  let config = toml::de::from_str::<QuestConfig>(&config_contents)?;
  Ok(config)
}

async fn load_user() -> Result<String> {
  let user = octocrab::instance()
    .current()
    .user()
    .await
    .context("Failed to get current user")?;
  Ok(user.login)
}

impl Quest {
  pub async fn load(
    config: QuestConfig,
    state_signal: SyncSignal<Option<QuestState>>,
  ) -> Result<Self> {
    let user = load_user().await?;
    let upstream = GithubRepo::new(&config.author, &config.repo);
    let origin = GithubRepo::new(&user, &config.repo);
    let origin_git = GitRepo::new();
    let stages = config
      .stages
      .iter()
      .enumerate()
      .map(|(i, stage)| Stage::new(i, stage.clone()))
      .collect::<Vec<_>>();
    let stage_index = stages
      .iter()
      .map(|stage| (stage.config.label.clone(), stage.idx))
      .collect::<HashMap<_, _>>();

    let q = Quest {
      user,
      config,
      upstream,
      origin,
      origin_git,
      stage_index,
      stages,
      state_signal,
    };
    q.infer_state_update().await?;
    Ok(q)
  }

  fn parse_stage(&self, pr: &PullRequest) -> Option<(Stage, StagePart)> {
    let branch = &pr.head.ref_field;
    let re = Regex::new("^(.*)-([abc])$").unwrap();
    let (_, [name, part_str]) = re.captures(branch)?.extract();
    let stage = self.stage_index.get(name)?;
    let part = StagePart::parse(part_str)?;
    Some((self.stages[*stage].clone(), part))
  }

  async fn infer_state(&self) -> Result<QuestState> {
    let pr_handler = self.origin.pr_handler();
    let pr_page_future = pr_handler
      .list()
      .state(octocrab::params::State::All)
      .sort(pulls::Sort::Created)
      .direction(Direction::Descending)
      .per_page(10)
      .send();

    let issue_handler = self.origin.issue_handler();
    let issue_page_future = issue_handler
      .list()
      .state(octocrab::params::State::All)
      .sort(issues::Sort::Created)
      .direction(Direction::Descending)
      .per_page(10)
      .send();

    let (mut pr_page, mut issue_page) = match try_join(pr_page_future, issue_page_future).await {
      Ok(result) => result,
      Err(octocrab::Error::GitHub {
        source: GitHubError {
          status_code: StatusCode::NOT_FOUND,
          ..
        },
        ..
      }) => {
        return Ok(QuestState {
          stage: self.stages[0].clone(),
          part: StagePart::Feature,
          status: StagePartStatus::Start,
        })
      }
      Err(e) => return Err(e.into()),
    };

    let prs = pr_page.take_items();
    let issues = issue_page.take_items();

    let Some((stage, part, finished)) = prs
      .iter()
      .filter_map(|pr| {
        let (stage, part) = self.parse_stage(pr)?;
        let finished = pr.merged_at.is_some()
          && match part {
            StagePart::Solution => {
              let issue = issues.iter().find(|issue| {
                issue
                  .labels
                  .iter()
                  .any(|label| label.name == stage.config.label)
              })?;
              matches!(issue.state, IssueState::Closed)
            }
            StagePart::Feature | StagePart::Test => true,
          };
        Some((stage, part, finished))
      })
      .max_by_key(|(stage, part, _)| (stage.idx, *part))
    else {
      return Ok(QuestState {
        stage: self.stages[0].clone(),
        part: StagePart::Feature,
        status: StagePartStatus::Start,
      });
    };

    Ok(if finished {
      match part.next_part() {
        Some(next_part) => QuestState {
          stage,
          part: next_part,
          status: StagePartStatus::Start,
        },
        None => QuestState {
          stage: self.stages[stage.idx + 1].clone(),
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

  pub async fn infer_state_update(&self) -> Result<()> {
    let new_state = self.infer_state().await?;
    let mut state_signal = self.state_signal;
    state_signal.set(Some(new_state));
    Ok(())
  }

  pub async fn infer_state_loop(&self) {
    loop {
      self.infer_state_update().await.unwrap();
      sleep(Duration::from_secs(10)).await;
    }
  }

  fn clone_repo(&self) -> Result<()> {
    let url = format!("git@github.com:{}/{}.git", self.user, self.config.repo);
    self.origin_git.clone(&url)
  }

  pub async fn create_repo(&self) -> Result<()> {
    self.origin.copy_from(&self.upstream).await?;
    self.clone_repo()?;
    env::set_current_dir(&self.config.repo)?;
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

    let issue = self.upstream.issue(&stage.config.label).await.unwrap();
    self.origin.copy_issue(issue).await?;

    self.infer_state_update().await?;

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

    self.infer_state_update().await?;

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

    self.infer_state_update().await?;

    Ok(())
  }
}
