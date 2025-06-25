use eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
  repo_quest::init_globals()?;
  repo_quest::cli::main().await
}
