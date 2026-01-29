use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::auth::{AuthUser, PermissiveAuthUser};
use crate::AppState;

#[derive(Serialize, FromRow, Debug)]
pub struct Review {
    pub id: i32,
    pub pull_request_id: i32,
    pub reviewer_id: i32,
    pub status: String,
    pub body: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct NewReview {
    pub status: ReviewStatus,
    pub body: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateReview {
    pub status: Option<ReviewStatus>,
    pub body: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum ReviewStatus {
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "changes_requested")]
    ChangesRequested,
}

impl std::fmt::Display for ReviewStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewStatus::Approved => write!(f, "approved"),
            ReviewStatus::ChangesRequested => write!(f, "changes_requested"),
        }
    }
}


#[axum::debug_handler]
pub async fn create_review(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((_repo_name, pull_id)): Path<(String, i32)>,
    Json(new_review): Json<NewReview>,
) -> Result<impl IntoResponse, (StatusCode, String)> {

    let review = sqlx::query_as::<_, Review>(
        r#"
        INSERT INTO reviews (pull_request_id, reviewer_id, status, body)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#
    )
    .bind(pull_id)
    .bind(user.id)
    .bind(new_review.status.to_string())
    .bind(new_review.body)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create review: {}", e)))?;

    Ok((StatusCode::CREATED, Json(review)))
}

#[axum::debug_handler]
pub async fn list_reviews(
    State(state): State<AppState>,
    PermissiveAuthUser(_user): PermissiveAuthUser,
    Path((_repo_name, pull_id)): Path<(String, i32)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let reviews = sqlx::query_as::<_, Review>(
        "SELECT * FROM reviews WHERE pull_request_id = $1"
    )
    .bind(pull_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch reviews: {}", e)))?;

    Ok(Json(reviews))
}

#[axum::debug_handler]
pub async fn get_review(
    State(state): State<AppState>,
    PermissiveAuthUser(_user): PermissiveAuthUser,
    Path((_repo_name, _pull_id, review_id)): Path<(String, i32, i32)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let review = sqlx::query_as::<_, Review>(
        "SELECT * FROM reviews WHERE id = $1"
    )
    .bind(review_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch review: {}", e)))?;

    match review {
        Some(review) => Ok(Json(review)),
        None => Err((StatusCode::NOT_FOUND, "Review not found".to_string())),
    }
}

#[axum::debug_handler]
pub async fn update_review(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((_repo_name, _pull_id, review_id)): Path<(String, i32, i32)>,
    Json(update_review): Json<UpdateReview>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let current_review = sqlx::query_as::<_, Review>(
        "SELECT * FROM reviews WHERE id = $1 AND reviewer_id = $2"
    )
    .bind(review_id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch review: {}", e)))?;

    let current_review = match current_review {
        Some(review) => review,
        None => return Err((StatusCode::NOT_FOUND, "Review not found or you don't have permission to update it".to_string())),
    };

    let status = update_review.status.map(|s| s.to_string()).unwrap_or(current_review.status);
    let body = update_review.body.or(current_review.body);

    let updated_review = sqlx::query_as::<_, Review>(
        r#"
        UPDATE reviews
        SET status = $1, body = $2, updated_at = now()
        WHERE id = $3
        RETURNING *
        "#
    )
    .bind(status)
    .bind(body)
    .bind(review_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update review: {}", e)))?;

    Ok(Json(updated_review))
}

#[axum::debug_handler]
pub async fn delete_review(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((_repo_name, _pull_id, review_id)): Path<(String, i32, i32)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query(
        "DELETE FROM reviews WHERE id = $1 AND reviewer_id = $2"
    )
    .bind(review_id)
    .bind(user.id)
    .execute(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to delete review: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Review not found or you don't have permission to delete it".to_string(),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
