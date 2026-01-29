# Git8

## Project Overview

This project implements a simple Git server using Rust and Axum. It provides endpoints for creating, deleting, and managing Git repositories.

## API Endpoints

### Authentication

*   `POST /register`: Register a new user.
*   `POST /login`: Log in and receive an authentication token.

### Repositories

*   `GET /repos`: List all available public repositories.
*   `POST /repos`: Create a new repository (requires authentication).
*   `DELETE /repos/:name`: Delete a repository (requires authentication).
*   `GET /repos/:name/branches`: List branches for a repository.
*   `GET /repos/:name/tree/:branch`: List files in the root of a branch.
*   `GET /repos/:name/tree/:branch/*path`: List files in a subdirectory of a branch.
*   `GET /repos/:name/commits/:branch`: Get the commit history for a branch.

### Pull Requests

*   `POST /repos/:name/pulls`: Create a new pull request (requires authentication).
*   `GET /repos/:name/pulls`: List all pull requests for a repository.
*   `GET /repos/:name/pulls/:pull_id`: Get a specific pull request.
*   `PATCH /repos/:name/pulls/:pull_id`: Update a pull request (e.g., merge or close).

### Pull Request Diffs

*   `GET /repos/:name/pulls/:pull_id/diff`: Get the diff for a pull request.

### Pull Request Reviews

*   `POST /repos/:name/pulls/:pull_id/reviews`: Create a new review for a pull request (requires authentication).
*   `GET /repos/:name/pulls/:pull_id/reviews`: List all reviews for a pull request.
*   `GET /repos/:name/pulls/:pull_id/reviews/:review_id`: Get a specific review.
*   `PATCH /repos/:name/pulls/:pull_id/reviews/:review_id`: Update a review (requires authentication).
*   `DELETE /repos/:name/pulls/:pull_id/reviews/:review_id`: Delete a review (requires authentication).

### Pull Request Comments

*   `POST /repos/:repo_name/pulls/:pull_request_id/comments`: Add a comment to a pull request (requires authentication).
*   `GET /repos/:repo_name/pulls/:pull_request_id/comments`: List all comments for a pull request.

### Issues

*   `POST /repos/:name/issues`: Create a new issue for a repository (requires authentication).
*   `GET /repos/:name/issues`: List all issues for a repository.
*   `GET /repos/:name/issues/:issue_id`: Get a specific issue.

### Issue Comments

*   `POST /repos/:name/issues/:issue_id/comments`: Add a comment to an issue (requires authentication).
*   `GET /repos/:name/issues/:issue_id/comments`: List all comments for an issue.

### Labels

*   `POST /repos/:name/labels`: Create a new label for a repository.
*   `GET /repos/:name/labels`: List all labels for a repository.
*   `POST /repos/:name/issues/:issue_id/labels/:label_name`: Add a label to an issue.
*   `DELETE /repos/:name/issues/:issue_id/labels/:label_name`: Remove a label from an issue.

### Assignees

*   `POST /repos/:name/issues/:issue_id/assignees/:assignee_username`: Add an assignee to an issue.
*   `DELETE /repos/:name/issues/:issue_id/assignees/:assignee_username`: Remove an assignee from an issue.

## `curl` Examples

### Authentication

*   **Register a new user:**

    ```bash
    curl -X POST -H "Content-Type: application/json" -d '{"username": "admin", "password": "pswd"}' http://localhost:3000/register
    ```

*   **Login:**

    ```bash
    curl -X POST -H "Content-Type: application/json" -d '{"username": "admin", "password": "pswd"}' http://localhost:3000/login
    ```

### Repositories

*   **Create a new repository (authenticated):**

    First, log in and get your token. Then, use the token in the `Authorization` header.

    ```bash
    # Replace <token> with the token from the login response
    curl -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" -d '{"name": "my-new-repo", "public": true}' http://localhost:3000/repos
    ```

*   **List public repositories:**

    ```bash
    curl http://localhost:3000/repos
    ```

*   **Delete a repository (authenticated):**

    ```bash
    # Replace <token> with the token from the login response
    curl -X DELETE -H "Authorization: Bearer <token>" http://localhost:3000/repos/my-new-repo
    ```

