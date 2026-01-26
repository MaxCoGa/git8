use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::auth::{AuthUser, PermissiveAuthUser};
use crate::AppState;

#[derive(Serialize, FromRow, Clone)]
pub struct Label {
    pub id: i32,
    pub repo_id: i32,
    pub name: String,
    pub color: String,
}

#[derive(Serialize, FromRow, Clone)]
pub struct DisplayUser {
    pub id: i32,
    pub username: String,
}

#[derive(Serialize, FromRow)]
pub struct Issue {
    pub id: i32,
    pub repo_id: i32,
    pub title: String,
    pub body: Option<String>,
    pub author_id: i32,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct FullIssue {
    #[serde(flatten)]
    pub issue: Issue,
    pub labels: Vec<Label>,
    pub assignees: Vec<DisplayUser>,
    pub author: DisplayUser,
}

#[derive(Deserialize)]
pub struct NewIssue {
    pub title: String,
    pub body: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<String>,
}

#[derive(Serialize, FromRow)]
pub struct IssueComment {
    id: i32,
    issue_id: i32,
    body: String,
    author_id: i32,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct NewComment {
    pub body: String,
}

#[derive(Deserialize)]
pub struct NewLabel {
    pub name: String,
    pub color: String,
}

#[axum::debug_handler]
pub async fn create_label(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(repo_name): Path<String>,
    Json(new_label): Json<NewLabel>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let label = sqlx::query_as!(
        Label,
        r#"
        WITH repo AS (
            SELECT id FROM repositories WHERE name = $1 AND user_id = $2
        )
        INSERT INTO labels (repo_id, name, color)
        SELECT id, $3, $4 FROM repo
        RETURNING id, repo_id, name, color
        "#,
        repo_name,
        user.id,
        new_label.name,
        new_label.color
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create label: {}", e),
        )
    })?;

    Ok((StatusCode::CREATED, Json(label)))
}

#[axum::debug_handler]
pub async fn list_labels(
    State(state): State<AppState>,
    PermissiveAuthUser(user): PermissiveAuthUser,
    Path(repo_name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = user.map(|u| u.id);

    let labels = sqlx::query_as!(
        Label,
        r#"
        SELECT l.id, l.repo_id, l.name, l.color
        FROM labels l
        JOIN repositories r ON l.repo_id = r.id
        WHERE r.name = $1 AND (r.public OR r.user_id = $2)
        "#,
        repo_name,
        user_id
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list labels: {}", e),
        )
    })?;

    Ok(Json(labels))
}

