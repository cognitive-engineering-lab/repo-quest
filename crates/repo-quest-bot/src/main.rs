use std::{
    fs,
    net::{IpAddr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};

use anyhow::Context;
use axum::{ServiceExt, extract::Request};
use clap::Parser;
use env_logger::Env;
use log::info;
use url::Url;

use repo_quest_core::quest::{definition::QuestDefinitionIndex, instance::QuestInstanceIndex};

use self::{
    bot::{AppState, Errors},
    forgejo::ForgejoBackend,
};

mod bot;
mod forgejo;
mod forgejo_hook;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Directory where `RepoQuest` bot state is stored
    #[arg(long)]
    state_dir: PathBuf,
    /// Base URL for the Forgejo instance
    #[arg(long, default_value = "http://forgejo:3000")]
    forgejo_url: Url,
    #[arg(long)]
    public_url: Url,
    /// Base URL for Forgejo to access this bot's hook
    #[arg(long, default_value = "http://repoquest:8000")]
    hook_url: Url,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Args {
        state_dir,
        forgejo_url,
        public_url,
        hook_url,
    } = Args::parse();

    #[cfg(not(debug_assertions))]
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();
    #[cfg(debug_assertions)]
    env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

    let given_quest_dir = state_dir;
    if !given_quest_dir.is_dir() {
        fs::create_dir_all(&given_quest_dir)
            .with_context(|| format!("Could not create dir `{}`", given_quest_dir.display()))?;
    }
    let quest_dir = given_quest_dir.canonicalize().with_context(|| {
        format!(
            "Could not canonicalize quest dir path `{}`",
            given_quest_dir.display()
        )
    })?;
    info!("Quest directory `{}`.", quest_dir.display());

    let forgejo = {
        // This username/password should match the ones declared in run-forgejo.sh.
        let auth = forgejo_api::Auth::Password {
            username: "repoquest",
            password: "repoquest",
            mfa: None,
        };
        ForgejoBackend::new(auth, forgejo_url.clone())
    };
    let quest_definitions = QuestDefinitionIndex::load_or_init(quest_dir.join("definitions"))?;
    let quest_instances = QuestInstanceIndex::load_or_init(quest_dir.join("instances"))?;

    // register to receive webhooks
    forgejo
        .register_webhook(
            hook_url
                .join("hook")
                .context("Could not append to hook URL.")?,
        )
        .await?;

    let path = quest_dir.join("errors.json");
    info!("Reading stored server errors from `{}`.", path.display());
    let errors = Errors::load(path)?;

    let state = AppState {
        forgejo,
        forgejo_url,
        public_url,
        quest_definitions,
        quest_instances,
        errors,
    };

    let app = bot::new(state);
    // run our app with hyper, listening globally on port 8000
    let addr = &SocketAddr::new(IpAddr::from(Ipv6Addr::UNSPECIFIED), 8000);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, ServiceExt::<Request>::into_make_service(app)).await?;

    Ok(())
}
