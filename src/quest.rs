use std::{
  borrow::Cow,
  collections::HashMap,
  fmt::Write,
  fs,
  path::{Path, PathBuf},
};

use crate::{
  git::{GitRepo, MergeType, UPSTREAM},
  github::{self, GithubRepo, load_user},
  package::QuestPackage,
  source::{InstanceOutputs, PackageSource, QuestSource, RepoSource},
  stage::{Stage, StagePart},
};
use eyre::{Context, Result};
use http::StatusCode;
use octocrab::{
  GitHubError,
  models::{IssueState, issues::Issue, pulls::PullRequest},
  params::{Direction, issues},
};
use serde::{Deserialize, Serialize};
use tokio::try_join;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct QuestConfig {
  pub title: String,
  pub author: String,
  pub repo: String,
  pub stages: Vec<Stage>,
  pub read_only: Option<Vec<PathBuf>>,
  pub r#final: Option<serde_json::Value>,
  pub final_url: Option<String>,
  pub rq_version: String,
}

#[derive(Debug, strum::Display)]
pub enum QuestStrictness {
  Relaxed,
  Strict,
}

impl QuestStrictness {
  pub fn is_strict(&self) -> bool {
    matches!(self, QuestStrictness::Strict)
  }
}

#[derive(Debug)]
pub struct QuestUserPrefs {
  pub strictness: QuestStrictness,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StageState {
  pub stage: Stage,
  pub issue_url: Option<String>,
  pub pr_url: Option<String>,
  pub refsol_url: Option<String>,
}

impl QuestConfig {
  pub fn load(repo: &GitRepo, remote: Option<&str>) -> Result<Self> {
    let branch = match remote {
      Some(remote) => Cow::Owned(format!("{remote}/meta")),
      None => Cow::Borrowed("meta"),
    };
    let config_str = repo.read_file(&branch, "rqst.toml")?;
    let mut config = toml::de::from_str::<QuestConfig>(&config_str)
      .context("Failed to parse quest configuration rqst.toml")?;

    if repo.contains_file(&branch, "final.toml")? {
      let quiz_str = repo.read_file(&branch, "final.toml")?;
      let quiz =
        toml::de::from_str::<serde_json::Value>(&quiz_str).context("Failed to parse final.toml")?;
      config.r#final = Some(quiz);
    }

    Ok(config)
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum QuestState {
  Ongoing { stage: u32, started: bool },
  Completed,
}

pub struct Quest {
  pub source: Box<dyn QuestSource>,
  pub origin: GithubRepo,
  pub origin_git: GitRepo,
  pub stage_index: HashMap<String, usize>,
  pub dir: PathBuf,
  pub config: QuestConfig,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StateDescriptor {
  pub dir: PathBuf,
  pub stages: Vec<StageState>,
  pub state: QuestState,
  pub can_skip: bool,
  pub behind_origin: bool,
}

pub enum CreateSource {
  Remote { user: String, repo: String },
  Package(Box<QuestPackage>),
}

impl Quest {
  async fn load_core(
    dir: &Path,
    config: QuestConfig,
    template: Box<dyn QuestSource>,
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
      dir: dir.to_path_buf(),
      config,
      source: template,
      origin,
      origin_git,
      stage_index,
    };

    q.infer_state_update().await?;

    Ok(q)
  }

  #[tracing::instrument(skip(source))]
  pub async fn create(dir: &Path, source: CreateSource, prefs: QuestUserPrefs) -> Result<Self> {
    github::check_ssh()?;

    let template: Box<dyn QuestSource> = match source {
      CreateSource::Remote { user, repo } => {
        let upstream = GithubRepo::load(&user, &repo).await?;
        Box::new(RepoSource(upstream))
      }
      CreateSource::Package(package) => Box::new(PackageSource(*package)),
    };

    let InstanceOutputs {
      origin,
      origin_git,
      config,
    } = template.instantiate(dir).await?;

    if prefs.strictness.is_strict() {
      origin_git.install_hooks()?;
      origin
        .set_var("STRICTNESS", &prefs.strictness.to_string())
        .await?;
    }

    Self::load_core(
      &dir.join(&config.repo),
      config,
      template,
      origin,
      origin_git,
    )
    .await
  }

  pub async fn load(dir: &Path) -> Result<Self> {
    let user = load_user().await?;
    let origin_git = GitRepo::new(dir);
    let upstream = origin_git
      .upstream()
      .context("Failed to test for upstream")?;
    let config = QuestConfig::load(&origin_git, upstream).context("Failed to load quest config")?;
    let origin_fut = async {
      GithubRepo::load(&user, &config.repo)
        .await
        .context("Failed to load GitHub repo")
    };
    let template_fut = async {
      if upstream.is_some() {
        let upstream = GithubRepo::load(&config.author, &config.repo)
          .await
          .context("Failed to load upstream GitHub repo")?;
        Ok(Box::new(RepoSource(upstream)) as Box<dyn QuestSource>)
      } else {
        let contents = origin_git.show_bin("meta", "package.json.gz")?;
        let package =
          QuestPackage::load_from_blob(&contents).context("Failed to load quest package")?;
        Ok(Box::new(PackageSource(package)) as Box<dyn QuestSource>)
      }
    };
    let (origin, template) = try_join!(origin_fut, template_fut)?;

    Self::load_core(dir, config, template, origin, origin_git).await
  }

  pub fn stages(&self) -> &[Stage] {
    &self.config.stages
  }

  fn stage(&self, idx: usize) -> &Stage {
    &self.config.stages[idx]
  }

  pub async fn infer_state(&self) -> Result<QuestState> {
    let issue_handler = self.origin.issue_handler();
    let issue_page_future = issue_handler
      .list()
      .state(octocrab::params::State::All)
      .sort(issues::Sort::Created)
      .direction(Direction::Descending)
      .per_page(10)
      .send();

    let mut issue_page = match issue_page_future.await {
      Ok(result) => result,
      Err(octocrab::Error::GitHub { source, .. })
        if matches!(
          &*source,
          GitHubError {
            status_code: StatusCode::NOT_FOUND,
            ..
          }
        ) =>
      {
        return Ok(QuestState::Ongoing {
          stage: 0,
          started: false,
        });
      }
      Err(e) => return Err(e.into()),
    };

    let issues = issue_page.take_items();

    let issue_map = issues
      .into_iter()
      .filter_map(|issue| {
        let label = issue.labels.first()?;
        let is_issue = issue.pull_request.is_none();
        if is_issue {
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

    let issue_stages = issue_map.iter().filter_map(|(label, issue)| {
      let stage = (*stage_map.get(label)?).clone();
      let finished = matches!(issue.state, IssueState::Closed);
      Some((stage, finished))
    });

    tracing::trace!("Issues: {:#?}", issue_stages.clone().collect::<Vec<_>>());

    let stage_idx = |stage: &Stage| self.stage_index[&stage.label];
    let Some((stage, finished)) =
      issue_stages.max_by_key(|(stage, finished)| (stage_idx(stage), *finished))
    else {
      return Ok(QuestState::Ongoing {
        stage: 0,
        started: false,
      });
    };

    let stage = stage_idx(&stage);

    Ok(if finished {
      if stage == self.stages().len() - 1 {
        QuestState::Completed
      } else {
        QuestState::Ongoing {
          stage: (stage + 1) as u32,
          started: false,
        }
      }
    } else {
      QuestState::Ongoing {
        stage: stage as u32,
        started: true,
      }
    })
  }

  pub async fn state_descriptor(&self) -> Result<StateDescriptor> {
    let state = self.infer_state().await?;
    let behind_origin = self.origin_git.is_behind_origin()?;
    Ok(StateDescriptor {
      dir: self.dir.clone(),
      stages: self.stage_states(),
      state,
      can_skip: self.source.can_skip(),
      behind_origin,
    })
  }

  async fn infer_state_update(&self) -> Result<()> {
    self.origin.fetch().await?;
    Ok(())
  }

  async fn file_pr(
    &self,
    default_title: &str,
    origin_head: &str,
    origin_commit: &str,
    upstream_head: &str,
    merge_type: MergeType,
    stage_label: &str,
  ) -> Result<PullRequest> {
    let pr = self.source.pull_request(upstream_head);

    let (title, mut body, mut labels, comments) = match pr {
      Some(pr) => {
        let comments = self.source.pull_request_comments(&pr).await?;
        let title = pr.title.expect("Missing PR title");
        let body = format!(
          "{}\n\nResolves {{{{ {stage_label} issue }}}}.\n",
          pr.body.expect("Missing PR body")
        );
        let labels = match pr.labels {
          Some(labels) => labels.iter().map(|label| label.name.clone()).collect(),
          None => Vec::new(),
        };
        (title, body, labels, comments)
      }
      None => (
        default_title.to_string(),
        format!("This PR resolves {{{{ {stage_label} issue }}}}."),
        vec![stage_label.to_string()],
        Vec::new(),
      ),
    };

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

    const RESET_LABEL: &str = "reset";
    if is_reset {
      labels.push(RESET_LABEL.into());
    }

    let new_pr = self
      .origin
      .file_pr(
        &title,
        &body,
        &labels,
        origin_head,
        origin_commit,
        &comments,
      )
      .await?;

    Ok(new_pr)
  }

  #[tracing::instrument(skip(self))]
  pub async fn start_stage(&self, stage_index: usize) -> Result<(PullRequest, Issue)> {
    let stage = self.stage(stage_index);

    let src_issue = self
      .source
      .issue(&stage.label)
      .with_context(|| format!("Failed to get issue for stage: {}", stage.name))?;

    let issue = self
      .origin
      .copy_issue(&src_issue)
      .await
      .with_context(|| format!("Failed to file issue for stage: {}", stage.name))?;

    let upstream_base = if stage_index > 0 {
      let prev_stage = self.stage(stage_index - 1);
      prev_stage.branch_name(StagePart::Solution)
    } else {
      "main".into()
    };

    self
      .origin_git
      .checkout_main()
      .context("Failed to checkout main")?;
    self.origin_git.pull().context("Failed to pull")?;

    let origin_head = &stage.label;
    let upstream_head = stage.branch_name(StagePart::Starter);

    self.origin_git.create_branch(origin_head)?;

    let readme_path = self.dir.join("README.md");
    let mut readme_contents =
      fs::read_to_string(&readme_path).context("Failed to read README.md")?;
    write!(readme_contents, "\n- [x] {}", stage.name)?;
    fs::write(&readme_path, readme_contents)?;

    self
      .origin_git
      .create_commit(&format!("Start of solution for {}", stage.name))?;

    let merge_type = if !stage.no_starter() {
      self
        .source
        .apply_patch(&self.origin_git, &upstream_base, &upstream_head)?
    } else {
      MergeType::Success
    };

    self.origin_git.push_branch(origin_head)?;
    let origin_commit = self.origin_git.head_commit()?;

    let pr = self
      .file_pr(
        &src_issue.title,
        origin_head,
        &origin_commit,
        &upstream_head,
        merge_type,
        &stage.label,
      )
      .await?;

    try_join!(
      self.origin.update_issue_links(&issue),
      self.origin.update_pr_links(&pr)
    )?;

    Ok((pr, issue))
  }

  #[tracing::instrument(skip(self))]
  pub async fn add_solution(&self, stage_index: usize) -> Result<()> {
    let stage = self.stage(stage_index);
    if self.source.refsol_url(stage).is_none() {
      panic!("Attempting to use reference solution for a quest source w/o one")
    }

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

    let origin_head = &stage.label;
    let upstream_head = stage.branch_name(StagePart::Solution);

    self
      .source
      .apply_patch(&self.origin_git, &base, &upstream_head)?;

    self.origin_git.push_branch(origin_head)?;

    Ok(())
  }

  fn stage_states(&self) -> Vec<StageState> {
    self
      .stages()
      .iter()
      .map(|stage| {
        let issue_url = self
          .origin
          .issue(&stage.label)
          .map(|issue| issue.html_url.to_string());

        let pr_url = self
          .origin
          .pr(&stage.label)
          .map(|pr| pr.html_url.as_ref().unwrap().to_string());

        let refsol_url = self.source.refsol_url(stage);

        StageState {
          stage: stage.clone(),
          issue_url,
          pr_url,
          refsol_url,
        }
      })
      .collect()
  }

  #[tracing::instrument(skip(self))]
  pub async fn skip_to_stage(&self, stage_index: usize) -> Result<()> {
    if stage_index > 1 {
      let prev_stage = self.stage(stage_index - 2);
      let branch = format!("{UPSTREAM}/{}", prev_stage.branch_name(StagePart::Solution));
      self
        .origin_git
        .reset(&branch)
        .with_context(|| format!("Failed to reset to branch: {branch}"))?;
    }

    let (pr, issue) = self.start_stage(stage_index - 1).await?;
    self.add_solution(stage_index - 1).await?;
    self.origin.merge_pr(&pr).await?;
    self.origin.wait_for_issue_closed(&issue).await?;

    Ok(())
  }
}