*   **List branches for `test-repo` repo:**

    ```bash
    curl http://localhost:3000/repos/test-repo/branches
    ```

### Pull Requests

*   **Create a new pull request (authenticated):**

    ```bash
    # Replace <token> with your auth token
    curl -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
      -d '{"title": "My First Pull Request", "body": "This is the pull request description.", "head_branch": "feature-branch", "base_branch": "main"}' \
      http://localhost:3000/repos/test-repo/pulls
    ```

*   **List pull requests for a repository:**

    ```bash
    curl http://localhost:3000/repos/test-repo/pulls
    ```

*   **Get a specific pull request:**

    ```bash
    curl http://localhost:3000/repos/test-repo/pulls/1
    ```

*   **Update a pull request (authenticated):**

    This can be used to merge a pull request.

    ```bash
    # Replace <token> with your auth token
    curl -X PATCH -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
        -d '{"status": "merged"}' \
        http://localhost:3000/repos/test-repo/pulls/1
    ```

### Pull Request Diffs

*   **Get the diff for a pull request:**

    ```bash
    curl http://localhost:3000/repos/test-repo/pulls/1/diff
    ```

### Pull Request Reviews

*   **Create a new review for a pull request (authenticated):**

    ```bash
    # Replace <token> with your auth token
    curl -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
      -d '{"body": "This looks good!", "status": "approved"}' \
      http://localhost:3000/repos/test-repo/pulls/1/reviews
    ```

*   **List reviews for a pull request:**

    ```bash
    curl http://localhost:3000/repos/test-repo/pulls/1/reviews
    ```

### Pull Request Comments

*   **Add a comment to a pull request (authenticated):**

    ```bash
    # Replace <token> with your auth token and :pull_request_id with a real pull request ID
    curl -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
      -d '{"body": "This is a comment on the pull request."}' \
      http://localhost:3000/repos/test-repo/pulls/1/comments
    ```

*   **List comments for a pull request:**

    ```bash
    curl http://localhost:3000/repos/test-repo/pulls/1/comments
    ```

### Issues

*   **Create a new issue (authenticated):**

    ```bash
    # Replace <token> with your auth token
    curl -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
      -d '{"title": "My First Issue", "body": "This is the issue description."}' \
      http://localhost:3000/repos/my-new-repo/issues
    ```

*   **List issues for a repository:**

    ```bash
    curl http://localhost:3000/repos/my-new-repo/issues
    ```

*   **Get a specific issue:**

    ```bash
    curl http://localhost:3000/repos/my-new-repo/issues/1
    ```

### Issue Comments

*   **Add a comment to an issue (authenticated):**

    ```bash
    # Replace <token> with your auth token and :issue_id with a real issue ID
    curl -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
      -d '{"body": "This is a comment on the issue."}' \
      http://localhost:3000/repos/my-new-repo/issues/1/comments
    ```

*   **List comments for an issue:**

    ```bash
    curl http://localhost:3000/repos/my-new-repo/issues/1/comments
    ```

### Labels

*   **Create a new label (authenticated):**

    ```bash
    curl -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" \
      -d '{"name": "bug", "color": "d73a4a"}' \
      http://localhost:3000/repos/my-new-repo/labels
    ```

*   **List labels for a repository:**

    ```bash
    curl http://localhost:3000/repos/my-new-repo/labels
    ```

*   **Add a label to an issue (authenticated):**

    ```bash
    curl -X POST -H "Authorization: Bearer <token>" http://localhost:3000/repos/my-new-repo/issues/1/labels/bug
    ```

*   **Remove a label from an issue (authenticated):**

    ```bash
    curl -X DELETE -H "Authorization: Bearer <token>" http://localhost:3000/repos/my-new-repo/issues/1/labels/bug
    ```

### Assignees

*   **Add an assignee to an issue (authenticated):**

    ```bash
    curl -X POST -H "Authorization: Bearer <token>" http://localhost:3000/repos/my-new-repo/issues/1/assignees/admin
    ```

*   **Remove an assignee from an issue (authenticated):**

    ```bash
    curl -X DELETE -H "Authorization: Bearer <token>" http://localhost:3000/repos/my-new-repo/issues/1/assignees/admin
    ```