#[axum::debug_handler]
pub async fn create_issue(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(repo_name): Path<String>,
    Json(new_issue): Json<NewIssue>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start transaction: {}", e),
        )
    })?;

    let repo_id_result = sqlx::query!(r#"SELECT id FROM repositories WHERE name = $1 AND (public OR user_id = $2)"#, repo_name, user.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get repo: {}", e)))?;

    let repo_id = match repo_id_result {
        Some(repo) => repo.id,
        None => return Err((StatusCode::FORBIDDEN, "Repository not found or you don't have permission to create an issue here.".to_string())),
    };

    let issue = sqlx::query_as!(
        Issue,
        r#"
        INSERT INTO issues (repo_id, title, body, author_id)
        VALUES ($1, $2, $3, $4)
        RETURNING id, repo_id, title, body, author_id, status, created_at
        "#,
        repo_id,
        new_issue.title,
        new_issue.body,
        user.id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create issue: {}", e)))?;

    if !new_issue.labels.is_empty() {
        let labels_to_add = sqlx::query_as!(Label, "SELECT id, repo_id, name, color FROM labels WHERE repo_id = $1 AND name = ANY($2)", repo_id, &new_issue.labels)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to find labels: {}", e)))?;
        
        for label in labels_to_add {
            sqlx::query!(r#"INSERT INTO issue_labels (issue_id, label_id) VALUES ($1, $2)"#, issue.id, label.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to add label to issue: {}", e)))?;
        }
    }

    if !new_issue.assignees.is_empty() {
        let users_to_add = sqlx::query_as!(DisplayUser, "SELECT id, username FROM users WHERE username = ANY($1)", &new_issue.assignees)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to find users: {}", e)))?;

        for assignee in users_to_add {
            sqlx::query!(r#"INSERT INTO issue_assignees (issue_id, user_id) VALUES ($1, $2)"#, issue.id, assignee.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to add assignee to issue: {}", e)))?;
        }
    }

    tx.commit().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to commit transaction: {}", e)))?;

    let full_issue = get_full_issue(&state, repo_name, issue.id, Some(user.id)).await?.1;

    Ok((StatusCode::CREATED, Json(full_issue)))
}

#[axum::debug_handler]
pub async fn get_issue(
    State(state): State<AppState>,
    PermissiveAuthUser(user): PermissiveAuthUser,
    Path((repo_name, issue_id)): Path<(String, i32)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = user.map(|u| u.id);
    let (_status, full_issue) = get_full_issue(&state, repo_name, issue_id, user_id).await?;
    Ok(Json(full_issue))
}

async fn get_full_issue(state: &AppState, repo_name: String, issue_id: i32, user_id: Option<i32>) -> Result<(StatusCode, FullIssue), (StatusCode, String)> {
    let issue = sqlx::query_as!(
        Issue,
        r#"
        SELECT i.id, i.repo_id, i.title, i.body, i.author_id, i.status, i.created_at
        FROM issues i
        JOIN repositories r ON i.repo_id = r.id
        WHERE r.name = $1 AND i.id = $2 AND (r.public OR r.user_id = $3)
        "#,
        repo_name,
        issue_id,
        user_id,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => (StatusCode::NOT_FOUND, "Issue not found".to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch issue: {}", e)),
    })?;

    let labels = sqlx::query_as!(
        Label,
        "SELECT l.id, l.repo_id, l.name, l.color FROM labels l JOIN issue_labels il ON l.id = il.label_id WHERE il.issue_id = $1",
        issue.id
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch labels: {}", e)))?;

    let assignees = sqlx::query_as!(
        DisplayUser,
        "SELECT u.id, u.username FROM users u JOIN issue_assignees ia ON u.id = ia.user_id WHERE ia.issue_id = $1",
        issue.id
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch assignees: {}", e)))?;

    let author = sqlx::query_as!(
        DisplayUser,
        "SELECT id, username FROM users WHERE id = $1",
        issue.author_id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch author: {}", e)))?;

    Ok((StatusCode::OK, FullIssue { issue, labels, assignees, author }))
}

#[axum::debug_handler]
pub async fn list_issues(
    State(state): State<AppState>,
    PermissiveAuthUser(user): PermissiveAuthUser,
    Path(repo_name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_id = user.map(|u| u.id);

    let issues = sqlx::query_as!(
        Issue,
        r#"
        SELECT i.id, i.repo_id, i.title, i.body, i.author_id, i.status, i.created_at
        FROM issues i
        JOIN repositories r ON i.repo_id = r.id
        WHERE r.name = $1 AND (r.public OR r.user_id = $2)
        "#,
        repo_name,
        user_id,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list issues: {}", e),
        )
    })?;

    let mut full_issues = Vec::new();
    for issue in issues {
        let (_, full_issue) = get_full_issue(&state, repo_name.clone(), issue.id, user_id).await?;
        full_issues.push(full_issue);
    }
    
    Ok(Json(full_issues))
}

#[axum::debug_handler]
pub async fn add_label_to_issue(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((repo_name, issue_id, label_name)): Path<(String, i32, String)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start transaction: {}", e)))?;
    
    let issue_repo_label = sqlx::query!(
        r#"
        SELECT 
            i.id as issue_id, 
            r.id as repo_id, 
            l.id as label_id
        FROM issues i
        JOIN repositories r ON i.repo_id = r.id
        CROSS JOIN labels l
        WHERE r.name = $1 AND i.id = $2 AND l.name = $3 AND l.repo_id = r.id AND (r.public OR r.user_id = $4)
        "#,
        repo_name,
        issue_id,
        label_name,
        user.id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to validate resources: {}", e)))?
    .ok_or((StatusCode::NOT_FOUND, "Issue, repository, or label not found, or you don't have permission.".to_string()))?;

    sqlx::query!(
        "INSERT INTO issue_labels (issue_id, label_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        issue_repo_label.issue_id,
        issue_repo_label.label_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to add label to issue: {}", e)))?;

    tx.commit().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to commit transaction: {}", e)))?;

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn remove_label_from_issue(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((repo_name, issue_id, label_name)): Path<(String, i32, String)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start transaction: {}", e)))?;
    
    let issue_repo_label = sqlx::query!(
        r#"
        SELECT 
            i.id as issue_id, 
            l.id as label_id
        FROM issues i
        JOIN repositories r ON i.repo_id = r.id
        CROSS JOIN labels l
        WHERE r.name = $1 AND i.id = $2 AND l.name = $3 AND l.repo_id = r.id AND (r.public OR r.user_id = $4)
        "#,
        repo_name,
        issue_id,
        label_name,
        user.id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to validate resources: {}", e)))?
    .ok_or((StatusCode::NOT_FOUND, "Issue, repository, or label not found, or you don't have permission.".to_string()))?;

    sqlx::query!(
        "DELETE FROM issue_labels WHERE issue_id = $1 AND label_id = $2",
        issue_repo_label.issue_id,
        issue_repo_label.label_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to remove label from issue: {}", e)))?;

    tx.commit().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to commit transaction: {}", e)))?;

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn add_assignee_to_issue(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((repo_name, issue_id, assignee_username)): Path<(String, i32, String)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start transaction: {}", e)))?;
    
    let issue_repo_assignee = sqlx::query!(
        r#"
        SELECT 
            i.id as issue_id,
            u.id as user_to_assign_id
        FROM issues i
        JOIN repositories r ON i.repo_id = r.id
        CROSS JOIN users u
        WHERE r.name = $1 AND i.id = $2 AND u.username = $3 AND (r.public OR r.user_id = $4)
        "#,
        repo_name,
        issue_id,
        assignee_username,
        user.id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to validate resources: {}", e)))?
    .ok_or((StatusCode::NOT_FOUND, "Issue, repository, or user to assign not found, or you don't have permission.".to_string()))?;

    sqlx::query!(
        "INSERT INTO issue_assignees (issue_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        issue_repo_assignee.issue_id,
        issue_repo_assignee.user_to_assign_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to add assignee to issue: {}", e)))?;

    tx.commit().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to commit transaction: {}", e)))?;

    Ok(StatusCode::OK)
}

#[axum::debug_handler]
pub async fn remove_assignee_from_issue(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((repo_name, issue_id, assignee_username)): Path<(String, i32, String)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start transaction: {}", e)))?;
    
    let issue_repo_assignee = sqlx::query!(
        r#"
        SELECT 
            i.id as issue_id,
            u.id as user_to_remove_id
        FROM issues i
        JOIN repositories r ON i.repo_id = r.id
        CROSS JOIN users u
        WHERE r.name = $1 AND i.id = $2 AND u.username = $3 AND (r.public OR r.user_id = $4)
        "#,
        repo_name,
        issue_id,
        assignee_username,
        user.id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to validate resources: {}", e)))?
    .ok_or((StatusCode::NOT_FOUND, "Issue, repository, or user to remove not found, or you don't have permission.".to_string()))?;

    sqlx::query!(
        "DELETE FROM issue_assignees WHERE issue_id = $1 AND user_id = $2",
        issue_repo_assignee.issue_id,
        issue_repo_assignee.user_to_remove_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to remove assignee from issue: {}", e)))?;

    tx.commit().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to commit transaction: {}", e)))?;

    Ok(StatusCode::OK)
}


#[axum::debug_handler]
pub async fn create_comment(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((repo_name, issue_id)): Path<(String, i32)>,
    Json(new_comment): Json<NewComment>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let comment_result = sqlx::query_as!(
        IssueComment,
        r#"
        WITH issue_repo AS (
            SELECT repo_id FROM issues WHERE id = $1
        ), repo_access AS (
            SELECT r.id
            FROM repositories r
            JOIN issue_repo ir ON r.id = ir.repo_id
            WHERE r.name = $4 AND (r.public OR r.user_id = $3)
        )
        INSERT INTO issue_comments (issue_id, body, author_id)
        SELECT $1, $2, $3
        FROM repo_access
        RETURNING id, issue_id, body, author_id, created_at
        "#,
        issue_id,
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
            "Issue not found, repository not found, or you don't have permission to comment.".to_string(),
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
    Path((repo_name, issue_id)): Path<(String, i32)>,
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
        IssueComment,
        r#"
        SELECT ic.id, ic.issue_id, ic.body, ic.author_id, ic.created_at
        FROM issue_comments ic
        JOIN issues i ON ic.issue_id = i.id
        JOIN repositories r ON i.repo_id = r.id
        WHERE r.name = $1 AND i.id = $2 AND (r.public OR r.user_id = $3)
        "#,
        repo_name,
        issue_id,
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
