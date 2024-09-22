use std::{collections::HashMap, path::PathBuf, time::Duration};

use crate::{
  git::{GitRepo, UPSTREAM},
  github::{load_user, GithubRepo, PullSelector},
  package::QuestPackage,
  stage::{Stage, StagePart, StagePartStatus},
  template::{InstanceOutputs, PackageTemplate, QuestTemplate, RepoTemplate},
};
use anyhow::Result;
use http::StatusCode;
use octocrab::{
  models::{issues::Issue, pulls::PullRequest, IssueState},
  params::{issues, pulls, Direction},
  GitHubError,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::{time::sleep, try_join};

pub trait StateEmitter: Send + Sync + 'static {
  fn emit(&self, state: StateDescriptor) -> Result<()>;
}

pub struct NoopEmitter;

impl StateEmitter for NoopEmitter {
  fn emit(&self, _state: StateDescriptor) -> Result<()> {
    Ok(())
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct QuestConfig {
  pub title: String,
  pub author: String,
  pub repo: String,
  pub stages: Vec<Stage>,
  pub r#final: Option<serde_json::Value>,
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
  pub fn load(repo: &GitRepo, remote: &str) -> Result<Self> {
    let contents = repo.show(&format!("{remote}/meta"), "rqst.toml")?;
    let config = toml::de::from_str::<QuestConfig>(&contents)?;
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
  template: Box<dyn QuestTemplate>,
  origin: GithubRepo,
  origin_git: GitRepo,
  stage_index: HashMap<String, usize>,
  dir: PathBuf,
  state_event: Box<dyn StateEmitter>,

  pub config: QuestConfig,
}

#[derive(Serialize, Deserialize, Clone, Type)]
pub struct StateDescriptor {
  dir: PathBuf,
  stages: Vec<StageState>,
  state: QuestState,
}

pub enum CreateSource {
  Remote { user: String, repo: String },
  Package(QuestPackage),
}

impl Quest {
  async fn load_core(
    dir: PathBuf,
    config: QuestConfig,
    state_event: Box<dyn StateEmitter>,
    template: Box<dyn QuestTemplate>,
    origin: GithubRepo,
    origin_git: GitRepo,
  ) -> Result<Self> {
    let stage_index = config
      .stages
      .iter()
      .enumerate()
      .map(|(i, stage)| (stage.label.clone(), i))
      .collect::<HashMap<_, _>>();

    let q = Quest {
      dir,
      config,
      template,
      origin,
      origin_git,
      stage_index,
      state_event,
    };

    q.infer_state_update().await?;

    Ok(q)
  }

  pub async fn create(
    dir: PathBuf,
    source: CreateSource,
    state_event: Box<dyn StateEmitter>,
  ) -> Result<Self> {
    let template: Box<dyn QuestTemplate> = match source {
      CreateSource::Remote { user, repo } => {
        let upstream = GithubRepo::load(&user, &repo).await?;
        Box::new(RepoTemplate(upstream))
      }
      CreateSource::Package(package) => Box::new(PackageTemplate(package)),
    };

    let InstanceOutputs {
      origin,
      origin_git,
      config,
    } = template.instantiate(&dir).await?;

    Self::load_core(
      dir.join(&config.repo),
      config,
      state_event,
      template,
      origin,
      origin_git,
    )
    .await
  }

  pub async fn load(dir: PathBuf, state_event: Box<dyn StateEmitter>) -> Result<Self> {
    let user = load_user().await?;
    let origin_git = GitRepo::new(&dir);
    let config = QuestConfig::load(&origin_git, "origin")?;
    let origin = GithubRepo::load(&user, &config.repo).await?;
    let template: Box<dyn QuestTemplate> = if origin_git.has_upstream()? {
      let upstream = GithubRepo::load(&config.author, &config.repo).await?;
      Box::new(RepoTemplate(upstream))
    } else {
      let contents = origin_git.show_bin("meta", "package.json.gz")?;
      let package = QuestPackage::load_from_blob(&contents)?;
      Box::new(PackageTemplate(package))
    };

    Self::load_core(dir, config, state_event, template, origin, origin_git).await
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

  pub async fn state_descriptor(&self) -> Result<StateDescriptor> {
    let state = self.infer_state().await?;
    Ok(StateDescriptor {
      dir: self.dir.clone(),
      stages: self.stage_states(),
      state,
    })
  }

  pub async fn infer_state_update(&self) -> Result<()> {
    self.origin.fetch().await?;
    let state = self.state_descriptor().await?;
    self.state_event.emit(state)?;

    Ok(())
  }

  pub async fn infer_state_loop(&self) {
    loop {
      self.infer_state_update().await.unwrap();
      sleep(Duration::from_secs(10)).await;
    }
  }

  async fn file_pr(&self, base_branch: &str, target_branch: &str) -> Result<PullRequest> {
    self.origin_git.checkout_main_and_pull()?;

    let (branch_head, merge_type) =
      self
        .origin_git
        .create_branch_from(&*self.template, base_branch, target_branch)?;

    let pr = self
      .template
      .pull_request(&PullSelector::Branch(target_branch.into()))?;
    let new_pr = self.origin.copy_pr(&pr, &branch_head, merge_type).await?;

    tracing::debug!("Filed PR: {base_branch} -> {target_branch}");

    Ok(new_pr)
  }

  async fn file_issue(&self, stage_index: usize) -> Result<Issue> {
    let stage = self.stage(stage_index);
    let issue = self.template.issue(&stage.label)?;
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
        .file_pr(&base_branch, &stage.branch_name(StagePart::Starter))
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
    let base = if stage.no_starter() {
      // TODO: repeats w/ file_feature
      if stage_index > 0 {
        let prev_stage = self.stage(stage_index - 1);
        prev_stage.branch_name(StagePart::Solution)
      } else {
        "main".into()
      }
    } else {
      stage.branch_name(StagePart::Starter)
    };
    let pr = self
      .file_pr(&base, &stage.branch_name(StagePart::Solution))
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
          .map(|pr| pr.data.html_url.as_ref().unwrap().to_string());

        let solution_pr_url = self
          .origin
          .pr(&PullSelector::Branch(
            stage.branch_name(StagePart::Solution),
          ))
          .map(|pr| pr.data.html_url.as_ref().unwrap().to_string());

        let reference_solution_pr_url = self.template.reference_solution_pr_url(stage);

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
    env, fs,
    path::Path,
    sync::{Arc, Once},
  };
  use tracing_subscriber::{fmt, layer::SubscriberExt, prelude::*, EnvFilter};

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
      tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

      let token = github::get_github_token();
      match token {
        GithubToken::Found(token) => github::init_octocrab(&token).unwrap(),
        other => panic!("Failed to get github token: {other:?}"),
      }
    });
  }

  async fn create_test_quest(source: CreateSource) -> Result<Arc<Quest>> {
    let dir = current_dir()?;
    let quest = Quest::create(dir, source, Box::new(NoopEmitter)).await?;
    Ok(Arc::new(quest))
  }

  macro_rules! test_quest {
    ($id:ident, $source:expr) => {
      setup();

      let $id = create_test_quest($source).await?;
      let _remote = DeleteRemoteRepo(Arc::clone(&$id));
      let _local = DeleteLocalRepo($id.dir.clone());
    };
    ($id:ident) => {
      test_quest!(
        $id,
        CreateSource::Remote {
          user: TEST_ORG.into(),
          repo: TEST_REPO.into(),
        }
      )
    };
  }

  macro_rules! state_is {
    ($quest:expr, $a:expr, $b:expr, $c:expr) => {{
      let state = $quest.infer_state().await?;
      match state {
        QuestState::Ongoing {
          stage,
          part,
          status,
        } => assert_eq!((stage, part, status), ($a, $b, $c)),
        QuestState::Completed => panic!("finished"),
      };
    }};
  }

  // TODO: some of this machinery should be its own tester binary
  #[tokio::test(flavor = "multi_thread")]
  #[ignore]
  async fn remote_playthrough() -> Result<()> {
    test_quest!(quest);

    state_is!(quest, 0, StagePart::Starter, StagePartStatus::Start);

    let issue = quest.file_issue(0).await?;
    state_is!(quest, 0, StagePart::Solution, StagePartStatus::Start);

    quest.origin.close_issue(&issue).await?;
    state_is!(quest, 1, StagePart::Starter, StagePartStatus::Start);

    let (pr, issue) = quest.file_feature_and_issue(1).await?;
    let pr = pr.unwrap();
    state_is!(quest, 1, StagePart::Starter, StagePartStatus::Ongoing);

    quest.origin.merge_pr(&pr).await?;
    state_is!(quest, 1, StagePart::Solution, StagePartStatus::Start);

    let pr = quest.file_solution(1).await?;
    state_is!(quest, 1, StagePart::Solution, StagePartStatus::Ongoing);

    quest.origin.merge_pr(&pr).await?;
    state_is!(quest, 1, StagePart::Solution, StagePartStatus::Ongoing);

    quest.origin.close_issue(&issue).await?;
    state_is!(quest, 2, StagePart::Starter, StagePartStatus::Start);

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread")]
  #[ignore]
  async fn local_playthrough() -> Result<()> {
    let package = QuestPackage::load_from_file(Path::new("rqst-test.json.gz"))?;
    test_quest!(quest, CreateSource::Package(package));

    state_is!(quest, 0, StagePart::Starter, StagePartStatus::Start);

    let issue = quest.file_issue(0).await?;
    state_is!(quest, 0, StagePart::Solution, StagePartStatus::Start);

    quest.origin.close_issue(&issue).await?;
    state_is!(quest, 1, StagePart::Starter, StagePartStatus::Start);

    let (pr, issue) = quest.file_feature_and_issue(1).await?;
    let pr = pr.unwrap();
    state_is!(quest, 1, StagePart::Starter, StagePartStatus::Ongoing);

    quest.origin.merge_pr(&pr).await?;
    state_is!(quest, 1, StagePart::Solution, StagePartStatus::Start);

    // don't merge the solution PR, that doesn't exist

    quest.origin.close_issue(&issue).await?;
    state_is!(quest, 2, StagePart::Starter, StagePartStatus::Start);

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
