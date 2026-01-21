use axum::{
    extract::{Path, Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::path::Path as StdPath;
use sqlx::{PgPool, FromRow};

use crate::{auth::{AuthUser, PermissiveAuthUser}, AppState};


#[derive(Serialize, FromRow)]
pub struct Repo {
    name: String,
    public: bool,
}

#[derive(Deserialize)]
pub struct CreateRepoRequest {
    name: String,
    public: Option<bool>,
}

#[derive(Serialize)] pub struct Branch { name: String }
#[derive(Serialize)] pub struct Commit { id: String, message: String, author: String, date: String }
#[derive(Serialize)] pub struct TreeEntry { name: String, entry_type: String }

pub async fn check_repo_read_access(
    repo_name: &str,
    pool: &PgPool,
    user: &PermissiveAuthUser,
) -> Result<(), Response> {
    let repo_info: Option<(i32, bool)> = sqlx::query_as("SELECT user_id, public FROM repositories WHERE name = $1")
        .bind(repo_name)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query repository info: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to check repository permissions").into_response()
        })?;

    match repo_info {
        Some((owner_id, is_public)) => {
            if is_public {
                return Ok(());
            }
            if let Some(u) = &user.0 {
                if u.id == owner_id {
                    return Ok(());
                }
            }
            Err((StatusCode::FORBIDDEN, "You do not have permission to access this repository").into_response())
        }
        None => Err((StatusCode::NOT_FOUND, "Repository not found").into_response()),
    }
}

pub async fn list_repos_handler(State(state): State<AppState>) -> Response {
    match sqlx::query_as::<_, Repo>("SELECT name, public FROM repositories WHERE public = true")
        .fetch_all(&state.pool)
        .await
    {
        Ok(repos) => Json(repos).into_response(),
        Err(e) => {
            tracing::error!("Failed to list public repositories: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list repositories").into_response()
        }
    }
}

pub async fn list_branches_handler(
    Path(name): Path<String>,
    State(state): State<AppState>,
    user: PermissiveAuthUser,
) -> Response {
    let repo_name = name.strip_suffix(".git").unwrap_or(&name);
    if let Err(response) = check_repo_read_access(repo_name, &state.pool, &user).await {
        return response;
    }
    
    let repo_path = StdPath::new("./repos").join(format!("{}.git", name));
    let repo = match git2::Repository::open(repo_path) {
        Ok(repo) => repo,
        Err(_) => return (StatusCode::NOT_FOUND, "Repository not found on filesystem").into_response(),
    };

    let mut branch_list = Vec::new();
    if let Ok(branches) = repo.branches(None) {
        for branch in branches.flatten() {
            if let Ok(Some(branch_name)) = branch.0.name() {
                branch_list.push(Branch { name: branch_name.to_string() });
            }
        }
    }

    Json(branch_list).into_response()
}

#[axum::debug_handler]
pub async fn create_repo_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateRepoRequest>,
) -> Response {
    let name = &payload.name;
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return (StatusCode::BAD_REQUEST, "Invalid repository name").into_response();
    }

    let repo_name_git = format!("{}.git", name);
    let path = StdPath::new("./repos").join(&repo_name_git);

    if path.exists() {
        return (StatusCode::CONFLICT, "Repository already exists on filesystem").into_response();
    }

    match git2::Repository::init_bare(&path) {
        Ok(repo) => {
            if let Ok(mut config) = repo.config() {
                let _ = config.set_bool("http.receivepack", true);
            }

            let repo_name_db = name.to_string();
            let is_public = payload.public.unwrap_or(false);

            let result = sqlx::query("INSERT INTO repositories (name, user_id, public) VALUES ($1, $2, $3)")
                .bind(&repo_name_db)
                .bind(user.0.id)
                .bind(is_public)
                .execute(&state.pool)
                .await;

            match result {
                Ok(_) => {
                    tracing::info!("Created new repository: {}", repo_name_git);
                    (StatusCode::CREATED, Json(Repo { name: repo_name_db, public: is_public })).into_response()
                }
                Err(e) => {
                    tracing::error!("Failed to record repository ownership: {}", e);
                    let _ = std::fs::remove_dir_all(&path);
                    (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create repository").into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to create repository filesystem: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create repository").into_response()
        }
    }
}

#[axum::debug_handler]
pub async fn delete_repo_handler(
    Path(name): Path<String>,
    State(state): State<AppState>,
    user: AuthUser,
) -> Response {
    let repo_name = name.strip_suffix(".git").unwrap_or(&name).to_string();

    let owner_result: Result<Option<(i32,)>, _> = sqlx::query_as("SELECT user_id FROM repositories WHERE name = $1")
        .bind(&repo_name)
        .fetch_optional(&state.pool)
        .await;

    match owner_result {
        Ok(Some((owner_id,))) => {
            if owner_id != user.0.id {
                return (StatusCode::FORBIDDEN, "You do not have permission to delete this repository").into_response();
            }
        }
        Ok(None) => return (StatusCode::NOT_FOUND, "Repository not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to query repository ownership: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to check ownership").into_response();
        }
    }

    if let Err(e) = sqlx::query("DELETE FROM repositories WHERE name = $1").bind(&repo_name).execute(&state.pool).await {
        tracing::error!("Failed to delete repository record: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete repository").into_response();
    }

    let repo_name_git = format!("{}.git", repo_name);
    let path = StdPath::new("./repos").join(&repo_name_git);
    if path.exists() {
        if let Err(e) = std::fs::remove_dir_all(&path) {
            tracing::error!("Failed to delete repository filesystem: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fully delete repository").into_response();
        }
    }

    tracing::info!("Deleted repository: {}", repo_name);
    (StatusCode::OK, format!("Repository {} deleted", repo_name)).into_response()
}

