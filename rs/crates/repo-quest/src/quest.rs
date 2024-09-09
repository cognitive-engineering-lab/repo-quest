use std::{
  collections::HashMap,
  env::{self, set_current_dir},
  path::{Path, PathBuf},
  process::Command,
  time::Duration,
};

use crate::{
  git::{GitRepo, UPSTREAM},
  github::{GithubRepo, PullSelector},
  stage::{Stage, StagePart, StagePartStatus},
};
use anyhow::{ensure, Context, Result};
use http::StatusCode;
use octocrab::{
  models::{issues::Issue, pulls::PullRequest, IssueState},
  params::{issues, pulls, Direction},
  GitHubError,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::{time::sleep, try_join};

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct QuestConfig {
  pub title: String,
  pub author: String,
  pub repo: String,
  pub stages: Vec<Stage>,
}

#[derive(Serialize, Deserialize, Type, Clone)]
pub struct StageState {
  stage: Stage,
  issue_url: Option<String>,
  feature_pr_url: Option<String>,
  solution_pr_url: Option<String>,
  reference_solution_pr_url: Option<String>,
}

impl QuestConfig {
  pub fn load(dir: impl AsRef<Path>) -> Result<Self> {
    let output = Command::new("git")
      .arg("show")
      .arg(format!("{UPSTREAM}/meta:rqst.toml"))
      .current_dir(dir)
      .output()
      .context("git failed")?;
    ensure!(
      output.status.success(),
      "git exited with non-zero status code"
    );
    let stdout = String::from_utf8(output.stdout)?.trim().to_string();
    let config = toml::de::from_str::<QuestConfig>(&stdout)?;

    Ok(config)
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "type")]
pub enum QuestState {
  Ongoing {
    stage: u32,
    part: StagePart,
    status: StagePartStatus,
  },
  Completed,
}

pub struct Quest {
  user: String,
  upstream: GithubRepo,
  origin: GithubRepo,
  origin_git: GitRepo,
  stage_index: HashMap<String, usize>,
  dir: PathBuf,
  app: Option<AppHandle>,

  pub config: QuestConfig,
}

pub async fn load_config_from_remote(owner: &str, repo: &str) -> Result<QuestConfig> {
  let items = octocrab::instance()
    .repos(owner, repo)
    .get_content()
    .path("rqst.toml")
    .r#ref("meta")
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

#[derive(Serialize, Deserialize, Clone, Type)]
pub struct StateDescriptor {
  dir: PathBuf,
  stages: Vec<StageState>,
  state: QuestState,
}

#[derive(Serialize, Deserialize, Clone, Type, Event)]
pub struct StateEvent(StateDescriptor);

impl Quest {
  pub async fn load(dir: PathBuf, config: QuestConfig, app: Option<AppHandle>) -> Result<Self> {
    let user = load_user().await?;
    let upstream = GithubRepo::new(&config.author, &config.repo);
    let origin = GithubRepo::new(&user, &config.repo);
    let origin_git = GitRepo::new();
    let stage_index = config
      .stages
      .iter()
      .enumerate()
      .map(|(i, stage)| (stage.label.clone(), i))
      .collect::<HashMap<_, _>>();

    let q = Quest {
      dir,
      user,
      config,
      upstream,
      origin,
      origin_git,
      stage_index,
      app,
    };

    try_join!(q.origin.fetch(), q.upstream.fetch())?;

    // Need to infer_state_update after fetching repo data so issues/PRs are populated
    q.infer_state_update().await?;

    if q.dir.exists() {
      set_current_dir(&q.dir)?;
    } else {
      set_current_dir(q.dir.parent().unwrap())?;
    }

    Ok(q)
  }

  pub fn stages(&self) -> &[Stage] {
    &self.config.stages
  }

  fn stage(&self, idx: usize) -> &Stage {
    &self.config.stages[idx]
  }

  fn parse_stage(&self, pr: &PullRequest) -> Option<(Stage, StagePart)> {
    let branch = &pr.head.ref_field;
    let re = Regex::new("^(.*)-([abc])$").unwrap();
    let (_, [name, part_str]) = re.captures(branch)?.extract();
    let stage = self.stage_index.get(name)?;
    let part = StagePart::parse(part_str)?;
    Some((self.stage(*stage).clone(), part))
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

    let (mut pr_page, mut issue_page) = match try_join!(pr_page_future, issue_page_future) {
      Ok(result) => result,
      Err(octocrab::Error::GitHub {
        source: GitHubError {
          status_code: StatusCode::NOT_FOUND,
          ..
        },
        ..
      }) => {
        return Ok(QuestState::Ongoing {
          stage: 0,
          part: StagePart::Starter,
          status: StagePartStatus::Start,
        })
      }
      Err(e) => return Err(e.into()),
    };

    let prs = pr_page.take_items();
    let issues = issue_page.take_items();

    let issue_map = issues
      .into_iter()
      .filter_map(|issue| {
        let label = issue.labels.first()?;
        if issue.pull_request.is_none() {
          Some((label.name.clone(), issue))
        } else {
          None
        }
      })
      .collect::<HashMap<_, _>>();

    let stage_map = self
      .stages()
      .iter()
      .map(|stage| (stage.label.clone(), stage))
      .collect::<HashMap<_, _>>();

    let pr_stages = prs.iter().filter_map(|pr| {
      let (stage, part) = self.parse_stage(pr)?;
      let finished = pr.merged_at.is_some()
        && match part {
          StagePart::Solution => {
            let issue = issue_map.get(&stage.label)?;
            matches!(issue.state, IssueState::Closed)
          }
          StagePart::Starter => true,
        };
      Some((stage, part, finished))
    });

    let issue_stages = issue_map.iter().filter_map(|(label, issue)| {
      let stage = (*stage_map.get(label)?).clone();
      Some(if matches!(issue.state, IssueState::Closed) {
        (stage, StagePart::Solution, true)
      } else {
        let no_starter = stage.no_starter();
        (stage, StagePart::Starter, no_starter)
      })
    });

    tracing::trace!("PRs: {:#?}", pr_stages.clone().collect::<Vec<_>>());
    tracing::trace!("Issues: {:#?}", issue_stages.clone().collect::<Vec<_>>());

    let stage_idx = |stage: &Stage| self.stage_index[&stage.label];
    let Some((stage, part, finished)) = pr_stages
      .chain(issue_stages)
      .max_by_key(|(stage, part, finished)| (stage_idx(stage), *part, *finished))
    else {
      return Ok(QuestState::Ongoing {
        stage: 0,
        part: StagePart::Starter,
        status: StagePartStatus::Start,
      });
    };

    let stage = stage_idx(&stage);

    Ok(if finished {
      match part.next_part() {
        Some(next_part) => QuestState::Ongoing {
          stage: stage as u32,
          part: next_part,
          status: StagePartStatus::Start,
        },
        None => {
          if stage == self.stages().len() - 1 {
            QuestState::Completed
          } else {
            QuestState::Ongoing {
              stage: (stage + 1) as u32,
              part: StagePart::Starter,
              status: StagePartStatus::Start,
            }
          }
        }
      }
    } else {
      QuestState::Ongoing {
        stage: stage as u32,
        part,
        status: StagePartStatus::Ongoing,
      }
    })
  }

  pub async fn infer_state_update(&self) -> Result<()> {
    let (new_state, _) = try_join!(self.infer_state(), self.origin.fetch())?;
    if let Some(app) = &self.app {
      let descriptor = StateDescriptor {
        dir: self.dir.clone(),
        stages: self.stage_states(),
        state: new_state,
      };
      StateEvent(descriptor).emit(app)?;
    }

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
    // First instantiate the user's repo from the template repo on the server side
    self.origin.copy_from(&self.upstream).await?;

    // Then clone from server side to client side
    self.clone_repo()?;

    // Move into the repo
    env::set_current_dir(&self.config.repo)?;

    // Initialize the upstreams and fetch content
    self.origin_git.setup_upstream(&self.upstream)?;

    Ok(())
  }

  async fn file_pr(&self, target_branch: &str, base_branch: &str) -> Result<PullRequest> {
    self.origin_git.checkout_main_and_pull()?;

    let (branch_head, merge_type) = self
      .origin_git
      .create_branch_from(target_branch, base_branch)?;

    let pr = self
      .upstream
      .pr(&PullSelector::Branch(target_branch.into()))
      .unwrap()
      .clone();
    let new_pr = self
      .origin
      .copy_pr(&self.upstream, &pr, &branch_head, merge_type)
      .await?;

    tracing::debug!("Filed PR: {base_branch} -> {target_branch}");

    Ok(new_pr)
  }

  async fn file_issue(&self, stage_index: usize) -> Result<Issue> {
    let stage = self.stage(stage_index);
    let issue = self
      .upstream
      .issue(&stage.label)
      .unwrap_or_else(|| panic!("Missing issue for stage {}", stage.label))
      .clone();
    let new_issue = self.origin.copy_issue(&issue).await?;
    self.infer_state_update().await?;
    Ok(new_issue)
  }

  pub async fn file_feature_and_issue(
    &self,
    stage_index: usize,
  ) -> Result<(Option<PullRequest>, Issue)> {
    let stage = self.stage(stage_index);
    let base_branch = if stage_index > 0 {
      let prev_stage = self.stage(stage_index - 1);
      prev_stage.branch_name(StagePart::Solution)
    } else {
      "main".into()
    };

    let pr = if !stage.no_starter() {
      let pr = self
        .file_pr(&stage.branch_name(StagePart::Starter), &base_branch)
        .await?;
      Some(pr)
    } else {
      None
    };

    // Need to refresh our state for issues that refer to the filed PR
    self.infer_state_update().await?;

    let issue = self.file_issue(stage_index).await?;
    Ok((pr, issue))
  }

  pub async fn file_solution(&self, stage_index: usize) -> Result<PullRequest> {
    let stage = self.stage(stage_index);
    let pr = self
      .file_pr(
        &stage.branch_name(StagePart::Solution),
        &stage.branch_name(StagePart::Starter),
      )
      .await?;

    self.infer_state_update().await?;

    Ok(pr)
  }

  pub fn stage_states(&self) -> Vec<StageState> {
    self
      .stages()
      .iter()
      .map(|stage| {
        let issue_url = self
          .origin
          .issue(&stage.label)
          .map(|issue| issue.html_url.to_string());

        let feature_pr_url = self
          .origin
          .pr(&PullSelector::Branch(stage.branch_name(StagePart::Starter)))
          .map(|pr| pr.html_url.as_ref().unwrap().to_string());

        let solution_pr_url = self
          .origin
          .pr(&PullSelector::Branch(
            stage.branch_name(StagePart::Solution),
          ))
          .map(|pr| pr.html_url.as_ref().unwrap().to_string());

        let reference_solution_pr_url = self
          .upstream
          .pr(&PullSelector::Branch(
            stage.branch_name(StagePart::Solution),
          ))
          .map(|pr| pr.html_url.as_ref().unwrap().to_string());

        StageState {
          stage: stage.clone(),
          issue_url,
          feature_pr_url,
          solution_pr_url,
          reference_solution_pr_url,
        }
      })
      .collect()
  }

  pub async fn hard_reset(&self, stage_index: usize) -> Result<()> {
    let prev_stage = self.stage(stage_index - 1);
    let branch = format!("{UPSTREAM}/{}", prev_stage.branch_name(StagePart::Solution));
    self.origin_git.reset(&branch)?;
    let issue = self.file_issue(stage_index - 1).await?;
    self
      .origin
      .issue_handler()
      .update(issue.number)
      .state(IssueState::Closed)
      .send()
      .await?;

    self.infer_state_update().await?;
    Ok(())
  }
}

#[cfg(test)]
mod test {
  use super::*;
  use crate::github::{self, GithubToken};
  use env::current_dir;
  use std::{
    fs,
    sync::{Arc, Once},
  };

  const TEST_ORG: &str = "cognitive-engineering-lab";
  const TEST_REPO: &str = "rqst-test";

  struct DeleteRemoteRepo(Arc<Quest>);
  impl Drop for DeleteRemoteRepo {
    fn drop(&mut self) {
      tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
          self.0.origin.delete().await.unwrap();
        })
      })
    }
  }

  struct DeleteLocalRepo(PathBuf);
  impl Drop for DeleteLocalRepo {
    fn drop(&mut self) {
      fs::remove_dir_all(&self.0).unwrap();
    }
  }

  fn setup() {
    static SETUP: Once = Once::new();
    SETUP.call_once(|| {
      let token = github::get_github_token();
      match token {
        GithubToken::Found(token) => github::init_octocrab(&token).unwrap(),
        other => panic!("Failed to get github token: {other:?}"),
      }
    });
  }

  async fn load_test_quest() -> Result<Arc<Quest>> {
    let config = load_config_from_remote(TEST_ORG, TEST_REPO).await?;
    assert_eq!(
      config,
      QuestConfig {
        title: "Test".into(),
        author: TEST_ORG.into(),
        repo: TEST_REPO.into(),
        stages: vec![
          Stage {
            label: "00-stage".into(),
            name: "A".into(),
            no_starter: Some(true)
          },
          Stage {
            label: "01-stage".into(),
            name: "B".into(),
            no_starter: None
          },
          Stage {
            label: "02-stage".into(),
            name: "C".into(),
            no_starter: None
          }
        ]
      }
    );

    let dir = current_dir()?.join(TEST_REPO);
    Ok(Arc::new(Quest::load(dir, config, None).await?))
  }

  macro_rules! test_quest {
    ($id:ident) => {
      setup();

      let $id = load_test_quest().await?;

      $id.create_repo().await?;
      let _remote = DeleteRemoteRepo(Arc::clone(&$id));

      $id.clone_repo()?;
      let _local = DeleteLocalRepo($id.dir.clone());
    };
  }

  // TODO: some of this machinery should be its own tester binary
  #[tokio::test(flavor = "multi_thread")]
  #[ignore]
  async fn standard_playthrough() -> Result<()> {
    test_quest!(quest);

    macro_rules! state_is {
      ($a:expr, $b:expr, $c:expr) => {
        let state = quest.infer_state().await?;
        match state {
          QuestState::Ongoing {
            stage,
            part,
            status,
          } => assert_eq!((stage, part, status), ($a, $b, $c)),
          QuestState::Completed => panic!("finished"),
        };
      };
    }

    state_is!(0, StagePart::Starter, StagePartStatus::Start);

    let issue = quest.file_issue(0).await?;
    state_is!(0, StagePart::Solution, StagePartStatus::Start);

    quest.origin.close_issue(&issue).await?;
    state_is!(1, StagePart::Starter, StagePartStatus::Start);

    let (pr, issue) = quest.file_feature_and_issue(1).await?;
    let pr = pr.unwrap();
    state_is!(1, StagePart::Starter, StagePartStatus::Ongoing);

    quest.origin.merge_pr(&pr).await?;
    state_is!(1, StagePart::Solution, StagePartStatus::Start);

    let pr = quest.file_solution(1).await?;
    state_is!(1, StagePart::Solution, StagePartStatus::Ongoing);

    quest.origin.merge_pr(&pr).await?;
    state_is!(1, StagePart::Solution, StagePartStatus::Ongoing);

    quest.origin.close_issue(&issue).await?;
    state_is!(2, StagePart::Starter, StagePartStatus::Start);

    Ok(())
  }

  // TODO: can't seem to run these even sequentially?
  #[tokio::test(flavor = "multi_thread")]
  #[ignore]
  async fn skip() -> Result<()> {
    test_quest!(quest);

    macro_rules! state_is {
      ($a:expr, $b:expr, $c:expr) => {
        let state = quest.infer_state().await?;
        match state {
          QuestState::Ongoing {
            stage,
            part,
            status,
          } => assert_eq!((stage, part, status), ($a, $b, $c)),
          QuestState::Completed => panic!("finished"),
        };
      };
    }

    state_is!(0, StagePart::Starter, StagePartStatus::Start);

    quest.hard_reset(1).await?;
    state_is!(1, StagePart::Starter, StagePartStatus::Start);

    quest.hard_reset(2).await?;
    state_is!(2, StagePart::Starter, StagePartStatus::Start);

    Ok(())
  }
}
