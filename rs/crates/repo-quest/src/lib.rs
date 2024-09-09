// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, path::PathBuf, sync::Arc};

use self::quest::{Quest, QuestConfig};
use github::GithubToken;
use quest::StateEvent;
use tauri::{AppHandle, Manager, State};
use tauri_specta::collect_events;

mod git;
mod github;
mod quest;
mod stage;
mod utils;

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

async fn load_quest_core(
  dir: PathBuf,
  config: &QuestConfig,
  app: AppHandle,
) -> Result<Arc<Quest>, String> {
  let quest = fmt_err(Quest::load(dir, config.clone(), Some(app.clone())).await)?;

  let quest = Arc::new(quest);
  app.manage(Arc::clone(&quest));

  let quest_ref = Arc::clone(&quest);
  tokio::spawn(async move {
    quest_ref.infer_state_loop().await;
  });

  Ok(quest)
}

#[tauri::command]
#[specta::specta]
async fn load_quest(dir: PathBuf, app: AppHandle) -> Result<QuestConfig, String> {
  let config = fmt_err(QuestConfig::load(&dir))?;
  load_quest_core(dir, &config, app).await?;
  Ok(config)
}

#[tauri::command]
#[specta::specta]
async fn new_quest(dir: PathBuf, quest: String, app: AppHandle) -> Result<QuestConfig, String> {
  let config = fmt_err(quest::load_config_from_remote("cognitive-engineering-lab", &quest).await)?;
  let quest = load_quest_core(dir.join(quest), &config, app).await?;
  fmt_err(quest.create_repo().await)?;
  Ok(config)
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
