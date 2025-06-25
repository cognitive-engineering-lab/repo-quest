use env::current_dir;
use eyre::{Result, ensure};
use repo_quest::{
  package::QuestPackage,
  quest::{CreateSource, Quest, QuestState},
};
use std::{
  env, fs,
  path::PathBuf,
  process::Command,
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
    repo_quest::init_globals().unwrap();
  });
}

async fn create_test_quest(source: CreateSource) -> Result<Arc<Quest>> {
  let dir = current_dir()?;
  let quest = Quest::create(&dir, source).await?;
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
  ($quest:expr, $a:expr, $b:expr) => {{
    let state = $quest.infer_state().await?;
    match state {
      QuestState::Ongoing { stage, started } => assert_eq!((stage, started), ($a, $b)),
      QuestState::Completed => panic!("finished"),
    };
  }};
}

async fn playthrough(quest: &Quest) -> Result<()> {
  state_is!(quest, 0, false);

  let (pr, issue) = quest.start_stage(0).await?;
  state_is!(quest, 0, true);
  assert_eq!(pr.title.as_ref().unwrap(), "A");
  assert_eq!(issue.title, "A");
  assert_eq!(issue.body.as_ref().unwrap(), "A");

  quest.origin.merge_pr(&pr).await?;
  quest.origin.wait_for_issue_closed(&issue).await?;
  state_is!(quest, 1, false);

  let (pr, issue) = quest.start_stage(1).await?;
  state_is!(quest, 1, true);

  if quest.source.provides_refsol() {
    quest.add_solution(1).await?;
  }
  quest.origin.merge_pr(&pr).await?;
  quest.origin.wait_for_issue_closed(&issue).await?;
  state_is!(quest, 2, false);

  Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn remote_playthrough() -> Result<()> {
  test_quest!(quest);
  playthrough(&quest).await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn local_playthrough() -> Result<()> {
  let status = Command::new("git")
    .args([
      "clone",
      "--mirror",
      &format!("https://github.com/{TEST_ORG}/{TEST_REPO}"),
      TEST_REPO,
    ])
    .status()?;
  ensure!(status.success(), "clone failed");

  let repo_path = env::current_dir().unwrap().join(TEST_REPO);
  let status = Command::new("cargo")
    .args(["run", "--", "pack", &repo_path.display().to_string()])
    .status()?;
  ensure!(status.success(), "pack failed");

  fs::remove_dir_all(repo_path)?;

  let package_path = PathBuf::from(format!("{TEST_REPO}.json.gz"));
  let package = QuestPackage::load_from_file(&package_path)?;
  test_quest!(quest, CreateSource::Package(Box::new(package)));

  playthrough(&quest).await?;

  fs::remove_file(package_path)?;

  Ok(())
}

// TODO: can't seem to run these even sequentially?
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn skip() -> Result<()> {
  test_quest!(quest);

  macro_rules! state_is {
    ($a:expr, $b:expr) => {
      let state = quest.infer_state().await?;
      match state {
        QuestState::Ongoing { stage, started } => assert_eq!((stage, started), ($a, $b)),
        QuestState::Completed => panic!("finished"),
      };
    };
  }

  state_is!(0, false);

  quest.skip_to_stage(1).await?;
  state_is!(1, false);

  quest.skip_to_stage(2).await?;
  state_is!(2, false);

  Ok(())
}
