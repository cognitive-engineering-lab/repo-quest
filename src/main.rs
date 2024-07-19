#![allow(non_snake_case)]

use anyhow::{anyhow, ensure, Context, Result};
use dioxus::prelude::*;
use futures_util::FutureExt;
use octocrab::Octocrab;
use quest::{Quest, QuestState};
use regex::Regex;
use stage::StagePart;
use std::{ops::Deref, process::Command, sync::Arc};
use tracing::{debug, Level};

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

#[derive(Clone)]
struct QuestRef(Arc<Quest>);

impl PartialEq for QuestRef {
  fn eq(&self, other: &Self) -> bool {
    std::ptr::eq(&*self.0, &*other.0)
  }
}

impl Deref for QuestRef {
  type Target = Quest;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

#[component]
fn QuestView(quest: QuestRef) -> Element {
  let mut error_signal = use_signal_sync(|| None::<anyhow::Error>);
  let mut loading_signal = use_signal_sync(|| false);

  let quest_ref = quest.clone();
  use_hook(move || {
    tokio::spawn(async move { quest_ref.infer_state_loop().await });
  });

  let state = quest.state.read().as_ref().unwrap().clone();
  let cur_stage = state.stage.idx();

  let loading = *loading_signal.read();
  rsx! {
    if let Some(err) = &*error_signal.read() {
      pre { "{err:?}" }
    }

    button {
      onclick: move |_| {
        let quest_ref = quest.clone();
        tokio::spawn(async move {
          quest_ref.infer_state_update().await;
        });
      },
      "⟳"
    }

    ol {
      for stage in 0 ..= cur_stage {
        li {
          div { {quest.stages[stage].name.clone()} }
          if stage == cur_stage {
            div {
              button {
                disabled: loading || state.status.is_ongoing(),
                onclick: {
                  let quest_ref = quest.clone();
                  move |_| {
                    let quest_ref = quest_ref.clone();
                    loading_signal.set(true);
                    tokio::spawn(async move {
                      let res = match state.part {
                        StagePart::Feature => quest_ref.file_feature_and_issue(cur_stage).boxed(),
                        StagePart::Test => quest_ref.file_tests(cur_stage).boxed(),
                        StagePart::Solution => quest_ref.file_solution(cur_stage).boxed(),
                      }.await;
                      if let Err(e) = res {
                        error_signal.set(Some(e));
                      }
                      loading_signal.set(false);
                    });
                  }
                },
                {match state.part {
                  StagePart::Feature => "File issue & features",
                  StagePart::Test => "File tests",
                  StagePart::Solution => "Give solution"
                }}
              }

              if loading {
                div { "Operation running..." }
              }

              if state.status.is_ongoing() {
                div {
                  {match state.part {
                    StagePart::Feature => "Merge PR before continuing",
                    StagePart::Test => "Merge PR before continuing",
                    StagePart::Solution => "File and merge your own PR and close the issue before continuing"
                  }}
                }
              }
            }
          }
        }
      }
    }
  }
}

fn infer_quest_name() -> Result<String> {
  let output = Command::new("git")
    .args(["remote", "get-url", "upstream"])
    .output()
    .context("git failed")?;
  ensure!(
    output.status.success(),
    "git exited with non-zero status code"
  );
  let stdout = String::from_utf8(output.stdout)?.trim_end().to_string();

  // TODO: parsing with a regex is hacky, but the url crate seemed to not work on SSH URLs...
  let re = Regex::new(r"^git@github.com:[^/]+/([\w\d-]+)\.git$").unwrap();
  let cap = re
    .captures(&stdout)
    .ok_or_else(|| anyhow!("Failed to parse: {stdout}"))?;
  Ok(cap.get(1).unwrap().as_str().to_string())
}

#[component]
fn QuestLoader(user: String) -> Element {
  let quest_name = use_signal(|| match infer_quest_name() {
    Ok(name) => Some(name),
    Err(e) => {
      debug!("Failed to infer quest name with error:\n{e:?}");
      None
    }
  });
  let state = use_signal_sync(|| None::<QuestState>);
  let mut quest_slot = use_signal_sync(|| None::<QuestRef>);
  match (&*quest_slot.read_unchecked(), &*quest_name.read_unchecked()) {
    (Some(quest), _) => rsx! { QuestView { quest: quest.clone() }},
    (a, b) => rsx! {
      h1 { "RepoQuest" }
      {match (a, b) {
        (None, Some(quest_name)) => {
          let quest_name = quest_name.clone();
          let res = use_resource(move || {
            let user = user.clone();
            let quest_name = quest_name.clone();
            async move {
              let quest = Quest::load(&user, &quest_name, state).await?;
              quest_slot.set(Some(QuestRef(Arc::new(quest))));
              Ok::<_, anyhow::Error>(())
            }
          });
          match &*res.read_unchecked() {
            None => rsx! { "Loading current quest..." },
            Some(Ok(())) => rsx! { "Unreachable?" },
            Some(Err(e)) => rsx! {
              div { "Failed to load quest with error:" },
              pre { "{e:?}" }
            },
          }
        }
        (None, None) => rsx! { InitForm { user, quest_slot, state } },
        _ => unreachable!()
      }}
    },
  }
}

#[component]
fn InitForm(
  user: String,
  quest_slot: SyncSignal<Option<QuestRef>>,
  state: SyncSignal<Option<QuestState>>,
) -> Element {
  let mut repo = use_signal(String::new);
  let mut start_init = use_signal(|| false);

  rsx! {
    if *start_init.read() {
      InitView { user, repo: repo.read_unchecked().clone(), quest_slot, state }
    } else {
      input { oninput: move |event| repo.set(event.value()) }
      button {
        onclick: move |_| start_init.set(true),
        "Create"
      }
    }
  }
}

#[component]
fn InitView(
  user: String,
  repo: String,
  quest_slot: SyncSignal<Option<QuestRef>>,
  state: SyncSignal<Option<QuestState>>,
) -> Element {
  let quest = use_resource(move || {
    let user = user.clone();
    let repo = repo.clone();
    async move {
      tokio::spawn(async move {
        let quest = Quest::load(&user, &repo, state).await?;
        quest.create_repo().await?;
        quest_slot.set(Some(QuestRef(Arc::new(quest))));
        Ok::<_, anyhow::Error>(())
      })
      .await
      .unwrap()
    }
  });

  match &*quest.read_unchecked() {
    None => rsx! { "Initializing repo..." },
    Some(Err(e)) => rsx! {
      div { "Failed to initialize repo with error:" }
      pre { "{e:?}" }
    },
    Some(Ok(())) => rsx! { "Unreachable?" },
  }
}

#[component]
fn App() -> Element {
  let user_res = use_resource(|| async move {
    init_octocrab()?;
    let user = octocrab::instance()
      .current()
      .user()
      .await
      .context("Failed to get current user")?;
    Ok::<_, anyhow::Error>(user.login)
  });

  rsx! {
      link { rel: "stylesheet", href: "main.css" }
      {match &*user_res.read_unchecked() {
        Some(Ok(user)) => rsx!{ QuestLoader { user } },
        Some(Err(e)) => rsx!{
          div { "Failed to load Github API. Full error:" }
          pre { "{e:?}" }
        },
        None => rsx!{ "Loading Github API..." }
      }}
  }
}

fn main() {
  dioxus_logger::init(Level::DEBUG).expect("failed to init logger");
  dioxus::launch(App);
}

// #[tokio::main]
// async fn main() -> Result<()> {
//   let step = std::env::args().nth(1).unwrap().parse::<usize>().unwrap();

// let
//   let stages = [Stage::new(1, "async-await"), Stage::new(2, "spawn")];

//   match step {
//     1 => quest.create_repo().await?,
//     2 => quest.init_repo()?,
//     3 => quest.file_feature_and_issue(&stages[0], None).await?,
//     4 => quest.file_tests(&stages[0]).await?,
//     5 => {
//       quest
//         .file_feature_and_issue(&stages[1], Some(&stages[0]))
//         .await?
//     }
//     6 => quest.file_tests(&stages[1]).await?,
//     _ => todo!(),
//   }

//   Ok(())
// }
