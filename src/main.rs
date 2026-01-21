use axum::{
    routing::{any, get, post, delete},
    Router,
    Extension,
    middleware,
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

    let protected_routes = Router::new()
        .route("/repos/:name", post(git_api::create_repo_handler))
        .route("/repos/:name", delete(git_api::delete_repo_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::auth));

    let app = Router::new()
        .route("/register", post(auth::register_handler))
        .route("/login", post(auth::login_handler))
        .route("/repos", get(git_api::list_repos_handler))
        .route("/repos/:name/branches", get(git_api::list_branches_handler))
        .route("/repos/:name/tree/:branch", get(git_api::list_files_root_handler))
        .route("/repos/:name/tree/:branch/", get(git_api::list_files_root_handler))
        .route("/repos/:name/tree/:branch/*path", get(git_api::list_files_subdirectory_handler))
        .route("/repos/:name/commits/:branch", get(git_api::commit_history_handler))
        .merge(protected_routes)
        .fallback(any(git_backend::handler))
        .layer(TraceLayer::new_for_http())
        .layer(Extension(state));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::debug!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
