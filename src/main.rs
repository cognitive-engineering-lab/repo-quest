#![allow(unused_variables)]
#![allow(dead_code)]
#![allow(unused_imports)]
mod forgejo;
mod forgejo_hook;
mod git;
mod quest;

use std::{
    collections::{HashMap, hash_map::Entry},
    env, fs,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, anyhow, bail};
use async_lock::Mutex;
use axum::{
    Form, Json, Router, ServiceExt,
    extract::{Path, Query, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use env_logger::Env;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use thiserror::Error;
use tower_http::{cors, normalize_path::NormalizePathLayer};
use tower_layer::Layer as _;
use url::Url;

use crate::{
    forgejo::ForgejoBackend,
    git::GitRepo,
    quest::{definition::*, instance::*},
};

/// The overall state of ReqoQuest. All of the state is loaded into memory at
/// program startup. Unless a user has many quest definitions or very many quest
/// instances, having everything in memory shouldn't be an issue.
///
/// All changes to the state should be written to disk on each change before the
/// global lock (see the note on concurrency in `main`) is released.
#[derive(Clone)]
struct AppState {
    forgejo: ForgejoBackend,
    quest_definitions_dir: PathBuf,
    quest_definitions_file: PathBuf,
    quest_definitions: QuestDefinitionIndex,
    quest_instances_dir: PathBuf,
    quest_instances_file: PathBuf,
    quest_instances: QuestInstanceIndex,
}

#[derive(Debug, Error)]
#[error("Internal server error")]
struct AppError(
    #[source]
    #[from]
    anyhow::Error,
);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("{}\n{:?}", self.0, self.0.source()),
        )
            .into_response()
    }
}

type Result<T> = std::result::Result<T, AppError>;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(not(debug_assertions))]
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    let args: Vec<String> = env::args().collect();

    let given_quest_dir = PathBuf::from(args.get(1).unwrap());
    if !given_quest_dir.is_dir() {
        fs::create_dir_all(&given_quest_dir)
            .with_context(|| format!("Could not create dir {given_quest_dir:?}"))?;
    }
    let quest_dir = given_quest_dir
        .canonicalize()
        .with_context(|| format!("Could not canonicalize quest dir path {given_quest_dir:?}"))?;
    info!("Quest directory {quest_dir:?}.");

    let mut quest_definitions_dir = quest_dir.clone();
    quest_definitions_dir.push("definitions/");
    let mut quest_instances_dir = quest_dir.clone();
    quest_instances_dir.push("instances/");
    let mut quest_definitions_file = quest_definitions_dir.clone();
    quest_definitions_file.push("data.json");
    let mut quest_instances_file = quest_instances_dir.clone();
    quest_instances_file.push("data.json");

    let forgejo = ForgejoBackend::new();
    let quest_definitions = QuestDefinitionIndex::load_or_init(&quest_definitions_file)?;
    let quest_instances = QuestInstanceIndex::load_or_init(&quest_instances_file)?;

    // register to receive webhooks
    forgejo
        .register_webhook(Url::parse("http://localhost:8000/hook").unwrap())
        .await?;

    // A note on concurrency:
    //
    // At the moment requests are completely serialized by using async_lock so that
    // one lock can be taken at the beginning of every request and held across calls
    // to the Forgejo instance. This should suffice because we anticipate the
    // instance to be used by a single user, and so concurrent requests should be
    // rare and (because the Forgejo instance is local to the reqpo-quest process)
    // should be resolved quickly enough that there is little benefit to handling
    // them in parallel.
    let state = Arc::new(Mutex::new(AppState {
        forgejo,
        quest_definitions_dir,
        quest_definitions_file,
        quest_definitions,
        quest_instances_dir,
        quest_instances_file,
        quest_instances,
    }));

    let cors = cors::CorsLayer::new()
        .allow_origin(cors::Any)
        .allow_headers(cors::Any);

    // build our application with a route
    let app = Router::new()
        // POST: Endpoint for Forgejo webhooks.
        .route("/hook", post(forgejo_hook::handler))
        // GET: A map from available quest template ids to names and to
        // descriptions
        //
        // POST: Creates a new quest template from source.
        .route(
            "/quest_definition",
            get(get_quest_definitions).post(add_quest_definition),
        )
        // GET: Gets the list of current quest ids and the names of the
        // templates the quests are based on.
        //
        // POST: Creates a quest and returns the ID of the quest on success.
        .route("/quest", get(get_quests).post(start_quest))
        // GET: Gets the list of chapters for a quest and the corresponding
        // issue IDs and PR IDs, if there are any. The chapter ID is the index
        // in the list and if there are no issue IDs or PRs, the chapter hasn't
        // been started yet.
        .route("/quest/{quest_id}/chapter", get(get_chapters))
        // GET: Gets the current chapter of the given quest along with the
        // corresponding issue ID and PR ID.
        //
        // POST: Sets the quest chapter to the requested chapter, if the
        // requested chapter is next and the current chapter is finished.
        .route(
            "/quest/{quest_id}/chapter/current",
            get(get_current_chapter).post(post_set_current_chapter),
        )
        // GET: Gets the PR ID for the reference solution for the given chapter
        // if there is one, or whether there is one available (if there isn't).
        //
        // POST: Creates the reference solution for the given chapter and
        // returns the PR ID, if the given chapter is current.
        .route(
            "/quest/{quest_id}/chapter/{chapter}/reference_solution",
            get(get_reference_solution).post(create_reference_solution),
        )
        .with_state(state)
        .layer(cors);
    let app = NormalizePathLayer::trim_trailing_slash().layer(app);

    // run our app with hyper, listening globally on port 8000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    axum::serve(listener, ServiceExt::<Request>::into_make_service(app))
        .await
        .unwrap();

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestDefinitionInfo {
    name: String,
    description: String,
}

