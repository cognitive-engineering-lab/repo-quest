use std::{
  io::{Read, Write},
  path::PathBuf,
  process::{Command, Stdio},
  thread::sleep,
  time::{Duration, Instant},
};

use colored::Colorize;

type PropResult = Result<(), String>;

trait Property {
  fn name(&self) -> String;
  fn satisfies(&self, input: &str) -> PropResult;
}

impl<S, F> Property for (S, F)
where
  S: AsRef<str>,
  F: Fn(&str) -> PropResult,
{
  fn name(&self) -> String {
    self.0.as_ref().to_string()
  }

  fn satisfies(&self, output: &str) -> PropResult {
    self.1(output)
  }
}

type TestSet = (&'static str, Vec<Box<dyn Property>>);

struct Spec {
  tests: Vec<TestSet>,
}

fn guessing_game_spec() -> Spec {
  fn parts_to_props(parts: &'static [(&'static str, &'static str)]) -> Vec<Box<dyn Property>> {
    parts
      .iter()
      .enumerate()
      .map(|(i, (name, contents))| {
        Box::new((name, move |output: &str| {
          let prefix = parts[..i]
            .iter()
            .map(|(_, s)| *s)
            .collect::<Vec<_>>()
            .join("");
          let output_fragment = if i > 0 {
            output.strip_prefix(&prefix).unwrap()
          } else {
            &output
          };
          if output_fragment.starts_with(contents) {
            Ok(())
          } else {
            let diff = prettydiff::diff_lines(output_fragment.trim_end(), contents.trim_end());
            let diff_indent = textwrap::indent(&diff.format(), "  ");
            let err_msg = format!("The diff is:\n{diff_indent}");
            Err(err_msg)
          }
        })) as Box<dyn Property>
      })
      .collect()
  }

  let happy_path_test = (
    "101\n",
    parts_to_props(&[
      (
        "Task 1: Prints the right initial strings",
        "Guess the number!\nPlease input your guess.\n",
      ),
      ("Task 2: Accepts an input", "You guessed: 101\n"),
      ("Task 3: Indicates a direction", "Too big!\n"),
    ]),
  );

  let error_handling = (
    "foobar\n",
    parts_to_props(&[
      (
        "Prints the right initial strings",
        "Guess the number!\nPlease input your guess.\n",
      ),
      ("Handles an invalid input", "Please type a number!"),
    ]),
  );

  Spec {
    tests: vec![happy_path_test, error_handling],
  }
}

pub struct Grader {}

fn run_timeout(timeout: Duration, mut f: impl FnMut() -> bool) -> Result<(), String> {
  let start = Instant::now();
  loop {
    if f() {
      return Ok(());
    } else if start.elapsed() > timeout {
      return Err("Binary timed out".to_string());
    } else {
      sleep(Duration::from_millis(10));
    }
  }
}

impl Grader {
  pub fn new() -> Self {
    Grader {}
  }

  fn exec(&self, input: &str) -> Result<String, String> {
    let mut build_cmd = Command::new("cargo");
    build_cmd
      .arg("build")
      .spawn()
      .map_err(|e| e.to_string())?
      .wait()
      .map_err(|e| e.to_string())?;

    let mut cmd = Command::new("cargo");
    cmd.args(["run", "-q"]);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    stdin
      .write_all(input.as_bytes())
      .map_err(|e| e.to_string())?;

    let _ = run_timeout(Duration::from_millis(500), || {
      child.try_wait().unwrap().is_some()
    });

    child.kill().map_err(|e| e.to_string())?;

    run_timeout(Duration::from_millis(500), || {
      child.try_wait().unwrap().is_some()
    })?;

    let mut stdout_buf = String::new();
    stdout
      .read_to_string(&mut stdout_buf)
      .map_err(|e| e.to_string())?;
    Ok(stdout_buf)
  }

  pub fn grade(&mut self) {
    let spec = guessing_game_spec();
    for (input, props) in spec.tests {
      let output = match self.exec(input) {
        Ok(output) => output,
        Err(e) => {
          println!(
            "{}\n{}",
            "✗ Binary failed to execute".red(),
            textwrap::indent(&e, "  ")
          );
          return;
        }
      };

      for prop in props {
        let name = prop.name();
        match prop.satisfies(&output) {
          Ok(()) => println!("{} {}", "✓".green(), name.green()),
          Err(err) => {
            let err_indent = textwrap::indent(&err, "  ");
            eprintln!("{} {}\n{err_indent}", "✗".red(), name.red(),);
            return;
          }
        }
      }
    }
  }
}
