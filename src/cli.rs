use std::{
  env::current_dir,
  path::{Path, PathBuf},
};

use crate::{
  git::GitRepo,
  package::QuestPackage,
  quest::{CreateSource, Quest, QuestState, QuestStrictness, QuestUserPrefs},
};
use clap::{Parser, Subcommand};
use crossterm::style::{StyledContent, Stylize};
use eyre::Result;
use inquire::{
  CustomUserError,
  validator::{ErrorMessage, Validation},
};
use terminal_colorsaurus::ColorScheme;

use self::{
  file_completer::{FilePathCompleter, FilePathValidator},
  spinner::spinner,
};

mod file_completer;
mod spinner;

#[derive(strum::Display)]
enum QuestSourceType {
  #[strum(to_string = "Github repository")]
  Github,
  #[strum(to_string = "Local path")]
  Local,
}

struct Ui {
  scheme: ColorScheme,
}

const FEEDBACK_FORM: &str = "https://forms.gle/R3W4h6DLo272Zy1L9";

impl Ui {
  fn new() -> Result<Self> {
    let scheme = terminal_colorsaurus::color_scheme(terminal_colorsaurus::QueryOptions::default())?;
    Ok(Ui { scheme })
  }

  fn emph(&self, s: impl Into<String>) -> StyledContent<String> {
    s.into().bold()
  }

  fn code(&self, s: impl Into<String>) -> StyledContent<String> {
    match self.scheme {
      ColorScheme::Light => s.into().dark_blue(),
      ColorScheme::Dark => s.into().cyan(),
    }
  }

  async fn new_quest_ui(&self, cwd: &Path) -> Result<()> {
    println!("\n{}", self.emph("New Quest"));
    println!("You are running RepoQuest outside a quest directory, so we will start a new quest.",);
    println!("\nFirst, tell me where you want to load the quest from.");
    let source_type = inquire::Select::new(
      "Quest source:",
      vec![QuestSourceType::Github, QuestSourceType::Local],
    )
    .prompt()?;

    let source = match source_type {
      QuestSourceType::Github => {
        fn validator(val: &str) -> std::result::Result<Validation, CustomUserError> {
          if val.split_once("/").is_some() {
            Ok(Validation::Valid)
          } else {
            Ok(Validation::Invalid(ErrorMessage::Custom(
            "Github repository must be of the form \"[organization]/[name]\" (without the quotes)"
              .to_string(),
          )))
          }
        }
        fn suggester(val: &str) -> std::result::Result<Vec<String>, CustomUserError> {
          Ok(
            ["cognitive-engineering-lab/rqst-async".to_string()]
              .into_iter()
              .filter(|s| s.starts_with(val))
              .collect(),
          )
        }
        println!(
          "\nNext, give me the Github repo in the format [organization]/[name], or select from the suggestions below."
        );
        let repo = inquire::Text::new("Github repo:")
          .with_autocomplete(suggester)
          .with_validator(validator)
          .prompt()?;

        let (user, repo) = repo.split_once("/").unwrap();

        CreateSource::Remote {
          user: user.to_string(),
          repo: repo.to_string(),
        }
      }

      QuestSourceType::Local => {
        let path = inquire::Text::new("Local path:")
          .with_autocomplete(FilePathCompleter::default())
          .with_validator(FilePathValidator::default())
          .prompt()?;

        let package = QuestPackage::load_from_file(Path::new(&path))?;

        CreateSource::Package(Box::new(package))
      }
    };

    println!(
      "\nFinally, choose whether you want to play this quest in relaxed mode (lints disabled) or strict mode (lints enabled in CI and local githooks)."
    );
    let strictness: QuestStrictness = inquire::Select::new(
      "Mode:",
      vec![QuestStrictness::Relaxed, QuestStrictness::Strict],
    )
    .prompt()?;
    let prefs = QuestUserPrefs { strictness };

    let quest_fut = Quest::create(cwd, source, prefs);
    let quest = spinner("Creating quest...", quest_fut).await?;

    println!(
      "\nQuest created in directory: {}",
      self.emph(quest.dir.to_string_lossy()),
    );
    println!(
      "\nYou should open that directory in your preferred code editor, then start the quest by running:"
    );
    println!(
      "\n$ {}",
      self.code(format!(
        "cd {}",
        quest.dir.file_name().unwrap().to_string_lossy()
      ))
    );
    println!("$ {}", self.code("repo-quest"));

    Ok(())
  }

