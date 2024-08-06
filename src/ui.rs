#![allow(non_snake_case)]

use crate::{
  github,
  quest::{self, Quest, QuestState},
  stage::StagePart,
};
use dioxus::{
  desktop::{Config, LogicalSize, WindowBuilder},
  prelude::*,
};
use futures_util::FutureExt;
use std::{ops::Deref, rc::Rc, sync::Arc};
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
fn QuestView(quest: QuestRef) -> Element {
  let mut error_signal = use_signal_sync(|| None::<anyhow::Error>);
  let mut loading_signal = use_context::<SyncSignal<ShowLoading>>();

  let quest_ref = quest.clone();
  use_hook(move || {
    tokio::spawn(async move { quest_ref.infer_state_loop().await });
  });

  let state = quest.state_signal.read().as_ref().unwrap().clone();
  let cur_stage = state.stage.idx;

  rsx! {
    if let Some(err) = &*error_signal.read() {
      pre { "{err:?}" }
    }

    h1 {
      "RepoQuest: "
      {quest.config.title.clone()}
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
                    StagePart::Starter if !quest.stages[stage].config.no_starter()  => "Waiting for you to merge starter PR",
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

fn QuestLoader() -> Element {
  let mut quest_slot = use_signal_sync(|| None::<QuestRef>);
  let state_signal = use_signal_sync(|| None::<QuestState>);
  let mut loading_signal = use_context::<SyncSignal<ShowLoading>>();
  match &*quest_slot.read_unchecked() {
    Some(quest) => rsx! { QuestView { quest: quest.clone() }},
    None => rsx! {
      h1 { "RepoQuest" }
      {
        let config = quest::load_config_from_current_dir();
        match config {
          Ok(config) => {
            let res = use_resource(move || {
              let config = config.clone();
              async move {
                loading_signal.set(ShowLoading(true));
                let quest = Quest::load(config, state_signal).await?;
                quest_slot.set(Some(QuestRef(Arc::new(quest))));
                loading_signal.set(ShowLoading(false));
                Ok::<_, anyhow::Error>(())
              }
            });
            match &*res.read_unchecked() {
              None => rsx! { "Loading current quest..." },
              Some(Ok(())) => unreachable!(),
              Some(Err(e)) => rsx! {
                div { "Failed to load quest with error:" },
                pre { "{e:?}" }
              }
            }
          }
          Err(_) => rsx! { InitForm { quest_slot, state_signal } }
        }
      }
    },
  }
}

#[component]
fn InitForm(
  quest_slot: SyncSignal<Option<QuestRef>>,
  state_signal: SyncSignal<Option<QuestState>>,
) -> Element {
  let mut repo = use_signal(String::new);
  let mut start_init = use_signal(|| false);

  rsx! {
    if *start_init.read() {
      InitView { repo: repo.read_unchecked().clone(), quest_slot, state_signal }
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
  repo: String,
  quest_slot: SyncSignal<Option<QuestRef>>,
  state_signal: SyncSignal<Option<QuestState>>,
) -> Element {
  let quest = use_resource(move || {
    let repo = repo.clone();
    async move {
      tokio::spawn(async move {
        let config = quest::load_config_from_remote("cognitive-engineering-lab", &repo).await?;
        let quest = Quest::load(config, state_signal).await?;
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

#[derive(Clone, Copy)]
struct ShowLoading(bool);

#[component]
fn App() -> Element {
  let init_res = use_hook(|| Rc::new(github::init_octocrab()));
  use_context_provider(|| SyncSignal::new_maybe_sync(ShowLoading(false)));
  let show_loading = use_context::<SyncSignal<ShowLoading>>();

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
        {match &*init_res {
          Ok(()) => rsx!{ QuestLoader { } },
          Err(e) => rsx!{
            div { "Failed to load Github API. Full error:" }
            pre { "{e:?}" }
          },
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