#[axum::debug_handler]
pub async fn list_files_root_handler(Path((name, branch)): Path<(String, String)>, State(state): State<AppState>, user: PermissiveAuthUser) -> Response {
    list_files_implementation(name, branch, None, user, state).await
}

#[axum::debug_handler]
pub async fn list_files_subdirectory_handler(Path((name, branch, path)): Path<(String, String, String)>, State(state): State<AppState>, user: PermissiveAuthUser) -> Response {
    list_files_implementation(name, branch, Some(path), user, state).await
}

async fn list_files_implementation(name: String, branch: String, path: Option<String>, user: PermissiveAuthUser, state: AppState) -> Response {
    let repo_name = name.strip_suffix(".git").unwrap_or(&name);
    if let Err(response) = check_repo_read_access(repo_name, &state.pool, &user).await {
        return response;
    }

    let repo_path = StdPath::new("./repos").join(format!("{}.git", name));
    let repo = match git2::Repository::open(&repo_path) {
        Ok(repo) => repo,
        Err(_) => return (StatusCode::NOT_FOUND, "Repository not found").into_response(),
    };

    let branch = match repo.find_branch(&branch, git2::BranchType::Local) {
        Ok(branch) => branch,
        Err(_) => return (StatusCode::NOT_FOUND, "Branch not found").into_response(),
    };

    let commit = match branch.get().peel_to_commit() {
        Ok(commit) => commit,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get commit for branch").into_response(),
    };

    let tree = match commit.tree() {
        Ok(tree) => tree,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get tree for commit").into_response(),
    };

    let target_tree = if let Some(path) = path {
        let path = path.strip_prefix("/").unwrap_or(&path);
        let path = path.strip_suffix("/").unwrap_or(&path);
        if path.is_empty() {
            tree
        } else {
            match tree.get_path(StdPath::new(path)) {
                Ok(entry) => match entry.to_object(&repo) {
                    Ok(object) => match object.into_tree() {
                        Ok(tree) => tree,
                        Err(_) => return (StatusCode::NOT_FOUND, "Path is not a directory").into_response(),
                    },
                    Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to resolve path object").into_response(),
                },
                Err(_) => return (StatusCode::NOT_FOUND, "Path not found in repository").into_response(),
            }
        }
    } else {
        tree
    };

    let mut files = Vec::new();
    for entry in target_tree.iter() {
        let entry_type = match entry.kind() {
            Some(git2::ObjectType::Blob) => "blob",
            Some(git2::ObjectType::Tree) => "tree",
            _ => "unknown",
        };
        if let Some(name) = entry.name() {
            files.push(TreeEntry {
                name: name.to_string(),
                entry_type: entry_type.to_string(),
            });
        }
    }

    Json(files).into_response()
}

#[axum::debug_handler]
pub async fn commit_history_handler(Path((name, branch_name)): Path<(String, String)>, State(state): State<AppState>, user: PermissiveAuthUser) -> Response {
    let repo_name = name.strip_suffix(".git").unwrap_or(&name);
    if let Err(response) = check_repo_read_access(repo_name, &state.pool, &user).await {
        return response;
    }

    let repo_path = StdPath::new("./repos").join(format!("{}.git", name));
    let repo = match git2::Repository::open(repo_path) {
        Ok(repo) => repo,
        Err(_) => return (StatusCode::NOT_FOUND, "Repository not found").into_response(),
    };

    let branch = match repo.find_branch(&branch_name, git2::BranchType::Local) {
        Ok(branch) => branch,
        Err(_) => {
            match repo.find_branch(&format!("origin/{}", branch_name), git2::BranchType::Remote) {
                Ok(branch) => branch,
                Err(_) => return (StatusCode::NOT_FOUND, "Branch not found").into_response(),
            }
        }
    };

    let commit = match branch.get().peel_to_commit() {
        Ok(commit) => commit,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get commit for branch").into_response(),
    };

    let mut revwalk = match repo.revwalk() {
        Ok(walk) => walk,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create revision walker").into_response(),
    };
    
    if let Err(_) = revwalk.push(commit.id()) {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to push commit to revision walker").into_response();
    }

    let mut commits = Vec::new();
    for oid in revwalk {
        if let Ok(oid) = oid {
            if let Ok(commit) = repo.find_commit(oid) {
                let author = commit.author();
                let author_name = author.name().unwrap_or("Unknown");
                let date = chrono::DateTime::from_timestamp(commit.time().seconds(), 0).unwrap().to_rfc2822();

                commits.push(Commit {
                    id: oid.to_string(),
                    message: commit.message().unwrap_or("").to_string(),
                    author: author_name.to_string(),
                    date,
                });
            }
        }
    }

    Json(commits).into_response()
}