  async fn run_quest_ui(&self, quest: Quest) -> Result<()> {
    let desc = spinner("...", quest.state_descriptor()).await?;

    println!(
      "\n{} {}",
      self.emph("Current quest:"),
      self.emph(&quest.config.title),
    );

    match desc.state {
      QuestState::Ongoing {
        stage: stage_index,
        started,
      } => {
        let stage = &quest.stages()[stage_index as usize];
        let state = &desc.stages[stage_index as usize];
        println!(
          "{}",
          self.emph(format!(
            "Chapter {}/{}: {}",
            stage_index + 1,
            quest.config.stages.len(),
            stage.name
          ))
        );

        if !started {
          let action =
            inquire::Select::new("Action:", vec!["Start chapter", "Exit"]).prompt_skippable()?;

          match action {
            Some("Start chapter") => {
              let (pr, issue) =
                spinner("Starting...", quest.start_stage(stage_index as usize)).await?;
              println!("\nIssue:        {}", issue.html_url);
              println!("Pull request: {}", pr.html_url.unwrap());
              if stage.no_starter() {
                print!("\nStart by reading the issue.")
              } else {
                print!("\nStart by reading the issue and the starter code in the PR.")
              }
              println!(
                " Then commit your solution to the current branch and push, then merge the PR when ready."
              );
              println!(
                "If you finish, or if you need help, then re-run repo-quest in this directory."
              )
            }

            Some("Exit") | None => return Ok(()),

            _ => unreachable!(),
          }
        } else {
          match &state.refsol_url {
            Some(url) => {
              println!(
                "{} Waiting for you to complete the PR and merge it: {}\nOr get help by selecting an action below.",
                self.emph("Status:"),
                state.pr_url.as_ref().unwrap()
              );

              let action = inquire::Select::new(
                "Action:",
                vec![
                  "View reference solution",
                  "Add reference solution to PR",
                  "Exit",
                ],
              )
              .prompt_skippable()?;

              match action {
                Some("View reference solution") => println!("Open this link: {url}"),

                Some("Add reference solution to PR") => {
                  spinner("Adding...", quest.add_solution(stage_index as usize)).await?;
                  println!(
                    "The reference solution has been added to your PR: {}\nReview the changes and merge when you're ready.\nThen, re-run repo-quest to start the next stage.",
                    state.pr_url.as_ref().unwrap()
                  )
                }

                Some("Exit") | None => return Ok(()),

                _ => unreachable!(),
              }
            }

            None => println!(
              "{} Waiting for you to complete the PR and merge it: {}\nOr get help by selecting an action below.",
              self.emph("Status:"),
              state.pr_url.as_ref().unwrap()
            ),
          }
        }
      }

      QuestState::Completed => {
        let num_things = if quest.config.final_url.is_some() {
          "Two things"
        } else {
          "One thing"
        };
        println!("You have completed the quest! {num_things} before you leave.");
        println!(
          "- If you have any feedback about your experience, please tell us here: {FEEDBACK_FORM}"
        );
        if let Some(final_url) = &quest.config.final_url {
          println!("- Please take this quiz which reviews the material in this quest: {final_url}");
          println!(
            "  Taking the quiz helps us evaluate the efficacy of RepoQuest and this quest in particular!"
          )
        }
      }
    }

    Ok(())
  }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
  #[command(subcommand)]
  command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
  /// Skip to a given chapter.
  Skip { chapter: usize },

  /// Package a quest template into a quest file.
  Pack { path: PathBuf },
}

pub async fn main() -> Result<()> {
  let args = Cli::parse();

  let ui = Ui::new()?;

  println!(
    "{}",
    ui.emph(format!(
      "Welcome to RepoQuest v{}!",
      env!("CARGO_PKG_VERSION")
    ))
  );

  let cwd = current_dir()?;

  match args.command {
    None => {
      let in_repo = GitRepo::new(&cwd).exists();
      if in_repo {
        let quest_fut = Quest::load(&cwd);
        let quest = spinner("Loading quest from current directory...", quest_fut).await?;
        ui.run_quest_ui(quest).await?;
      } else {
        ui.new_quest_ui(&cwd).await?;
      }
    }
    Some(Command::Skip { chapter }) => {
      let quest_fut = Quest::load(&cwd);
      let quest = spinner("Loading quest from current directory...", quest_fut).await?;
      spinner(
        format!("Advancing to Chapter {chapter}..."),
        quest.skip_to_stage(chapter),
      )
      .await?;
      println!("Done!");
    }
    Some(Command::Pack { path }) => {
      let package = QuestPackage::build(&path).await?;
      let dst = format!("{}.json.gz", package.config.repo);
      package.save(Path::new(&dst))?;
      println!("Successfully generated quest package: {dst}");
    }
  };

  Ok(())
}
