use std::{
    collections::HashMap,
    path::{Component, PathBuf},
};

use axum::{
    extract::{Path, Query},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
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
    serve_file(
        build_path("css", &page, &css_name, "css"),
        "text/css; charset=utf-8",
    )
    .await
}

// GET /js/{page}/{*js_name}
pub async fn js_handler(Path((page, js_name)): Path<(String, String)>) -> Response {
    serve_file(
        build_path("js", &page, &js_name, "js"),
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn temp_converter_celsius_to_fahrenheit(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

// GET /temp_converter?celsius=<f64>
// Non-finite input ("NaN", "inf") or a result overflowing to infinity -> 400.
pub async fn temp_converter(
    Query(params): Query<HashMap<String, f64>>,
) -> Result<String, StatusCode> {
    let fahrenheit =
        temp_converter_celsius_to_fahrenheit(params.get("celsius").copied().unwrap_or(0.0));
    if !fahrenheit.is_finite() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(fahrenheit.to_string())
}

pub async fn health() -> StatusCode {
    StatusCode::OK
}

pub async fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_path_adds_default_extension() {
        assert_eq!(
            build_path("css", "main", "main", "css"),
            Some(PathBuf::from("css").join("main").join("main.css"))
        );
    }

    #[test]
    fn build_path_keeps_existing_extension() {
        assert_eq!(
            build_path("js", "main", "lib.min.js", "js"),
            Some(PathBuf::from("js").join("main").join("lib.min.js"))
        );
        assert_eq!(
            build_path("html", "main", "data.txt", "html"),
            Some(PathBuf::from("html").join("main").join("data.txt"))
        );
    }

    #[test]
    fn build_path_allows_nested_names() {
        assert_eq!(
            build_path("css", "main", "sub/dir/style", "css"),
            Some(PathBuf::from("css/main/sub/dir/style.css"))
        );
    }

    #[test]
    fn build_path_rejects_traversal_and_absolute() {
        assert_eq!(build_path("css", "..", "x", "css"), None);
        assert_eq!(build_path("css", "main", "../x", "css"), None);
        assert_eq!(build_path("css", "main", "a/../../x", "css"), None);
        // inner "." is dropped by Path::components and stays inside the dir
        assert_eq!(
            build_path("css", "main", "./x", "css"),
            Some(PathBuf::from("css/main/x.css"))
        );
        assert_eq!(build_path("css", "main", "/etc/passwd", "css"), None);
    }

    #[test]
    fn image_mime_known_types() {
        let cases = [
            ("a.png", "image/png"),
            ("a.jpg", "image/jpeg"),
            ("a.jpeg", "image/jpeg"),
            ("a.gif", "image/gif"),
            ("a.webp", "image/webp"),
            ("a.svg", "image/svg+xml"),
            ("a.ico", "image/x-icon"),
            ("a.bmp", "image/bmp"),
            ("a.avif", "image/avif"),
        ];
        for (file, mime) in cases {
            assert_eq!(image_mime(std::path::Path::new(file)), Some(mime), "{file}");
        }
    }

    #[test]
    fn image_mime_is_case_insensitive() {
        assert_eq!(image_mime(std::path::Path::new("A.PNG")), Some("image/png"));
        assert_eq!(
            image_mime(std::path::Path::new("b.JpEg")),
            Some("image/jpeg")
        );
    }

    #[test]
    fn image_mime_unknown_or_missing_extension() {
        assert_eq!(image_mime(std::path::Path::new("a.txt")), None);
        assert_eq!(image_mime(std::path::Path::new("noext")), None);
        assert_eq!(image_mime(std::path::Path::new(".png")), None);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn celsius_to_fahrenheit_formula() {
        let cases = [
            (0.0, 32.0),
            (100.0, 212.0),
            (37.0, 98.6),
            (36.6, 97.88),
            (-40.0, -40.0),
            (-273.15, -459.67),
            (1.0, 33.8),
        ];
        for (c, f) in cases {
            assert_close(temp_converter_celsius_to_fahrenheit(c), f);
        }
    }

    async fn call(celsius: Option<f64>) -> Result<String, StatusCode> {
        let params = celsius
            .map(|v| HashMap::from([("celsius".to_string(), v)]))
            .unwrap_or_default();
        temp_converter(Query(params)).await
    }

    #[tokio::test]
    async fn temp_converter_handler() {
        assert_eq!(call(None).await, Ok("32".into()));
        assert_eq!(call(Some(0.0)).await, Ok("32".into()));
        assert_eq!(call(Some(100.0)).await, Ok("212".into()));
        assert_eq!(call(Some(-40.0)).await, Ok("-40".into()));
        assert_close(call(Some(37.0)).await.unwrap().parse().unwrap(), 98.6);
    }

    #[tokio::test]
    async fn temp_converter_rejects_non_finite() {
        for c in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            f64::MIN,
        ] {
            assert_eq!(call(Some(c)).await, Err(StatusCode::BAD_REQUEST), "{c}");
        }
    }

    #[tokio::test]
    async fn health_is_ok() {
        assert_eq!(health().await, StatusCode::OK);
    }

    #[tokio::test]
    async fn version_matches_cargo_toml() {
        assert_eq!(version().await, env!("CARGO_PKG_VERSION"));
    }
}
