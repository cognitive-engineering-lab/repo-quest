// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, path::PathBuf, sync::Arc};

use rq_core::{
  github::{self, GithubToken},
  package::QuestPackage,
  quest::{CreateSource, Quest, QuestConfig, StateDescriptor, StateEmitter},
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, State};
use tauri_specta::collect_events;
use tauri_specta::Event;

struct TauriEmitter(AppHandle);

#[derive(Serialize, Deserialize, Clone, Type, Event)]
pub struct StateEvent(StateDescriptor);

impl StateEmitter for TauriEmitter {
  fn emit(&self, state: StateDescriptor) -> anyhow::Result<()> {
    Ok(StateEvent(state).emit(&self.0)?)
  }
}

#[inline]
fn fmt_err<T>(r: anyhow::Result<T>) -> Result<T, String> {
  r.map_err(|e| format!("{e:?}"))
}

#[tauri::command]
#[specta::specta]
fn get_github_token() -> GithubToken {
  github::get_github_token()
}

#[tauri::command]
#[specta::specta]
fn init_octocrab(token: String) -> Result<(), String> {
  fmt_err(github::init_octocrab(&token))
}

#[tauri::command]
#[specta::specta]
fn current_dir() -> PathBuf {
  env::current_dir().unwrap()
}

fn manage_quest(quest: Quest, app: &AppHandle) -> Arc<Quest> {
  let quest = Arc::new(quest);
  app.manage(Arc::clone(&quest));

  let quest_ref = Arc::clone(&quest);
  tokio::spawn(async move {
    quest_ref.infer_state_loop().await;
  });

  quest
}

#[tauri::command]
#[specta::specta]
async fn load_quest(
  dir: PathBuf,
  app: AppHandle,
) -> Result<(QuestConfig, StateDescriptor), String> {
  let quest = fmt_err(Quest::load(dir, Box::new(TauriEmitter(app.clone()))).await)?;
  let quest = manage_quest(quest, &app);
  let state = fmt_err(quest.state_descriptor().await)?;
  Ok((quest.config.clone(), state))
}

#[derive(Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value")]
pub enum QuestLocation {
  Remote(String),
  Local(PathBuf),
}

#[tauri::command]
#[specta::specta]
async fn new_quest(
  dir: PathBuf,
  quest_loc: QuestLocation,
  app: AppHandle,
) -> Result<(QuestConfig, StateDescriptor), String> {
  let source = match quest_loc {
    QuestLocation::Remote(remote) => {
      let (user, repo) = remote
        .split_once("/")
        .ok_or_else(|| format!("Invalid quest name: {remote}"))?;
      CreateSource::Remote {
        user: user.to_string(),
        repo: repo.to_string(),
      }
    }
    QuestLocation::Local(local) => {
      let package = fmt_err(QuestPackage::load_from_file(&local))?;
      CreateSource::Package(package)
    }
  };
  let quest = fmt_err(Quest::create(dir, source, Box::new(TauriEmitter(app.clone()))).await)?;
  let quest = manage_quest(quest, &app);
  let state = fmt_err(quest.state_descriptor().await)?;
  Ok((quest.config.clone(), state))
}

#[tauri::command]
#[specta::specta]
async fn file_feature_and_issue(quest: State<'_, Arc<Quest>>, stage: u32) -> Result<(), String> {
  let stage = usize::try_from(stage).unwrap();
  fmt_err(quest.file_feature_and_issue(stage).await)?;
  Ok(())
}

#[tauri::command]
#[specta::specta]
async fn file_solution(quest: State<'_, Arc<Quest>>, stage: u32) -> Result<(), String> {
  let stage = usize::try_from(stage).unwrap();
  fmt_err(quest.file_solution(stage).await)?;
  Ok(())
}

#[tauri::command]
#[specta::specta]
async fn refresh_state(quest: State<'_, Arc<Quest>>) -> Result<(), String> {
  fmt_err(quest.infer_state_update().await)
}

#[tauri::command]
#[specta::specta]
async fn hard_reset(quest: State<'_, Arc<Quest>>, stage: u32) -> Result<(), String> {
  let stage = usize::try_from(stage).unwrap();
  fmt_err(quest.hard_reset(stage).await)?;
  Ok(())
}

pub fn specta_builder() -> tauri_specta::Builder {
  tauri_specta::Builder::<tauri::Wry>::new()
    .commands(tauri_specta::collect_commands![
      get_github_token,
      init_octocrab,
      load_quest,
      current_dir,
      new_quest,
      file_feature_and_issue,
      file_solution,
      refresh_state,
      hard_reset
    ])
    .events(collect_events![StateEvent])
}
