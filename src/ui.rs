#![allow(non_snake_case)]

use crate::{
  github::{self, GithubToken},
  quest::{self, Quest, QuestConfig, QuestState},
  stage::StagePart,
};
use dioxus::{
  desktop::{Config, LogicalSize, WindowBuilder},
  prelude::*,
};
use futures_util::FutureExt;
use std::{env, ops::Deref, path::PathBuf, rc::Rc, sync::Arc};
use tracing::Level;

macro_rules! error_view {
  ($label:expr, $error:expr) => {{
    rsx! {
      div {
        class: "error",
        div { {format!("{} failed with error:", $label)} }
        pre { {format!("{:?}", $error)} }
      }
    }
  }};
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
  let mut loading_signal = use_context::<SyncSignal<ShowLoading>>();
  let mut title_signal = use_context::<SyncSignal<Title>>();

  let quest_ref = quest.clone();
  let title = quest.config.title.clone();
  use_hook(move || {
    title_signal.set(Title(Some(title)));
    tokio::spawn(async move { quest_ref.infer_state_loop().await });
  });

  let state = quest.state_signal.read().as_ref().unwrap().clone();
  let cur_stage = state.stage.idx;
  let quest_dir = quest.dir.clone();

  rsx! {
    if let Some(err) = &*error_signal.read() {
      pre { "{err:?}" }
    }

    button {
      id: "refresh",
      onclick: move |_| {
        let quest_ref = quest.clone();
        tokio::spawn(async move {
          quest_ref.infer_state_update().await.unwrap();
        });
      },
      "⟳"
    }

    div {
      class: "working-dir",
      "Directory: "
      code { {quest_dir.display().to_string()} }
    }

    ol {
      class: "stages",
      for stage in 0 ..= cur_stage {
        li {
          div {
            span {
              class: "stage-title",
              {quest.stages[stage].config.name.clone()}
            }
            span {
              class: "separator",
              "·"
            }

            if stage == cur_stage {
              if state.status.is_start() {
                button {
                  onclick: {
                    let quest_ref = quest.clone();
                    move |_| {
                      let quest_ref = quest_ref.clone();
                      tokio::spawn(async move {
                        loading_signal.set(ShowLoading(true));
                        let res = match state.part {
                          StagePart::Starter => quest_ref.file_feature_and_issue(cur_stage).boxed(),
                          StagePart::Solution => quest_ref.file_solution(cur_stage).boxed(),
                        }.await;
                        if let Err(e) = res {
                          error_signal.set(Some(e));
                        }
                        loading_signal.set(ShowLoading(false));
                      });
                    }
                  },
                  {match state.part {
                    StagePart::Starter => if quest.stages[stage].config.no_starter() {
                      "File issue"
                    } else {
                      "File issue & starter PR"
                    },
                    StagePart::Solution => "Give solution"
                  }}
                }
              } else {
                span {
                  class: "status",
                  {match state.part {
                    StagePart::Starter if !quest.stages[stage].config.no_starter() => "Waiting for you to merge starter PR",
                    _ => "Waiting for you to solve & close issue"
                  }}
                }
              }
            } else {
              span {
                class: "status",
                "Completed"
              }
            }
          }

          div {
            class: "gh-links",

            if let Some(issue_url) = quest.issue_url(stage) {
              a {
                href: issue_url,
                "Issue"
              }
            }

            if let Some(feature_pr_url) = quest.feature_pr_url(stage) {
              a {
                href: feature_pr_url,
                "Starter PR"
              }
            }

            if let Some(solution_pr_url) = quest.solution_pr_url(stage) {
              a {
                href: solution_pr_url,
                "Solution PR"
              }
            }
          }
        }
      }
    }
  }
}

#[component]
fn ExistingQuestLoader(dir: PathBuf, config: QuestConfig) -> Element {
  let mut loading_signal = use_context::<SyncSignal<ShowLoading>>();
  let mut quest_slot = use_context::<SyncSignal<Option<QuestRef>>>();
  let state_signal = use_context::<SyncSignal<Option<QuestState>>>();
  let res = use_resource(move || {
    let config = config.clone();
    let dir = dir.clone();
    async move {
      loading_signal.set(ShowLoading(true));
      let quest = Quest::load(dir, config, state_signal).await?;
      quest_slot.set(Some(QuestRef(Arc::new(quest))));
      loading_signal.set(ShowLoading(false));
      Ok::<_, anyhow::Error>(())
    }
  });
  match &*res.read_unchecked() {
    None => rsx! { "Loading quest..." },
    Some(Ok(())) => unreachable!(),
    Some(Err(e)) => rsx! {
      div { "Failed to load quest with error:" },
      pre { "{e:?}" }
    },
  }
}

fn QuestLoader() -> Element {
  let quest_slot = use_context_provider(|| SyncSignal::<Option<QuestRef>>::new_maybe_sync(None));
  use_context_provider(|| SyncSignal::<Option<QuestState>>::new_maybe_sync(None));
  match &*quest_slot.read_unchecked() {
    Some(quest) => rsx! { QuestView { quest: quest.clone() }},
    None => {
      let dir = env::current_dir().unwrap();
      let config = QuestConfig::load(&dir);
      match config {
        Ok(config) => rsx! { ExistingQuestLoader { dir, config } },
        Err(_) => rsx! { InitForm {} },
      }
    }
  }
}

