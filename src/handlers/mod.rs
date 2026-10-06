use std::{collections::HashMap, path::{Component, PathBuf}};

use axum::{extract::{Path, Query}, http::{StatusCode, header}, response::{IntoResponse, Response}};

fn build_path(kind: &str, page: &str, name: &str, ext: &str) -> Option<PathBuf> {
    let rel = PathBuf::from(page).join(name);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let mut full = PathBuf::from(kind).join(rel);
    if full.extension().is_none() {
        full.set_extension(ext);
    }
    Some(full)
}

async fn serve_file(path: Option<PathBuf>, mime: &'static str) -> Response {
    let Some(path) = path else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// GET /
pub async fn html_handler_main() -> Response {
    let path = PathBuf::from("html").join("index.html");
    serve_file(Some(path), "text/html; charset=utf-8").await
}

// GET /html/{page}/{*html_name}
pub async fn html_handler(Path((page, html_name)): Path<(String, String)>) -> Response {
    serve_file(
        build_path("html", &page, &html_name, "html"),
        "text/html; charset=utf-8",
    )
    .await
}

// GET /css/{page}/{*css_name}
pub async fn css_handler(Path((page, css_name)): Path<(String, String)>) -> Response {
    let path = PathBuf::from("css").join(&page).join(&css_name);
    serve_file(
        Some(path),
        "text/css; charset=utf-8",
    )
    .await
}

// GET /js/{page}/{*js_name}
pub async fn js_handler(Path((page, js_name)): Path<(String, String)>) -> Response {
    let path = PathBuf::from("js").join(&page).join(&js_name);
    serve_file(
        Some(path),
        "text/javascript; charset=utf-8",
    )
    .await
}

fn image_mime(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => return None,
    })
}

// GET /img/{page}/{*img_name}
pub async fn img_handler(Path((page, img_name)): Path<(String, String)>) -> Response {
    let rel = PathBuf::from(&page).join(&img_name);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let path = PathBuf::from("img").join(rel);

    let Some(mime) = image_mime(&path) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn temp_converter(Query(params): Query<HashMap<String, u64>>) -> String {
    (params.get("celsius").cloned().unwrap_or(0) * 9 / 5 + 32).to_string()
}