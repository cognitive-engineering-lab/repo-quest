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
fn StageView(stage: usize) -> Element {
  let quest = use_context::<QuestRef>();
  let mut loading_signal = use_context::<SyncSignal<ShowLoading>>();
  let mut app_error = use_context::<SyncSignal<AppError>>();

  let state_opt = match quest.state_signal.unwrap().read().as_ref().unwrap() {
    QuestState::Ongoing {
      stage,
      part,
      status,
    } => Some((stage.idx, *part, *status)),
    QuestState::Completed => None,
  };

  let quest_ref = quest.clone();
  let advance_stage = move |_| {
    let quest_ref = quest_ref.clone();
    tokio::spawn(async move {
      loading_signal.set(ShowLoading(true));
      let (stage, part, _) = state_opt.unwrap();
      let res = match part {
        StagePart::Starter => quest_ref
          .file_feature_and_issue(stage)
          .map(|res| res.map(|_| ()))
          .boxed(),
        StagePart::Solution => quest_ref
          .file_solution(stage)
          .map(|res| res.map(|_| ()))
          .boxed(),
      }
      .await;
      if let Err(err) = res {
        app_error.set(AppError::from_error("Running Github action", &err));
      }
      loading_signal.set(ShowLoading(false));
    });
  };

  rsx! {
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

        if let Some((cur_stage, cur_part, cur_status)) = state_opt {
          if stage == cur_stage {
            if cur_status.is_start() {
              {match cur_part {
                StagePart::Starter => rsx!{
                  button {
                    onclick: advance_stage,
                    if quest.stages[cur_stage].config.no_starter() {
                      "File issue"
                    } else {
                      "File issue & starter PR"
                    }
                  }
                },
                StagePart::Solution => rsx! {
                  details {
                    class: "help",

                    summary { "Help" }

                    div {
                      "Try first learning from our reference solution and incorporating it into your codebase. If that doesn't work, we can replace your code with ours."
                    }

                    div {
                      div {
                        a {
                          href: quest.reference_solution_pr_url(cur_stage).unwrap(),
                          "View reference solution"
                        }
                      }

                      div {
                        button {
                          onclick: advance_stage,
                          "File reference solution"
                        }
                      }
                    }
                  }
                }
              }}
            } else {
              span {
                class: "status",
                {match cur_part {
                  StagePart::Starter if !quest.stages[stage].config.no_starter() => "Waiting for you to merge starter PR",
                  _ => "Waiting for you to merge solution PR and close issue"
                }}
              }
            }
          } else {
            span {
              class: "status",
              "Completed"
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

fn SetChapter() -> Element {
  let mut loading_signal = use_context::<SyncSignal<ShowLoading>>();
  let mut dialog_signal = use_signal(|| false);
  let quest = use_context::<QuestRef>();
  let mut selected = use_signal(|| None::<usize>);

  let cur_stage = match quest.state_signal.unwrap().read().as_ref().unwrap() {
    QuestState::Ongoing { stage, .. } => stage.idx,
    QuestState::Completed => quest.stages.len(),
  };

  rsx! {
    select {
      onchange: move |event| selected.set(Some(event.value().parse::<usize>().unwrap())),

      option {
        disabled: true,
        selected: selected.read().is_none(),
        value: "",
        "Choose a chapter..."
      }

      for (i, stage) in quest.stages.iter().enumerate().filter(|(i, _)| *i > cur_stage) {
        option {
          value: i.to_string(),
          {format!("Chapter {i}: {}", stage.config.name)}
        }
      }
    }

    if *dialog_signal.read() {
      dialog {
        "open": true,

        "Skipping a chapter will irrevocably overwrite any changes in your repository! This cannot be undone! Are you sure you want to proceed?"

        form {
          method: "dialog",

          button {
            onclick: move |_| {
              dialog_signal.set(false);
              let stage_index = *selected.read_unchecked().as_ref().unwrap();
              let quest_ref = quest.clone();
              tokio::spawn(async move {
                loading_signal.set(ShowLoading(true));
                quest_ref.hard_reset(stage_index).await.unwrap();
                loading_signal.set(ShowLoading(false));
              });
            },

            "Yes"
          }
          button {
            onclick: move |_| {
              dialog_signal.set(false);
            },
            "No"
          }
        }
      }
    }

    button {
      disabled: selected.read().is_none(),
      onclick: move |_| {
        dialog_signal.set(true);
      },
      "Skip to chapter"
    }
  }
}

fn QuestView() -> Element {
  let quest = use_context::<QuestRef>();
  let mut title_signal = use_context::<SyncSignal<Title>>();

  let quest_ref = quest.clone();
  let title = quest.config.title.clone();
  use_hook(move || {
    title_signal.set(Title(Some(title)));
    tokio::spawn(async move { quest_ref.infer_state_loop().await });
  });

  let state = quest.state_signal.unwrap().read().as_ref().unwrap().clone();
  let cur_stage = match state {
    QuestState::Ongoing { stage, .. } => stage.idx,
    QuestState::Completed => quest.stages.len() - 1,
  };
  let quest_dir = quest.dir.display().to_string();

  rsx! {
    div {
      class: "columns",

      div {
        ol {
          start: "0",
          class: "stages",
          for stage in 0 ..= cur_stage {
            StageView { stage }
          }
        }
      }

      div {
        class: "meta",

        h2 {
          "Controls"
        }

        div {
          div {
            button {
              onclick: move |_| {
                let quest_ref = quest.clone();
                tokio::spawn(async move {
                  quest_ref.infer_state_update().await.unwrap();
                });
              },
              "Refresh state"
            }
          }

          div {
            button {
              onclick: move |_| {
                eval(&format!(r#"navigator.clipboard.writeText("{quest_dir}")"#));
              },
              "Copy directory to 📋"
            }
          }

          div {
            SetChapter {}
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
      let quest = Quest::load(dir, config, Some(state_signal)).await?;
      quest_slot.set(Some(QuestRef(Arc::new(quest))));
      loading_signal.set(ShowLoading(false));
      Ok::<_, anyhow::Error>(())
    }
  });
  match &*res.read_unchecked() {
    None => rsx! { "Loading quest..." },
    Some(Ok(())) => unreachable!(),
    Some(Err(e)) => rsx! {
      div { "Failed to load quest with error:" }
      pre { "{e:?}" }
    },
  }
}

fn QuestLoader() -> Element {
  let quest_slot = use_context_provider(|| SyncSignal::<Option<QuestRef>>::new_maybe_sync(None));
  use_context_provider(|| SyncSignal::<Option<QuestState>>::new_maybe_sync(None));
  match &*quest_slot.read_unchecked() {
    Some(quest) => {
      let quest_ref = quest.clone();
      use_context_provider(move || quest_ref);
      rsx! { QuestView { }}
    }
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
  let mut app_error = use_context::<SyncSignal<AppError>>();

  let cur_state = state.read();
  match &*cur_state {
    InitState::AwaitingInput => rsx! {
      if *new_quest.read() {
        div {
          class: "new-quest",

          div {
            strong { "Start a new quest" }
          }

          table {
            tr {
              td { "Quest:" }
              td {
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
            }

            tr {
              td { "Directory:" }
              td {
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
        Err(e) => {
          app_error.set(AppError::from_error(
            format!("Loading quest from directory: {}", dir.display()),
            &e,
          ));
          rsx! {}
        }
      }
    }
  }
}

#[component]
fn InitView(repo: String, dir: PathBuf) -> Element {
  let state_signal = use_context::<SyncSignal<Option<QuestState>>>();
  let mut quest_slot = use_context::<SyncSignal<Option<QuestRef>>>();
  let mut loading_signal = use_context::<SyncSignal<ShowLoading>>();
  let mut app_error = use_context::<SyncSignal<AppError>>();
  let quest = use_resource(move || {
    let repo = repo.clone();
    let dir = dir.clone();
    async move {
      loading_signal.set(ShowLoading(true));
      let result = tokio::spawn(async move {
        let config = quest::load_config_from_remote("cognitive-engineering-lab", &repo).await?;
        let quest = Quest::load(dir.join(repo), config, Some(state_signal)).await?;
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
    None => rsx! { "Initializing quest..." },
    Some(Err(e)) => {
      app_error.set(AppError::from_error("Initializing quest", e));
      rsx! {}
    }
    Some(Ok(())) => rsx! { "Unreachable?" },
  }
}

fn GithubLoader() -> Element {
  let token = use_hook(|| Rc::new(github::get_github_token()));
  let mut app_error = use_context::<SyncSignal<AppError>>();
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
    GithubToken::Error(err) => {
      app_error.set(AppError::from_error("Loading GitHub API", err));
      rsx! {}
    }
    GithubToken::NotFound => rsx! {
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

#[derive(Clone)]
struct AppError(Option<(String, String)>);

impl AppError {
  pub fn from_error(action: impl Into<String>, error: &anyhow::Error) -> Self {
    AppError(Some((action.into(), format!("{error:?}"))))
  }
}

const _: &str = manganis::mg!(file("./assets/normalize.css"));
const _: &str = manganis::mg!(file("./assets/main.css"));

#[component]
fn App() -> Element {
  let show_loading = use_context_provider(|| SyncSignal::new_maybe_sync(ShowLoading(false)));
  let title = use_context_provider(|| SyncSignal::new_maybe_sync(Title(None)));
  let app_error = use_context_provider(|| SyncSignal::new_maybe_sync(AppError(None)));

  rsx! {
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

      {match &app_error.read().0 {
        Some((action, error)) => rsx!(div {
          class: "error",

          div {
            class: "action",
            "Fatal error while: {action}"
          }

          div {
            "RepoQuest encountered an unrecoverable error. Please fix the issue and restart RepoQuest, or contact the developers for support. The backtrace is below."
          }

          pre { {error.clone()} }
        }),
        None => rsx!(GithubLoader {})
      }}
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