fn InitForm() -> Element {
  enum InitState {
    AwaitingInput,
    Remote { dir: PathBuf, repo: String },
    Local(PathBuf),
  }

  let mut new_quest = use_signal(|| false);
  let mut new_dir = use_signal(|| None::<String>);
  let mut repo = use_signal(|| None::<String>);
  let mut state = use_signal(|| InitState::AwaitingInput);

  let cur_state = state.read();
  match &*cur_state {
    InitState::AwaitingInput => rsx! {
      if *new_quest.read() {
        div {
          class: "new-quest",

          div {
            strong { "Start a new quest" }
          }

          div {
            select {
              onchange: move |event| repo.set(Some(event.value())),
              option {
                disabled: true,
                selected: repo.read().is_none(),
                value: "",
                "Choose a quest..."
              }
              option {
                value: "rqst-async",
                "rqst-async"
              }
            }
          }

          div {
            label {
              r#for: "new-quest-dir",
              "Choose a dir"
            }

            if let Some(new_dir) = &*new_dir.read() {
              span {
                class: "selected-file",
                "{new_dir}"
              }
            }

            input {
              id: "new-quest-dir",
              r#type: "file",
              "webkitdirectory": true,
              onchange: move |event| {
                let mut files = event.files().unwrap().files();
                if !files.is_empty() {
                  new_dir.set(Some(files.remove(0)));
                }
              },
            }
          }

          div {
            button {
              disabled: repo.read().is_none() || new_dir.read().is_none(),
              onclick: move |_| state.set(InitState::Remote {
                dir: PathBuf::from(new_dir.read_unchecked().as_ref().unwrap()),
                repo: repo.read_unchecked().as_ref().unwrap().clone()
              }),
              "Create"
            }
          }
        }
      } else {
        div {
          class: "controls",

          button {
            onclick: move |_| new_quest.set(true),
            "Start a new quest"
          }

          label {
            r#for: "load-quest",
            "Load an existing quest"
          }

          input {
            id: "load-quest",
            r#type: "file",
            "webkitdirectory": true,
            onchange: move |event| {
              let mut files = event.files().unwrap().files();
              if !files.is_empty() {
                state.set(InitState::Local(PathBuf::from(files.remove(0))));
              }
            },
          }
        }
      }
    },
    InitState::Remote { dir, repo } => rsx! { InitView { repo: repo.clone(), dir: dir.clone() } },
    InitState::Local(dir) => {
      let config = QuestConfig::load(dir);
      match config {
        Ok(config) => rsx! { ExistingQuestLoader { dir: dir.clone(), config } },
        Err(e) => error_view!(format!("Loading quest from {}", dir.display()), e),
      }
    }
  }
}

#[component]
fn InitView(repo: String, dir: PathBuf) -> Element {
  let state_signal = use_context::<SyncSignal<Option<QuestState>>>();
  let mut quest_slot = use_context::<SyncSignal<Option<QuestRef>>>();
  let mut loading_signal = use_context::<SyncSignal<ShowLoading>>();
  let quest = use_resource(move || {
    let repo = repo.clone();
    let dir = dir.clone();
    async move {
      loading_signal.set(ShowLoading(true));
      let result = tokio::spawn(async move {
        let config = quest::load_config_from_remote("cognitive-engineering-lab", &repo).await?;
        let quest = Quest::load(dir.join(repo), config, state_signal).await?;
        quest.create_repo().await?;
        quest_slot.set(Some(QuestRef(Arc::new(quest))));
        loading_signal.set(ShowLoading(false));
        Ok::<_, anyhow::Error>(())
      })
      .await;
      loading_signal.set(ShowLoading(false));
      result.unwrap()
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

fn GithubLoader() -> Element {
  let token = use_hook(|| Rc::new(github::get_github_token()));
  match token.as_ref() {
    GithubToken::Found(token) => {
      let init_res = use_hook(|| Rc::new(github::init_octocrab(token)));
      match &*init_res {
        Ok(()) => rsx! { QuestLoader { } },
        Err(e) => rsx! {
          div { "Failed to load Github API. Full error:" }
          pre { "{e:?}" }
        },
      }
    }
    GithubToken::Error(err) => error_view!("Github token", err),
    GithubToken::Missing => rsx! {
      div {
        "Before running RepoQuest, you need to provide it access to Github. "
        "Follow the instructions at the link below and restart RepoQuest."
      }
      div {
        a {
          href: "https://github.com/cognitive-engineering-lab/repo-quest/blob/main/README.md#github-token",
          "https://github.com/cognitive-engineering-lab/repo-quest/blob/main/README.md#github-token"
        }
      }
    },
  }
}

// TODO: deal with CWD when launched from app.

#[derive(Clone, Copy)]
struct ShowLoading(bool);

#[derive(Clone)]
struct Title(Option<String>);

#[component]
fn App() -> Element {
  let show_loading = use_context_provider(|| SyncSignal::new_maybe_sync(ShowLoading(false)));
  let title = use_context_provider(|| SyncSignal::new_maybe_sync(Title(None)));

  rsx! {
    link { rel: "stylesheet", href: "main.css" }

    if show_loading.read().0 {
      div {
        id: "loading-cover",

        div {
          id: "spinner"
        }
      }
    }

    div {
      id: "app",
      h1 {
        "RepoQuest"
        if let Some(title) = title.read().0.as_ref() {
          ": {title}"
        }
      }
      GithubLoader {}
    }
  }
}

pub fn launch() {
  let level = if cfg!(debug_assertions) {
    Level::DEBUG
  } else {
    Level::INFO
  };
  dioxus_logger::init(level).expect("failed to init logger");
  LaunchBuilder::desktop()
    .with_cfg(
      Config::new().with_window(
        WindowBuilder::new()
          .with_title("RepoQuest")
          .with_always_on_top(false)
          .with_inner_size(LogicalSize::new(800, 500)),
      ),
    )
    .launch(App);
}
