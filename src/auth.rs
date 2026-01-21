use axum::{
    extract::{FromRequestParts, State},
    http::{request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use async_trait::async_trait;
use bcrypt::{hash, verify, DEFAULT_COST};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::AppState;

#[derive(Debug, Serialize, FromRow, Clone)]
pub struct User {
    pub id: i32,
    username: String,
    #[serde(skip_serializing)]
    password_hash: String,
}

pub struct AuthUser(pub User);

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = get_token_from_header(&parts.headers).ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, "Missing or invalid authorization header").into_response()
        })?;

        let user = validate_token(token, state).await.map_err(|e| e.into_response())?;
        Ok(AuthUser(user))
    }
}

#[derive(Clone)]
pub struct PermissiveAuthUser(pub Option<User>);

#[async_trait]
impl FromRequestParts<AppState> for PermissiveAuthUser {
    type Rejection = Response; // This rejection is infallible, but the trait requires it.

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = if let Some(token) = get_token_from_header(&parts.headers) {
            validate_token(token, state).await.ok()
        } else {
            None
        };
        Ok(PermissiveAuthUser(user))
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginUser {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    token: String,
}

pub async fn register_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateUser>,
) -> impl IntoResponse {
    let password_hash = match hash(payload.password, DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password").into_response()
        }
    };

    let result = sqlx::query_as::<_, User>(
        "INSERT INTO users (username, password_hash) VALUES ($1, $2) RETURNING id, username, password_hash",
    )
    .bind(&payload.username)
    .bind(&password_hash)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(user) => (StatusCode::CREATED, Json(user)).into_response(),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            (StatusCode::CONFLICT, "Username already exists").into_response()
        }
        Err(e) => {
            tracing::error!("Failed to register user: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to register user").into_response()
        }
    }
}

pub async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<LoginUser>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, User>("SELECT id, username, password_hash FROM users WHERE username = $1")
        .bind(&payload.username)
        .fetch_one(&state.pool)
        .await;

    let user = match result {
        Ok(user) => user,
        Err(sqlx::Error::RowNotFound) => {
            return (StatusCode::UNAUTHORIZED, "Invalid username or password").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch user: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to login").into_response();
        }
    };

    if let Ok(true) = verify(&payload.password, &user.password_hash) {
        let token: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let result = sqlx::query("INSERT INTO sessions (token, user_id) VALUES ($1, $2)")
            .bind(&token)
            .bind(user.id)
            .execute(&state.pool)
            .await;

        match result {
            Ok(_) => (StatusCode::OK, Json(LoginResponse { token })).into_response(),
            Err(e) => {
                tracing::error!("Failed to create session: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create session").into_response()
            }
        }
    } else {
        (StatusCode::UNAUTHORIZED, "Invalid username or password").into_response()
    }
}

fn get_token_from_header(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("Authorization")
        .and_then(|auth_header| auth_header.to_str().ok())
        .and_then(|auth_str| auth_str.strip_prefix("Bearer "))
}

async fn validate_token(token: &str, state: &AppState) -> Result<User, StatusCode> {
    sqlx::query_as::<_, User>(
        "SELECT u.id, u.username, u.password_hash FROM users u JOIN sessions s ON u.id = s.user_id WHERE s.token = $1",
    )
    .bind(token)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => StatusCode::UNAUTHORIZED,
        _ => {
            tracing::error!("Token validation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })
}
