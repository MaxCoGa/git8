use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use git2::{self, DiffOptions};
use tokio::task;

use crate::auth::{AuthUser, PermissiveAuthUser};
use crate::AppState;

pub mod comments;
pub mod reviews;

#[derive(Serialize, FromRow, Debug)]
pub struct PullRequest {
    pub id: i32,
    pub repo_id: i32,
    pub title: String,
    pub body: Option<String>,
    pub base_branch: String,
    pub head_branch: String,
    pub author_id: i32,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct NewPullRequest {
    pub title: String,
    pub body: Option<String>,
    pub base_branch: String,
    pub head_branch: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum PullRequestStatus {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "merged")]
    Merged,
}

impl std::fmt::Display for PullRequestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PullRequestStatus::Open => write!(f, "open"),
            PullRequestStatus::Closed => write!(f, "closed"),
            PullRequestStatus::Merged => write!(f, "merged"),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct UpdatePullRequest {
    pub status: Option<PullRequestStatus>,
    pub title: Option<String>,
    pub body: Option<String>,
}


#[axum::debug_handler]
pub async fn create_pull_request(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(repo_name): Path<String>,
    Json(new_pull_request): Json<NewPullRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start transaction: {}", e),
        )
    })?;

    let repo_info: Option<(i32, String)> = sqlx::query_as(
        r#"SELECT r.id, r.name FROM repositories r WHERE r.name = $1 AND (r.public OR r.user_id = $2)"#,
    )
    .bind(&repo_name)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get repo: {}", e)))?;

    let (repo_id, _repo_name_from_db) = match repo_info {
        Some((id, name)) => (id, name),
        None => return Err((StatusCode::FORBIDDEN, "Repository not found or you don't have permission to create a pull request here.".to_string())),
    };

    let pull_request = sqlx::query_as::<_, PullRequest>(
        r#"
        INSERT INTO pull_requests (repo_id, title, body, base_branch, head_branch, author_id)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, repo_id, title, body, base_branch, head_branch, author_id, status, created_at, updated_at
        "#
    )
    .bind(repo_id)
    .bind(&new_pull_request.title)
    .bind(&new_pull_request.body)
    .bind(&new_pull_request.base_branch)
    .bind(&new_pull_request.head_branch)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create pull request: {}", e)))?;

    tx.commit().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to commit transaction: {}", e)))?;

    Ok((StatusCode::CREATED, Json(pull_request)))
}

#[axum::debug_handler]
pub async fn list_pull_requests(
    State(state): State<AppState>,
    PermissiveAuthUser(user): PermissiveAuthUser,
    Path(repo_name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = user.map(|u| u.id);
    let repo_id_option: Option<i32> = sqlx::query_scalar(
        r#"SELECT id FROM repositories WHERE name = $1 AND (public OR user_id = $2)"#
    )
    .bind(repo_name)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get repo: {}", e)))?;

    let repo_id = match repo_id_option {
        Some(id) => id,
        None => return Err((StatusCode::FORBIDDEN, "Repository not found or you don't have permission to view pull requests here.".to_string())),
    };

    let pull_requests = sqlx::query_as::<_, PullRequest>(
        r#"SELECT id, repo_id, title, body, base_branch, head_branch, author_id, status, created_at, updated_at FROM pull_requests WHERE repo_id = $1"#
    )
    .bind(repo_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch pull requests: {}", e)))?;

    Ok(Json(pull_requests))
}

#[axum::debug_handler]
pub async fn get_pull_request(
    State(state): State<AppState>,
    PermissiveAuthUser(user): PermissiveAuthUser,
    Path((repo_name, pull_id)): Path<(String, i32)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = user.map(|u| u.id);
    let repo_id_option: Option<i32> = sqlx::query_scalar(
        r#"SELECT id FROM repositories WHERE name = $1 AND (public OR user_id = $2)"#
    )
    .bind(repo_name)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get repo: {}", e)))?;

    let repo_id = match repo_id_option {
        Some(id) => id,
        None => return Err((StatusCode::FORBIDDEN, "Repository not found or you don't have permission to view this pull request.".to_string())),
    };

    let pull_request = sqlx::query_as::<_, PullRequest>(
        r#"SELECT id, repo_id, title, body, base_branch, head_branch, author_id, status, created_at, updated_at FROM pull_requests WHERE repo_id = $1 AND id = $2"#
    )
    .bind(repo_id)
    .bind(pull_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch pull request: {}", e)))?;

    match pull_request {
        Some(pr) => Ok(Json(pr)),
        None => Err((StatusCode::NOT_FOUND, "Pull request not found.".to_string())),
    }
}

#[axum::debug_handler]
pub async fn update_pull_request(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((repo_name, pull_id)): Path<(String, i32)>,
    Json(update_payload): Json<UpdatePullRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start transaction: {}", e),
        )
    })?;

    let repo_info: Option<(i32, String)> = sqlx::query_as(
        r#"SELECT r.id, r.name FROM repositories r WHERE r.name = $1 AND (r.public OR r.user_id = $2)"#,
    )
    .bind(&repo_name)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get repo: {}", e)))?;

    let (repo_id, repo_name_from_db) = match repo_info {
        Some((id, name)) => (id, name),
        None => return Err((StatusCode::FORBIDDEN, "Repository not found or you don't have permission to update pull requests here.".to_string())),
    };

    let current_pr = sqlx::query_as::<_, PullRequest>(
        "SELECT * FROM pull_requests WHERE id = $1 AND repo_id = $2"
    )
    .bind(pull_id)
    .bind(repo_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch pull request: {}", e)))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Pull request not found.".to_string()))?;

    let new_status = update_payload.status.map(|s| s.to_string()).unwrap_or_else(|| current_pr.status.clone());

    if update_payload.status == Some(PullRequestStatus::Merged) && current_pr.status != "merged" {
        if let Err(e) = perform_git_merge(&repo_name_from_db, &current_pr.base_branch, &current_pr.head_branch, &user.username).await {
            return Err(e);
        }
    }

    let new_title = update_payload.title.unwrap_or(current_pr.title);
    let new_body = update_payload.body.or(current_pr.body);

    let updated_pr = sqlx::query_as::<_, PullRequest>(
        r#"
        UPDATE pull_requests
        SET status = $1, title = $2, body = $3, updated_at = now()
        WHERE id = $4 AND repo_id = $5
        RETURNING *
        "#,
    )
    .bind(new_status)
    .bind(new_title)
    .bind(new_body)
    .bind(pull_id)
    .bind(repo_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Failed to update pull request: {}", e),
    ))?;

    tx.commit().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to commit transaction: {}", e)))?;

    Ok(Json(updated_pr))
}


