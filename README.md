# Git8

## Project Overview

This project implements a simple Git server using Rust and Axum. It provides endpoints for creating, deleting, and managing Git repositories.

## API Endpoints

### Public Routes

*   `POST /register`: Register a new user.
*   `POST /login`: Log in and receive an authentication token.
*   `GET /repos`: List all available repositories.
*   `GET /repos/:name/branches`: List branches for a repository.
*   `GET /repos/:name/tree/:branch`: List files in the root of a branch.
*   `GET /repos/:name/tree/:branch/*path`: List files in a subdirectory of a branch.
*   `GET /repos/:name/commits/:branch`: Get the commit history for a branch.

### Protected Routes (require authentication)

*   `POST /repos`: Create a new repository.
*   `DELETE /repos/:name`: Delete a repository.

## `curl` Examples

*   **List repositories:**

    ```bash
    curl http://localhost:3000/repos
    ```

*   **List branches for `test` repo:**

    ```bash
    curl http://localhost:3000/repos/test/branches
    ```

*   **List files in `main` branch of `test` repo:**

    ```bash
    curl http://localhost:3000/repos/test/tree/main
    ```

*   **List commit history for `main` branch of `test` repo:**

    ```bash
    curl http://localhost:3000/repos/test/commits/main
    ```

*   **Register a new user:**

    ```bash
    curl -X POST -H "Content-Type: application/json" -d '{"username": "admin", "password": "pswd"}' http://localhost:3000/register
    ```

*   **Login:**

    ```bash
    curl -X POST -H "Content-Type: application/json" -d '{"username": "admin", "password": "pswd"}' http://localhost:3000/login
    ```

*   **Create a new repository (authenticated):**

    First, log in and get your token. Then, use the token in the `Authorization` header. The request body should be a JSON object with the repository `name` and a boolean `public` field.

    ```bash
    # Replace <token> with the token from the login response
    curl -X POST -H "Authorization: Bearer <token>" -H "Content-Type: application/json" -d '{"name": "my-new-repo", "public": false}' http://localhost:3000/repos
    ```

*   **Delete a repository (authenticated):**

    ```bash
    # Replace <token> with the token from the login response
    curl -X DELETE -H "Authorization: Bearer <token>" http://localhost:3000/repos/my-new-repo
    ```
