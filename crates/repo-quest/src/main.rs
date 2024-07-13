use self::stage::Stage;
use anyhow::{Context, Result};
use octocrab::Octocrab;
use quest::Quest;
use std::process::Command;

mod git_repo;
mod github_repo;
mod quest;
mod stage;

fn get_github_token() -> Result<String> {
  let token_output = Command::new("gh")
    .args(["auth", "token"])
    .output()
    .context("Failed to run `gh auth token`")?;
  let token = String::from_utf8(token_output.stdout)?;
  let token_clean = token.trim_end().to_string();
  Ok(token_clean)
}

fn init_octocrab() -> Result<()> {
  let token = get_github_token()?;
  let crab_inst = Octocrab::builder().personal_token(token).build()?;
  octocrab::initialise(crab_inst);
  Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
  let step = std::env::args().nth(1).unwrap().parse::<usize>().unwrap();

  init_octocrab()?;

  let user = octocrab::instance()
    .current()
    .user()
    .await
    .context("Failed to get current user")?
    .login;
  let quest = Quest::new(&user, "rqst-async");
  let stages = [Stage::new(1, "async-await"), Stage::new(2, "spawn")];

  match step {
    1 => quest.create_repo().await?,
    2 => quest.init_repo()?,
    3 => quest.file_feature_and_issue(&stages[0], None).await?,
    4 => quest.file_tests(&stages[0]).await?,
    5 => {
      quest
        .file_feature_and_issue(&stages[1], Some(&stages[0]))
        .await?
    }
    6 => quest.file_tests(&stages[1]).await?,
    _ => todo!(),
  }

  Ok(())
}
