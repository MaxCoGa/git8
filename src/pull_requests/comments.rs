use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::auth::{AuthUser, PermissiveAuthUser};
use crate::AppState;

#[derive(Serialize, FromRow)]
pub struct PullRequestComment {
    id: i32,
    pull_request_id: i32,
    body: String,
    author_id: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct NewComment {
    pub body: String,
}

#[axum::debug_handler]
pub async fn create_comment(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((repo_name, pull_request_id)): Path<(String, i32)>,
    Json(new_comment): Json<NewComment>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let comment_result = sqlx::query_as!(
        PullRequestComment,
        r#"
        WITH pull_request_repo AS (
            SELECT repo_id FROM pull_requests WHERE id = $1
        ), repo_access AS (
            SELECT r.id
            FROM repositories r
            JOIN pull_request_repo prr ON r.id = prr.repo_id
            WHERE r.name = $4 AND (r.public OR r.user_id = $3)
        )
        INSERT INTO pull_request_comments (pull_request_id, body, author_id)
        SELECT $1, $2, $3
        FROM repo_access
        RETURNING id, pull_request_id, body, author_id, created_at
        "#,
        pull_request_id,
        new_comment.body,
        user.id,
        repo_name
    )
    .fetch_one(&state.pool)
    .await;

    match comment_result {
        Ok(comment) => Ok((StatusCode::CREATED, Json(comment))),
        Err(sqlx::Error::RowNotFound) => Err((
            StatusCode::FORBIDDEN,
            "Pull request not found, repository not found, or you don't have permission to comment.".to_string(),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create comment: {}", e),
        )),
    }
}

#[axum::debug_handler]
pub async fn list_comments(
    State(state): State<AppState>,
    PermissiveAuthUser(user): PermissiveAuthUser,
    Path((repo_name, pull_request_id)): Path<(String, i32)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = user.map(|u| u.id);

    let repo_exists_and_has_access = sqlx::query!(
        "SELECT EXISTS(SELECT 1 FROM repositories WHERE name = $1 AND (public OR user_id = $2))",
        repo_name,
        user_id
    )
    .fetch_one(&state.pool)
    .await
    .map(|r| r.exists.unwrap_or(false))
    .unwrap_or(false);

    if !repo_exists_and_has_access {
        return Err((StatusCode::NOT_FOUND, "Repository not found or you don't have permission to view it.".to_string()));
    }

    let comments = sqlx::query_as!(
        PullRequestComment,
        r#"
        SELECT prc.id, prc.pull_request_id, prc.body, prc.author_id, prc.created_at
        FROM pull_request_comments prc
        JOIN pull_requests pr ON prc.pull_request_id = pr.id
        JOIN repositories r ON pr.repo_id = r.id
        WHERE r.name = $1 AND pr.id = $2 AND (r.public OR r.user_id = $3)
        "#,
        repo_name,
        pull_request_id,
        user_id,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list comments: {}", e),
        )
    })?;

    Ok(Json(comments))
}
