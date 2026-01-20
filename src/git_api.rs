use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response, Json},
};
use serde::Serialize;
use std::path::Path as StdPath;


#[derive(Serialize)]
pub struct Repo {
    name: String,
}

#[derive(Serialize)]
pub struct Branch {
    name: String,
}

#[derive(Serialize)]
pub struct Commit {
    id: String,
    message: String,
    author: String,
    date: String,
}

#[derive(Serialize)]
pub struct TreeEntry {
    name: String,
    entry_type: String, // "blob" (file) or "tree" (directory)
}

pub async fn list_repos_handler() -> Response {
    let mut repos = Vec::new();
    let paths = match std::fs::read_dir("./repos") {
        Ok(paths) => paths,
        Err(e) => {
            tracing::error!("Failed to read repos directory: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list repositories").into_response();
        }
    };

    for path in paths {
        if let Ok(path) = path {
            if let Some(repo_name) = path.file_name().to_str() {
                if repo_name.ends_with(".git") {
                    repos.push(Repo { name: repo_name.strip_suffix(".git").unwrap().to_string() });
                }
            }
        }
    }
    Json(repos).into_response()
}

pub async fn list_branches_handler(Path(name): Path<String>) -> Response {
    let repo_path = StdPath::new("./repos").join(format!("{}.git", name));
    let repo = match git2::Repository::open(repo_path) {
        Ok(repo) => repo,
        Err(_) => return (StatusCode::NOT_FOUND, "Repository not found").into_response(),
    };

    let branches = match repo.branches(None) {
        Ok(branches) => branches,
        Err(e) => {
            tracing::error!("Failed to list branches for {}: {}", name, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list branches").into_response();
        }
    };

    let mut branch_list = Vec::new();
    for branch in branches {
        if let Ok((branch, _)) = branch {
            if let Ok(Some(branch_name)) = branch.name() {
                branch_list.push(Branch { name: branch_name.to_string() });
            }
        }
    }

    Json(branch_list).into_response()
}

pub async fn list_files_root_handler(Path((name, branch)): Path<(String, String)>) -> Response {
    list_files_implementation(name, branch, None).await
}

pub async fn list_files_subdirectory_handler(Path((name, branch, path)): Path<(String, String, String)>) -> Response {
    list_files_implementation(name, branch, Some(path)).await
}

pub async fn list_files_implementation(name: String, branch: String, path: Option<String>) -> Response {
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

pub async fn commit_history_handler(Path((name, branch_name)): Path<(String, String)>) -> Response {
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

pub async fn create_repo_handler(Path(name): Path<String>) -> Response {
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return (StatusCode::BAD_REQUEST, "Invalid repository name").into_response();
    }

    let repo_name = if name.ends_with(".git") {
        name
    } else {
        format!("{}.git", name)
    };

    let path = StdPath::new("./repos").join(&repo_name);

    if path.exists() {
        return (StatusCode::CONFLICT, "Repository already exists").into_response();
    }

    match git2::Repository::init_bare(&path) {
        Ok(repo) => {
            match repo.config() {
                Ok(mut config) => {
                    if let Err(e) = config.set_bool("http.receivepack", true) {
                        tracing::error!("Failed to set http.receivepack for {}: {}", repo_name, e);
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Failed to configure repository for pushes",
                        )
                            .into_response();
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to open config for {}: {}", repo_name, e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to configure repository",
                    )
                        .into_response();
                }
            }

            tracing::info!("Created and configured new repository: {}", repo_name);
            (StatusCode::CREATED, format!("Repository {} created", repo_name)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to create repository: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create repository",
            )
                .into_response()
        }
    }
}

pub async fn delete_repo_handler(Path(name): Path<String>) -> Response {
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return (StatusCode::BAD_REQUEST, "Invalid repository name").into_response();
    }

    let repo_name = if name.ends_with(".git") {
        name
    } else {
        format!("{}.git", name)
    };

    let path = StdPath::new("./repos").join(&repo_name);

    if !path.exists() {
        return (StatusCode::NOT_FOUND, "Repository not found").into_response();
    }

    match std::fs::remove_dir_all(&path) {
        Ok(_) => {
            tracing::info!("Deleted repository: {}", repo_name);
            (StatusCode::OK, format!("Repository {} deleted", repo_name)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to delete repository '{}': {}", repo_name, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete repository",
            )
                .into_response()
        }
    }
}
