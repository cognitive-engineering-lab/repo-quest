use std::{
  collections::HashMap,
  fs::File,
  io::{BufReader, BufWriter, Read},
  path::{Path, PathBuf},
};

use crate::{
  git::GitRepo,
  github::{FullPullRequest, GithubRepo},
  quest::QuestConfig,
  stage::StagePart,
};
use anyhow::{Context, Result};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use octocrab::models::{issues::Issue, Label};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Patch {
  pub base: String,
  pub head: String,
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
  patch_map: HashMap<(String, String), usize>,
  pub labels: Vec<Label>,
}

fn version() -> Version {
  Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
}

impl QuestPackage {
  pub async fn build(path: &Path) -> Result<Self> {
    let git_repo = GitRepo::new(path);
    let config = QuestConfig::load(&git_repo, "origin")?;
    let gh_repo = GithubRepo::load(&config.author, &config.repo).await?;

    let initial = git_repo.read_initial_files()?;
    let issues = gh_repo.issues().clone();
    let prs = gh_repo.prs().clone();
    let labels = gh_repo
      .issue_handler()
      .list_labels_for_repo()
      .send()
      .await?
      .take_items();
    let patches = config
      .stages
      .iter()
      .enumerate()
      .filter(|(_, stage)| !matches!(stage.no_starter, Some(true)))
      .map(|(i, stage)| {
        let prev_stage = (i > 0).then(|| &config.stages[i - 1]);
        let base = match prev_stage {
          Some(stage) => stage.branch_name(StagePart::Solution),
          None => "main".into(),
        };
        let head = stage.branch_name(StagePart::Starter);
        let patch = git_repo.diff(&base, &head)?;
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

  pub fn patch(&self, key: &(String, String)) -> Option<usize> {
    self.patch_map.get(key).copied()
  }

  fn deserialize<T: Read>(t: T) -> Result<Self> {
    let mut decoder = GzDecoder::new(t);
    let mut package: QuestPackage = serde_json::from_reader(&mut decoder)?;
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
