use axum::{
    body::Body,
    extract::{Extension, State},
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use bcrypt::{hash, verify, DEFAULT_COST};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::AppState;

#[derive(Debug, Serialize, FromRow, Clone)]
pub struct User {
    id: i32,
    username: String,
    #[serde(skip_serializing)]
    password_hash: String,
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
    Extension(state): Extension<AppState>,
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
    Extension(state): Extension<AppState>,
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

pub async fn auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let token = if let Some(auth_header) = request.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                token
            } else {
                return (StatusCode::UNAUTHORIZED, "Invalid authorization header format").into_response();
            }
        } else {
            return (StatusCode::UNAUTHORIZED, "Invalid authorization header").into_response();
        }
    } else {
        return (StatusCode::UNAUTHORIZED, "Missing authorization header").into_response();
    };

    let result = sqlx::query_as::<_, User>(
        "SELECT u.id, u.username, u.password_hash FROM users u JOIN sessions s ON u.id = s.user_id WHERE s.token = $1",
    )
    .bind(token)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(user) => {
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Err(sqlx::Error::RowNotFound) => {
            (StatusCode::UNAUTHORIZED, "Invalid token").into_response()
        }
        Err(e) => {
            tracing::error!("Failed to validate token: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to validate token").into_response()
        }
    }
}
