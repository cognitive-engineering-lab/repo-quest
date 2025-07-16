use std::{
  collections::HashMap,
  fs::File,
  io::{BufReader, BufWriter, Read},
  path::{Path, PathBuf},
};

use crate::{
  chapter::ChapterPart,
  git::{Branch, GitRepo, Ref},
  github::GithubRepo,
  quest::QuestConfig,
};
use eyre::{Context, Result};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use futures_util::future::try_join_all;
use octocrab::models::{
  Label,
  issues::Issue,
  pulls::{self, PullRequest},
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct FullPullRequest {
  pub data: PullRequest,
  pub comments: Vec<pulls::Comment>,
}

#[derive(Serialize, Deserialize)]
pub struct Patch {
  pub base: Branch,
  pub head: Branch,
  pub patch: String,
}

#[derive(Serialize, Deserialize)]
pub struct QuestPackage {
  pub version: Version,
  pub config: QuestConfig,
  pub issues: Vec<Issue>,
  pub prs: Vec<FullPullRequest>,
  pub initial: HashMap<PathBuf, String>,
  pub patches: Vec<Patch>,
  #[serde(skip)]
  patch_map: HashMap<(Branch, Branch), usize>,
  pub labels: Vec<Label>,
}

fn version() -> Version {
  Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
}

impl QuestPackage {
  pub async fn build(path: &Path) -> Result<Self> {
    let git_repo = GitRepo::new(path);
    let config = QuestConfig::load(&git_repo, None)?;
    let gh_repo = GithubRepo::load(&config.author, &config.repo).await?;

    let initial = git_repo.read_files(&Branch::main())?;
    let issues = gh_repo.issues().clone();
    let prs = try_join_all(gh_repo.prs().iter().map(async |pr| {
      let comments = gh_repo.pr_comments(pr).await?;
      Ok::<_, eyre::Error>(FullPullRequest {
        data: pr.clone(),
        comments,
      })
    }))
    .await?;
    let labels = gh_repo
      .issue_handler()
      .list_labels_for_repo()
      .send()
      .await?
      .take_items();
    let patches = config
      .chapters
      .iter()
      .enumerate()
      .filter(|(_, chapter)| !matches!(chapter.no_starter, Some(true)))
      .map(|(i, chapter)| {
        let prev_chapter = (i > 0).then(|| &config.chapters[i - 1]);
        let base = match prev_chapter {
          Some(chapter) => chapter.branch(ChapterPart::Solution),
          None => Branch::main(),
        };
        let head = chapter.branch(ChapterPart::Starter);
        let patch = git_repo.diff(&Ref::from(&base), &Ref::from(&head))?;
        Ok(Patch { base, head, patch })
      })
      .collect::<Result<Vec<_>>>()?;

    Ok(QuestPackage {
      version: version(),
      config,
      initial,
      issues,
      prs,
      labels,
      patches,
      patch_map: HashMap::default(),
    })
  }

  pub fn patch(&self, key: &(Branch, Branch)) -> Option<usize> {
    self.patch_map.get(key).copied()
  }

  fn deserialize(t: impl Read) -> Result<Self> {
    let mut decoder = GzDecoder::new(t);
    let mut package: QuestPackage =
      serde_json::from_reader(&mut decoder).context("Failed to parse JSON")?;
    package.patch_map = package
      .patches
      .iter()
      .enumerate()
      .map(|(i, patch)| ((patch.base.clone(), patch.head.clone()), i))
      .collect();
    let version = version();
    let req = VersionReq::parse(&format!("^{version}")).unwrap();
    if !req.matches(&package.version) {
      tracing::warn!("Loaded package has potentially incompatible version: {version}");
    }
    Ok(package)
  }

  pub fn load_from_file(path: &Path) -> Result<Self> {
    let mut f = BufReader::new(File::open(path)?);
    Self::deserialize(&mut f)
      .with_context(|| format!("Failed to load quest package: {}", path.display()))
  }

  pub fn load_from_blob(blob: &[u8]) -> Result<Self> {
    Self::deserialize(blob).context("Failed to load quest package from blob")
  }

  pub fn save(&self, path: &Path) -> Result<()> {
    let mut f = BufWriter::new(File::create(path)?);
    let mut encoder = GzEncoder::new(&mut f, Compression::best());
    serde_json::to_writer_pretty(&mut encoder, self)?;
    Ok(())
  }
}
