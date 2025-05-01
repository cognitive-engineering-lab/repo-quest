use std::{env::current_dir, path::Path};

use crossterm::style::Stylize;
use eyre::{Result, bail};
use inquire::{
  CustomUserError,
  validator::{ErrorMessage, Validation},
};
use rq_core::{
  git::GitRepo,
  github::{self, GithubToken},
  package::QuestPackage,
  quest::{CreateSource, NoopEmitter, Quest, QuestState},
  stage::{StagePart, StagePartStatus},
};

use crate::{
  file_completer::{FilePathCompleter, FilePathValidator},
  spinner::spinner,
};

#[derive(strum::Display)]
enum QuestSourceType {
  #[strum(to_string = "Github repository")]
  Github,
  #[strum(to_string = "Local path")]
  Local,
}

async fn new_quest_ui(cwd: &Path) -> Result<()> {
  println!(
    "\n{}\nYou are running RepoQuest outside a quest directory, so we will start a new quest.",
    "New Quest".bold()
  );
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

      CreateSource::Package(package)
    }
  };

  let quest_fut = Quest::create(cwd, source, Box::new(NoopEmitter));
  let quest = spinner("Creating quest...", quest_fut).await?;

  println!(
    "\nQuest created in directory: {}\n\nYou should open that directory in your preferred code editor, then start the quest by running:\n\n$ cd {}\n$ repo-quest",
    quest.dir.display(),
    quest.dir.file_name().unwrap().to_string_lossy()
  );

  Ok(())
}

const FEEDBACK_FORM: &str = "https://forms.gle/R3W4h6DLo272Zy1L9";

async fn run_quest_ui(quest: Quest) -> Result<()> {
  let desc = spinner("...", quest.state_descriptor()).await?;

  println!(
    "\n{} {}",
    "Current quest:".bold(),
    quest.config.title.as_str().bold(),
  );

  match desc.state {
    QuestState::Ongoing {
      stage: stage_index,
      part,
      status,
    } => {
      let stage = &quest.stages()[stage_index as usize];
      let state = &desc.stages[stage_index as usize];
      println!(
        "{}",
        format!(
          "Chapter {}/{}: {}",
          stage_index + 1,
          quest.config.stages.len(),
          stage.name
        )
        .bold()
      );
      match part {
        StagePart::Starter => match status {
          StagePartStatus::Start => {
            let file_text = if stage.no_starter() {
              "File issue"
            } else {
              "File issue and starter PR"
            };
            let action =
              inquire::Select::new("Action:", vec![file_text, "Exit"]).prompt_skippable()?;
            match action {
              Some(action) if action == file_text => {
                let (pr, issue) = spinner(
                  "Filing...",
                  quest.file_feature_and_issue(stage_index as usize),
                )
                .await?;
                println!("Issue: {}", issue.html_url);
                if let Some(pr) = pr {
                  println!("Pull request: {}", pr.html_url.unwrap());
                  println!("Read the issue and pull request. Merge the PR, then start coding.")
                } else {
                  println!("Read the issue and then start coding.")
                }
                println!("If you finish or if you need help, re-run repo-quest in this directory.")
              }
              Some("Exit") | None => return Ok(()),
              _ => unreachable!(),
            }
          }
          StagePartStatus::Ongoing => {
            println!(
              "{} Waiting for you to merge the starter code PR: {}\nRerun repo-quest when you have completed this step.",
              "Status:".bold(),
              state.feature_pr_url.as_ref().unwrap()
            );
          }
        },
        StagePart::Solution => match status {
          StagePartStatus::Start => match &state.reference_solution_pr_url {
            Some(url) => {
              println!(
                "{} Waiting for you to close the issue: {}\nOr get help by selecting an action below.",
                "Status:".bold(),
                state.issue_url.as_ref().unwrap()
              );
              let action = inquire::Select::new(
                "Action:",
                vec![
                  "View reference solution",
                  "File reference solution PR",
                  "Exit",
                ],
              )
              .prompt_skippable()?;
              match action {
                Some("View reference solution") => println!("Open this link: {url}"),
                Some("File reference solution PR") => {
                  let pr = spinner("Filing...", quest.file_solution(stage_index as usize)).await?;
                  println!(
                    "Review and merge this PR: {}\nThen rerun repo-quest when that's done.",
                    pr.html_url.unwrap()
                  );
                }
                Some("Exit") | None => return Ok(()),
                _ => unreachable!(),
              }
            }
            None => println!(
              "{} Waiting for you to close the issue: {}\nRerun repo-quest when you have completed this step.",
              "Status:".bold(),
              state.issue_url.as_ref().unwrap()
            ),
          },
          StagePartStatus::Ongoing => println!(
            "{} Waiting for you to merge the solution PR: {}\nAnd close the issue: {}\nThen rerun repo-quest when you have completed this step.",
            "Status:".bold(),
            state.reference_solution_pr_url.as_ref().unwrap(),
            state.issue_url.as_ref().unwrap()
          ),
        },
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

pub async fn ui_main() -> Result<()> {  
  let cwd = current_dir()?;
  let in_repo = GitRepo::new(&cwd).exists();

  if in_repo {
    let quest_fut = Quest::load(&cwd, Box::new(NoopEmitter));
    let quest = spinner("Loading quest from current directory...", quest_fut).await?;
    run_quest_ui(quest).await
  } else {
    new_quest_ui(&cwd).await
  }
}
