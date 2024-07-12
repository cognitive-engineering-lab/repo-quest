use self::repo::Repo;
use anyhow::Result;
use octocrab::Octocrab;
use std::process::Command;

mod repo;

fn get_github_token() -> Result<String> {
  let token_output = Command::new("gh").args(["auth", "token"]).output()?;
  let token = String::from_utf8(token_output.stdout)?;
  let token_clean = token.trim_end().to_string();
  Ok(token_clean)
}

#[tokio::main]
async fn main() -> Result<()> {
  let step = std::env::args().nth(1).unwrap().parse::<usize>().unwrap();

  let token = get_github_token()?;
  let crab_inst = Octocrab::builder().personal_token(token).build()?;
  octocrab::initialise(crab_inst);

  let base = Repo::new("cognitive-engineering-lab", "rqst-async");
  let user = octocrab::instance().current().user().await?.login;
  let fork = Repo::new(&user, "rqst-async");

  match step {
    1 => fork.fork_from(&base).await?,
    2 => {
      let pr = base.pr("01a-async-await").await.unwrap();
      fork.copy_pr(&base, pr).await?;

      let issue = base
        .issue("Use chatbot model in place of a fixed response")
        .await
        .unwrap();
      fork.copy_issue(issue).await?;
    }
    3 => {
      let pr = base.pr("01b-async-await").await.unwrap();
      fork.copy_pr(&base, pr).await?;
    }
    _ => todo!(),
  }

  Ok(())
}
