use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use std::process::Stdio;
use std::str::FromStr;
use tokio::io::AsyncWriteExt;

pub async fn handler(req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(format!("Failed to read request body: {}", e)))
                .unwrap();
        }
    };

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("http-backend");

    cmd.env("GIT_PROJECT_ROOT", "./repos");
    cmd.env("GIT_HTTP_EXPORT_ALL", "");
    cmd.env("PATH_INFO", parts.uri.path());
    cmd.env("REQUEST_METHOD", parts.method.as_str());
    cmd.env("QUERY_STRING", parts.uri.query().unwrap_or(""));
    if let Some(content_type) = parts.headers.get("content-type") {
        if let Ok(ct_str) = content_type.to_str() {
            cmd.env("CONTENT_TYPE", ct_str);
        }
    }

    cmd.stdout(Stdio::piped());
    cmd.stdin(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("Failed to spawn git http-backend: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Failed to spawn git http-backend"))
                .unwrap();
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(&body_bytes).await {
            eprintln!("Failed to write to git http-backend stdin: {}", e);
        }
    }

    let output = match child.wait_with_output().await {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Failed to wait for git http-backend: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Failed to execute git http-backend"))
                .unwrap();
        }
    };

    if !output.status.success() {
        eprintln!(
            "git http-backend exited with error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut headers_end = 0;
    if let Some(pos) = output.stdout.windows(2).position(|w| w == b"\n\n") {
        headers_end = pos + 2;
    } else if let Some(pos) = output.stdout.windows(4).position(|w| w == b"\r\n\r\n") {
        headers_end = pos + 4;
    }

    if headers_end == 0 {
        if !output.stderr.is_empty() {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain")
                .body(Body::from(output.stderr))
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(output.stdout))
            .unwrap();
    }

    let headers_part = &output.stdout[..headers_end];
    let body_part = &output.stdout[headers_end..];

    let mut response = Response::builder();
    for line in std::str::from_utf8(headers_part).unwrap_or("").lines() {
        if line.is_empty() {
            continue;
        }

        if line.to_lowercase().starts_with("status:") {
            if let Some(status_str) = line.split_whitespace().nth(1) {
                if let Ok(status_code) = StatusCode::from_str(status_str) {
                    response = response.status(status_code);
                }
            }
        } else if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                response = response.header(key, value);
            }
        }
    }

    response.body(Body::from(body_part.to_vec())).unwrap_or_else(|e| {
        eprintln!("Failed to build response: {}", e);
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Failed to construct response"))
            .unwrap()
    })
}
