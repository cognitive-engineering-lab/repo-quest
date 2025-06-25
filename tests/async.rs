use eyre::Result;
use std::env::current_dir;

use repo_quest::quest::{CreateSource, Quest};

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn async_quest() -> Result<()> {
  repo_quest::init_globals()?;

  let source = CreateSource::Remote {
    user: "cognitive-engineering-lab".into(),
    repo: "rqst-async".into(),
  };
  let dir = current_dir()?;
  let quest = Quest::create(&dir, source).await?;
  // quest.start_stage(0).await?;
  quest.skip_to_stage(1).await?;

  Ok(())
}
