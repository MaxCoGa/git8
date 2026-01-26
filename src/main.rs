use axum::{
    routing::{any, get, post, delete, Router},
};
use std::net::SocketAddr;
use std::path::Path as StdPath;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use sqlx::PgPool;

mod git_backend;
mod git_api;
mod db;
mod auth;
mod issues;

#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "app=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    if !StdPath::new("./repos").exists() {
        if let Err(e) = std::fs::create_dir("./repos") {
            tracing::error!("Failed to create repos directory: {}", e);
            return;
        }
    }

    let pool = match db::create_pool().await {
        Ok(pool) => pool,
        Err(e) => {
            tracing::error!("Failed to create database pool: {}", e);
            return;
        }
    };

    if let Err(e) = db::run_migrations(&pool).await {
        tracing::error!("Failed to run database migrations: {}", e);
        return;
    }
    
    let state = AppState {
        pool,
    };

    let app = Router::new()
        .route("/register", post(auth::register_handler))
        .route("/login", post(auth::login_handler))
        .route("/repos", get(git_api::list_repos_handler))
        .route("/repos", post(git_api::create_repo_handler))
        .route("/repos/:name", delete(git_api::delete_repo_handler))
        .route("/repos/:name/branches", get(git_api::list_branches_handler))
        .route("/repos/:name/tree/:branch", get(git_api::list_files_root_handler))
        .route("/repos/:name/tree/:branch/", get(git_api::list_files_root_handler))
        .route("/repos/:name/tree/:branch/*path", get(git_api::list_files_subdirectory_handler))
        .route("/repos/:name/commits/:branch", get(git_api::commit_history_handler))
        .route("/repos/:name/issues", post(issues::create_issue))
        .route("/repos/:name/issues", get(issues::list_issues))
        .route("/repos/:name/issues/:issue_id", get(issues::get_issue))
        .route("/repos/:name/issues/:issue_id/comments", post(issues::create_comment))
        .route("/repos/:name/issues/:issue_id/comments", get(issues::list_comments))
        .route("/repos/:name/labels", post(issues::create_label))
        .route("/repos/:name/labels", get(issues::list_labels))
        .route("/repos/:name/issues/:issue_id/labels/:label_name", post(issues::add_label_to_issue))
        .route("/repos/:name/issues/:issue_id/labels/:label_name", delete(issues::remove_label_from_issue))
        .route("/repos/:name/issues/:issue_id/assignees/:assignee_username", post(issues::add_assignee_to_issue))
        .route("/repos/:name/issues/:issue_id/assignees/:assignee_username", delete(issues::remove_assignee_from_issue))
        .fallback(any(git_backend::handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service()).await.unwrap();
}
