use anyhow::Result;

mod grader;

fn main() -> Result<()> {
  let mut grader = grader::Grader::new();
  grader.grade();

  Ok(())
}
