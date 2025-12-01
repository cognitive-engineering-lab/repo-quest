use std::sync::Arc;

use anyhow::anyhow;
use async_lock::Mutex;
use axum::{Json, extract::State};
use log::debug;
use serde::Deserialize;

use crate::{AppState, set_current_chapter};

type Result<T> = std::result::Result<T, crate::AppError>;

#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryHookData {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestHookData {
    pub number: i64,
    pub merged: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoHookBody {
    pub action: String,
    pub pull_request: Option<PullRequestHookData>,
    pub repository: Option<RepositoryHookData>,
}

pub async fn handler(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(body): Json<ForgejoHookBody>,
) -> Result<()> {
    debug!("Hook call:\n\n{:?}\n", body);
    if body.action == "closed" && body.pull_request.is_some_and(|pr| pr.merged) {
        let mut state = state.lock().await;
        let quest_id = body
            .repository
            .ok_or(anyhow!("No repository info provided with hook body."))?
            .id;

        let quest = state.quest_instances.metadata(quest_id)?;
        let quest_defn = state.quest_definitions.definition(quest.definition_id)?;

        let next_chapter_number = quest.tasks.len();
        if next_chapter_number < quest_defn.metadata.tasks.len() {
            set_current_chapter(&mut state, quest_id, next_chapter_number).await?;
        }
    }
    Ok(())
}