async fn get_quest_definitions(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<Json<HashMap<String, QuestDefinitionInfo>>> {
    let state = state.lock().await;

    let quest_definitions = &state.quest_definitions;
    Ok(Json(
        quest_definitions
            .quest_definitions
            .iter()
            .map(|(k, t)| {
                (
                    k.clone(),
                    QuestDefinitionInfo {
                        name: t.name.clone(),
                        description: t.description.clone(),
                    },
                )
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum QuestSource {
    GitHub { slug: String },
    Bundle { path: Url },
    // TODO: Upload { },
}

async fn add_quest_definition(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(source): Json<QuestSource>,
) -> Result<Json<String>> {
    todo!()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestInfo {
    name: String,
    repo_url: Url,
    task_info: Option<Task>,
}

async fn get_quests(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Result<Json<HashMap<i64, QuestInfo>>> {
    let state = state.lock().await;
    let quests = &state.quest_instances.quests;
    let quest_defns = &state.quest_definitions;
    let mut quests_info = HashMap::new();
    for (&id, quest) in quests {
        let quest_defn = quest_defns.get(&quest.definition_id)?;
        let quest_info = QuestInfo {
            name: quest_defn.name.clone(),
            repo_url: quest.repo_url.clone(),
            task_info: quest.tasks.last().cloned(),
        };
        quests_info.insert(id, quest_info);
    }
    Ok(Json(quests_info))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartQuestQuery {
    username: String,
    quest_template_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartQuestResponse {
    /// RepoQuest ID for the quest
    id: i64,
    /// HTML location for the next step of the quest
    url: Url,
}

async fn start_quest(
    State(state): State<Arc<Mutex<AppState>>>,
    Json(query): Json<StartQuestQuery>,
) -> Result<Json<StartQuestResponse>> {
    let mut state = state.lock().await;
    let defns = &state.quest_definitions;

    let template = defns.get(&query.quest_template_id)?.clone();

    // create local quest repo
    let prefix = template.generated_repo_name.clone() + ".";
    let local_repo_dir = tempfile::Builder::new()
        .prefix(&prefix)
        .tempdir_in(&state.quest_instances_dir)
        .with_context(|| {
            format!(
                "Could not initialize tempdir in {:?}",
                &state.quest_instances_dir
            )
        })?
        .keep();
    let local_repo = GitRepo::init(local_repo_dir)?;
    debug!("Initialized local git repo {local_repo:?}");

    // set the quest definition repo as a remote
    let mut defn_repo_path = state.quest_definitions_dir.clone();
    defn_repo_path.push(template.repo.as_path());
    local_repo.add_remote(
        "quest",
        defn_repo_path.to_str().ok_or(anyhow!(
            "Could not convert template repo path {defn_repo_path:?} to string."
        ))?,
    )?;

    // fetch and initialize
    local_repo.fetch("quest")?;
    local_repo.restore_from("quest/main")?;
    local_repo.commit("Initial commit")?;

    // generate forgejo repo from template
    let forgejo_repo = state
        .forgejo
        .create_quest_repo(&query.username, &template)
        .await?;

    // set repo upstream to forgejo
    let repo_url = forgejo_repo
        .html_url
        .ok_or(anyhow!("Forgejo repo has no url."))?;
    let mut remote_url = repo_url.clone();
    remote_url
        .set_username("repoquest")
        .map_err(|_| anyhow!(""))?;
    remote_url
        .set_password(Some("repoquest"))
        .map_err(|_| anyhow!(""))?;

    local_repo.add_remote("origin", remote_url.as_str())?;

    // push starter code to upstream main
    local_repo.push("origin", "main", "main")?;

    // define quest instance
    let quest = Quest {
        definition_id: query.quest_template_id,
        local_repo,
        repo_url: repo_url.clone(),
        owner: query.username,
        repo: forgejo_repo.name.ok_or(anyhow!("No repo name"))?,
        tasks: Vec::new(),
    };
    let id = forgejo_repo.id.ok_or(anyhow!("No repo id."))?;

    // add quest to index
    let quest = match state.quest_instances.quests.entry(id) {
        Entry::Occupied(_) => Err(anyhow!("Quest with given id already exists.")),
        Entry::Vacant(vacant_entry) => Ok(vacant_entry.insert(quest)),
    }?;

    // save index
    state.quest_instances.store(&state.quest_instances_file)?;

    let mut url = repo_url;
    // start first chapter, if there are chapters
    if !template.tasks.is_empty() {
        // Forgejo can't accept PRs right away... this works around that.
        // TODO: don't return from repo creation until the repo is fully created.
        std::thread::sleep(Duration::from_secs(1));
        let task = set_current_chapter(&mut state, id, 0).await?;
        url = task.issue_url;
    };

    Ok(Json(StartQuestResponse { id, url }))
}

async fn create_reference_solution(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<String>,
    Path(chapter_id): Path<String>,
) -> Result<()> {
    todo!()
}

async fn get_reference_solution(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
    Path(chapter_id): Path<String>,
) -> Result<Json<i64>> {
    todo!()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NextChapterBody {
    chapter_number: usize,
}

async fn get_chapters(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
) -> Result<Json<Vec<Option<Task>>>> {
    let state = state.lock().await;

    let quests = &state.quest_instances;

    let quest = quests
        .quests
        .get(&quest_id)
        .ok_or(anyhow!("No quest with id {quest_id}."))?;

    let mut tasks = quest
        .tasks
        .iter()
        .cloned()
        .map(Some)
        .collect::<Vec<Option<Task>>>();

    let quest_definitions = &state.quest_definitions;
    let task_templates_len = quest_definitions
        .quest_definitions
        // TODO: how to fix this while keeping the newtype?
        .get(&quest.definition_id)
        .ok_or(anyhow!(
            "No quest template id {}, for quest {quest_id}.",
            &quest.definition_id
        ))?
        .tasks
        .len();

    tasks.extend((1..task_templates_len - tasks.len()).map(|_| None));

    Ok(Json(tasks))
}

async fn get_current_chapter(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
) -> Result<Json<Option<usize>>> {
    let state = state.lock().await;
    let quests = &state.quest_instances;

    let quest = quests
        .quests
        .get(&quest_id)
        .ok_or(anyhow!("No quest with id {quest_id}."))?;

    let cur_task_id = quest.tasks.len().checked_sub(1);
    Ok(Json(cur_task_id))
}

/// Sets the current chapter to the requested chapter, if the requested chapter
/// is the next chapter.
#[axum::debug_handler]
async fn post_set_current_chapter(
    State(state): State<Arc<Mutex<AppState>>>,
    Path(quest_id): Path<i64>,
    Json(body): Json<NextChapterBody>,
) -> Result<Json<Task>> {
    info!("Set current chapter for {quest_id} to {body:?}.");

    let mut state = state.lock().await;
    Ok(Json(
        set_current_chapter(&mut state, quest_id, body.chapter_number).await?,
    ))
}

async fn set_current_chapter(
    state: &mut AppState,
    quest_id: i64,
    chapter_number: usize,
) -> Result<Task> {
    let AppState {
        ref forgejo,
        ref quest_instances_file,
        quest_instances: ref mut quests,
        quest_definitions: ref defns,
        ..
    } = *state;

    let quest = quests
        .quests
        .get(&quest_id)
        .ok_or(anyhow!("No quest with id {quest_id}."))?;

    let quest_definition = defns
        .quest_definitions
        .get(&quest.definition_id)
        .ok_or(anyhow!("No quest definition with {}", &quest.definition_id))?;

    let next_task_pos = quest.tasks.len();

    if next_task_pos != chapter_number {
        return Err(AppError(anyhow!(
            "Requested chapter {chapter_number} is not the next chapter."
        )));
    }

    // Confirm that previous task is complete.
    if let Some(task) = quest.tasks.last()
        && !forgejo
            .is_pull_request_merged(&quest.owner, &quest.repo, task.pr.0)
            .await?
    {
        return Err(anyhow!(
            "Pull request {}/{}#{} not merged.",
            &quest.owner,
            &quest.repo,
            task.pr.0
        )
        .into());
    }

    let task_template = quest_definition.tasks.get(chapter_number).ok_or(anyhow!(
        "Missing definition of task for chapter {chapter_number}."
    ))?;

    let local_repo = &quest.local_repo;
    let scaffolding = &task_template.scaffolding.0;
    local_repo.create_branch("main", scaffolding)?;
    local_repo.switch_branch(scaffolding)?;
    let remote_branch = format!("remotes/quest/{}", scaffolding);
    local_repo.restore_from(&remote_branch)?;
    local_repo.commit("task commit message")?;
    local_repo.push("origin", scaffolding, scaffolding)?;
    local_repo.switch_branch("main")?;

    let task = forgejo
        .create_task(&quest.owner, &quest.repo, task_template)
        .await?;

    let quest = quests
        .quests
        .get_mut(&quest_id)
        .ok_or(anyhow!("No quest with id {quest_id}."))?;

    quest.tasks.push(task.clone());

    quests.store(quest_instances_file)?;

    Ok(task)
}