async fn perform_git_merge(repo_name: &str, base_branch: &str, head_branch: &str, username: &str) -> Result<(), (StatusCode, String)> {
    let repo_path = format!("./repos/{}.git", repo_name);
    let repo = git2::Repository::open(repo_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to open repository: {}", e)))?;

    let base_ref = format!("refs/heads/{}", base_branch);
    let head_ref = format!("refs/heads/{}", head_branch);

    let base_commit = repo.find_reference(&base_ref).and_then(|r| r.peel_to_commit()).map_err(|e| (StatusCode::BAD_REQUEST, format!("Base branch not found: {}", e)))?;
    let head_commit = repo.find_reference(&head_ref).and_then(|r| r.peel_to_commit()).map_err(|e| (StatusCode::BAD_REQUEST, format!("Head branch not found: {}", e)))?;

    let mut index = repo.merge_commits(&base_commit, &head_commit, None).map_err(|e| (StatusCode::CONFLICT, format!("Merge conflict: {}", e)))?;

    if index.has_conflicts() {
        return Err((StatusCode::CONFLICT, "Merge has conflicts. Please resolve them manually.".to_string()));
    }

    let oid = index.write_tree_to(&repo).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write merge tree: {}", e)))?;
    let tree = repo.find_tree(oid).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to find merge tree: {}", e)))?;

    let signature = git2::Signature::now(username, "user@example.com").map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create signature: {}", e)))?;
    let message = format!("Merge pull request from {} into {}", head_branch, base_branch);

    let merge_commit_oid = repo.commit(
        Some(&base_ref),
        &signature,
        &signature,
        &message,
        &tree,
        &[&base_commit, &head_commit],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create merge commit: {}", e)))?;

    let mut base_branch_ref = repo.find_reference(&base_ref).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to find base branch reference: {}", e)))?;
    base_branch_ref.set_target(merge_commit_oid, "Fast-forward merge").map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update base branch: {}", e)))?;

    Ok(())
}

#[axum::debug_handler]
pub async fn get_pull_request_diff(
    State(state): State<AppState>,
    PermissiveAuthUser(user): PermissiveAuthUser,
    Path((repo_name, pull_id)): Path<(String, i32)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = user.map(|u| u.id);

    let repo_id_option: Option<i32> = sqlx::query_scalar(
        r#"SELECT id FROM repositories WHERE name = $1 AND (public OR user_id = $2)"#
    )
    .bind(&repo_name)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get repo: {}", e)))?;

    let repo_id = match repo_id_option {
        Some(id) => id,
        None => return Err((StatusCode::FORBIDDEN, "Repository not found or you don't have permission.".to_string())),
    };

    let pr = sqlx::query_as::<_, PullRequest>(
        "SELECT * FROM pull_requests WHERE id = $1 AND repo_id = $2"
    )
    .bind(pull_id)
    .bind(repo_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch pull request: {}", e)))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Pull request not found.".to_string()))?;

    let result = task::spawn_blocking(move || {
        let repo_path = format!("./repos/{}.git", repo_name);
        let repo = match git2::Repository::open(&repo_path) {
            Ok(repo) => repo,
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to open repository: {}", e))),
        };

        let base_ref = format!("refs/heads/{}", pr.base_branch);
        let head_ref = format!("refs/heads/{}", pr.head_branch);

        let base_commit = match repo.find_reference(&base_ref).and_then(|r| r.peel_to_commit()) {
            Ok(commit) => commit,
            Err(e) => return Err((StatusCode::BAD_REQUEST, format!("Base branch not found: {}", e))),
        };
        let head_commit = match repo.find_reference(&head_ref).and_then(|r| r.peel_to_commit()) {
            Ok(commit) => commit,
            Err(e) => return Err((StatusCode::BAD_REQUEST, format!("Head branch not found: {}", e))),
        };

        let base_tree = match base_commit.tree() {
            Ok(tree) => tree,
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get base tree: {}", e))),
        };
        let head_tree = match head_commit.tree() {
            Ok(tree) => tree,
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get head tree: {}", e))),
        };

        let mut opts = DiffOptions::new();
        let diff = match repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut opts)) {
            Ok(diff) => diff,
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create diff: {}", e))),
        };

        match format_diff(&diff) {
            Ok(diff_text) => Ok(diff_text),
            Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to format diff: {}", e))),
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task failed: {}", e)))?;

    result
}

fn format_diff(diff: &git2::Diff<'_>) -> Result<String, git2::Error> {
    let mut diff_text = String::new();
    diff.print(git2::DiffFormat::Patch, |_, _, line| {
        let line_char = match line.origin() {
            '+' | '-' | ' ' => line.origin(),
            _ => ' ',
        };
        diff_text.push(line_char);
        diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        true
    })?;
    Ok(diff_text)
}
