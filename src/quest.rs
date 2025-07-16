use std::{
  collections::HashMap,
  fmt::Write,
  fs,
  path::{Path, PathBuf},
};

use crate::{
  chapter::{Chapter, ChapterPart},
  git::{Branch, GitRepo, MergeType, Ref},
  github::{self, GithubRepo, load_user},
  package::QuestPackage,
  source::{InstanceOutputs, QuestSource},
};
use eyre::{Context, Result, bail, ensure};
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
  pub chapters: Vec<Chapter>,
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
pub struct ChapterState {
  pub chapter: Chapter,
  pub issue_url: Option<String>,
  pub pr_url: Option<String>,
  pub refsol_url: Option<String>,
}

impl QuestConfig {
  pub fn load(repo: &GitRepo, remote: Option<&str>) -> Result<Self> {
    let branch = match remote {
      Some(remote) => Branch::new(format!("{remote}/meta")),
      None => Branch::new("meta"),
    };
    let config_str = repo.read_file_string(&branch, Path::new("rqst.toml"))?;
    let mut config = toml::de::from_str::<QuestConfig>(&config_str)
      .context("Failed to parse quest configuration rqst.toml")?;

    let final_path = Path::new("final.toml");
    if repo.contains_file(&branch, final_path)? {
      let quiz_str = repo.read_file_string(&branch, final_path)?;
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
  Ongoing { chapter: u32, started: bool },
  Completed,
}

pub struct Quest {
  pub source: Box<dyn QuestSource>,
  pub origin: GithubRepo,
  pub origin_git: GitRepo,
  pub chapter_index: HashMap<String, usize>,
  pub dir: PathBuf,
  pub config: QuestConfig,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StateDescriptor {
  pub dir: PathBuf,
  pub chapters: Vec<ChapterState>,
  pub state: QuestState,
  pub can_skip: bool,
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
    let chapter_index = config
      .chapters
      .iter()
      .enumerate()
      .map(|(i, chapter)| (chapter.label.clone(), i))
      .collect::<HashMap<_, _>>();

    let q = Quest {
      dir: dir.to_path_buf(),
      config,
      source: template,
      origin,
      origin_git,
      chapter_index,
    };

    let exists = q.origin.fetch().await?;
    ensure!(exists, "Repo is missing");

    Ok(q)
  }

  #[tracing::instrument(skip(source))]
  pub async fn create(dir: &Path, source: CreateSource, prefs: QuestUserPrefs) -> Result<Self> {
    github::check_ssh()?;

    let template: Box<dyn QuestSource> = match source {
      CreateSource::Remote { user, repo } => {
        let upstream = GithubRepo::load(&user, &repo).await?;
        Box::new(upstream)
      }
      CreateSource::Package(package) => Box::new(*package),
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
    let remote = origin_git
      .upstream()
      .context("Failed to test for upstream")?;
    let config = QuestConfig::load(&origin_git, remote).context("Failed to load quest config")?;
    let origin_fut = async {
      GithubRepo::load(&user, &config.repo)
        .await
        .context("Failed to load GitHub repo")
    };
    let template_fut = async {
      if remote.is_some() {
        let upstream = GithubRepo::load(&config.author, &config.repo)
          .await
          .context("Failed to load upstream GitHub repo")?;
        Ok(Box::new(upstream) as Box<dyn QuestSource>)
      } else {
        let contents = origin_git.read_file_bytes(&Branch::meta(), Path::new("package.json.gz"))?;
        let package =
          QuestPackage::load_from_blob(&contents).context("Failed to load quest package")?;
        Ok(Box::new(package) as Box<dyn QuestSource>)
      }
    };
    let (origin, template) = try_join!(origin_fut, template_fut)?;

    Self::load_core(dir, config, template, origin, origin_git).await
  }

  pub fn chapters(&self) -> &[Chapter] {
    &self.config.chapters
  }

  fn chapter(&self, idx: usize) -> &Chapter {
    &self.config.chapters[idx]
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
          chapter: 0,
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

    let chapter_map = self
      .chapters()
      .iter()
      .map(|chapter| (chapter.label.clone(), chapter))
      .collect::<HashMap<_, _>>();

    let issue_chapters = issue_map.iter().filter_map(|(label, issue)| {
      let chapter = (*chapter_map.get(label)?).clone();
      let finished = matches!(issue.state, IssueState::Closed);
      Some((chapter, finished))
    });

    tracing::trace!("Issues: {:#?}", issue_chapters.clone().collect::<Vec<_>>());

    let chapter_idx = |chapter: &Chapter| self.chapter_index[&chapter.label];
    let Some((chapter, finished)) =
      issue_chapters.max_by_key(|(chapter, finished)| (chapter_idx(chapter), *finished))
    else {
      return Ok(QuestState::Ongoing {
        chapter: 0,
        started: false,
      });
    };

    let chapter = chapter_idx(&chapter);

    Ok(if finished {
      if chapter == self.chapters().len() - 1 {
        QuestState::Completed
      } else {
        QuestState::Ongoing {
          chapter: (chapter + 1) as u32,
          started: false,
        }
      }
    } else {
      QuestState::Ongoing {
        chapter: chapter as u32,
        started: true,
      }
    })
  }

  pub async fn state_descriptor(&self) -> Result<StateDescriptor> {
    let state = self.infer_state().await?;
    Ok(StateDescriptor {
      dir: self.dir.clone(),
      chapters: self.chapter_states(),
      state,
      can_skip: self.source.can_skip(),
    })
  }

  async fn file_pr(
    &self,
    default_title: &str,
    origin_head: &Branch,
    origin_commit: &Ref,
    upstream_head: &Branch,
    merge_type: MergeType,
    chapter_label: &str,
  ) -> Result<PullRequest> {
    let pr = self.source.pull_request(upstream_head);

    let (title, mut body, mut labels, comments) = match pr {
      Some(pr) => {
        let comments = self.source.pull_request_comments(&pr).await?;
        let title = pr.title.expect("Missing PR title");
        let body = format!(
          "{}\n\nResolves {{{{ {chapter_label} issue }}}}. (Don't merge until you've added your solution!)\n",
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
        format!(
          "This PR resolves {{{{ {chapter_label} issue }}}}. (Don't merge until you've added your solution!)"
        ),
        vec![chapter_label.to_string()],
        Vec::new(),
      ),
    };

    const RESET_LABEL: &str = "reset";
    if merge_type.is_reset() {
      body.push_str("\n\nNote: due to a merge conflict, this PR is a hard reset to the reference code, and may have overwritten your previous changes.");
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
  pub async fn start_chapter(&self, chapter_index: usize) -> Result<(PullRequest, Issue)> {
    let chapter = self.chapter(chapter_index);

    let src_issue = self
      .source
      .issue(&chapter.label)
      .with_context(|| format!("Failed to get issue for chapter: {}", chapter.name))?;

    let issue = self
      .origin
      .copy_issue(&src_issue)
      .await
      .with_context(|| format!("Failed to file issue for chapter: {}", chapter.name))?;

    let upstream_base = if chapter_index > 0 {
      let prev_chapter = self.chapter(chapter_index - 1);
      prev_chapter.branch(ChapterPart::Solution)
    } else {
      Branch::new("main")
    };

    let origin_head = Branch::new(&chapter.label);
    let upstream_head = chapter.branch(ChapterPart::Starter);

    self.origin_git.create_branch_from_main(&origin_head)?;

    let readme_path = self.dir.join("README.md");
    let mut readme_contents =
      fs::read_to_string(&readme_path).context("Failed to read README.md")?;
    write!(readme_contents, "\n- [x] {}", chapter.name)?;
    fs::write(&readme_path, readme_contents)?;

    self.origin_git.add(&readme_path)?;
    let commit_msg = format!("Start of solution for {}", chapter.name);
    self.origin_git.commit(&commit_msg)?;

    let merge_type = if !chapter.no_starter() {
      self
        .source
        .apply_patch(&self.origin_git, &upstream_base, &upstream_head)?
    } else {
      MergeType::Success
    };

    self.origin_git.push(&origin_head)?;
    let origin_commit = self.origin_git.head_commit()?;

    let pr = self
      .file_pr(
        &src_issue.title,
        &origin_head,
        &origin_commit,
        &upstream_head,
        merge_type,
        &chapter.label,
      )
      .await?;

    try_join!(
      self.origin.update_issue_links(&issue),
      self.origin.update_pr_links(&pr)
    )?;

    Ok((pr, issue))
  }

  #[tracing::instrument(skip(self))]
  pub async fn add_solution(&self, chapter_index: usize) -> Result<()> {
    let chapter = self.chapter(chapter_index);
    if self.source.refsol_url(chapter).is_none() {
      panic!("Attempting to use reference solution for a quest source w/o one")
    }

    let base = if chapter.no_starter() {
      // TODO: repeats w/ file_feature
      if chapter_index > 0 {
        let prev_chapter = self.chapter(chapter_index - 1);
        prev_chapter.branch(ChapterPart::Solution)
      } else {
        Branch::main()
      }
    } else {
      chapter.branch(ChapterPart::Starter)
    };

    let origin_head = Branch::new(&chapter.label);
    let upstream_head = chapter.branch(ChapterPart::Solution);

    self
      .source
      .apply_patch(&self.origin_git, &base, &upstream_head)?;

    self.origin_git.push(&origin_head)?;

    Ok(())
  }

  fn chapter_states(&self) -> Vec<ChapterState> {
    self
      .chapters()
      .iter()
      .map(|chapter| {
        let issue_url = self
          .origin
          .issue(&chapter.label)
          .map(|issue| issue.html_url.to_string());

        let pr_url = self
          .origin
          .pr(&Branch::new(&chapter.label))
          .map(|pr| pr.html_url.as_ref().unwrap().to_string());

        let refsol_url = self.source.refsol_url(chapter);

        ChapterState {
          chapter: chapter.clone(),
          issue_url,
          pr_url,
          refsol_url,
        }
      })
      .collect()
  }

  #[tracing::instrument(skip(self))]
  pub async fn skip_to_chapter(&self, chapter_index: usize) -> Result<()> {
    let Some(upstream) = self.origin_git.upstream()? else {
      bail!("Cannot skip to chapter without an upstream")
    };

    if chapter_index > 1 {
      let prev_chapter = self.chapter(chapter_index - 2);
      let branch = Branch::new(format!(
        "{upstream}/{}",
        prev_chapter.branch(ChapterPart::Solution)
      ));
      self
        .origin_git
        .hard_reset(&branch)
        .with_context(|| format!("Failed to reset to branch: {branch}"))?;
    }

    let (pr, issue) = self.start_chapter(chapter_index - 1).await?;
    self.add_solution(chapter_index - 1).await?;
    self.origin.merge_pr(&pr).await?;
    self.origin.wait_for_issue_closed(&issue).await?;
    self.start_chapter(chapter_index).await?;

    Ok(())
  }
}
