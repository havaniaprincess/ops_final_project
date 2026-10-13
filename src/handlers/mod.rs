use std::{
    collections::HashMap,
    path::{Component, PathBuf},
};

use axum::{
    extract::{Path, Query},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};

/// Builds a full path to a static file based on its kind, page, name, and extension.
/// Returns `None` if the path contains any components that are not normal (e.g., `..`).
fn build_path(kind: &str, page: &str, name: &str, ext: &str) -> Option<PathBuf> {
    let rel = PathBuf::from(page).join(name);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        log::log!(
            target: "os",
            log::Level::Warn,
            "Path contains any components that are not normal: {}", rel.display(),
        );
        return None;
    }
    let mut full = PathBuf::from(kind).join(rel);
    if full.extension().is_none() {
        full.set_extension(ext);
    }
    Some(full)
}

/// Serves a static file at the given path with the specified MIME type.
/// Returns a `BAD_REQUEST` response if the path is `None`.
/// Returns a `NOT_FOUND` response if the file does not exist.
/// Returns an `INTERNAL_SERVER_ERROR` response if reading the file fails.
async fn serve_file(path: Option<PathBuf>, mime: &'static str) -> Response {
    let Some(path) = path else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        log::log!(
            target: "os",
            log::Level::Warn,
            "Path contains any components that are not normal: {}", path.display(),
        );
        return StatusCode::BAD_REQUEST.into_response();
    }
    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            log::log!(
                target: "os",
                log::Level::Error,
                "Error reading file: {}: {}", e, path.display(),
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /
///
/// Serves the main HTML page (index.html) with the appropriate MIME type.
/// Returns a `BAD_REQUEST` response if the path is invalid.
/// Returns a `NOT_FOUND` response if the file does not exist.
/// Returns an `INTERNAL_SERVER_ERROR` response if reading the file fails.
pub async fn html_handler_main() -> Response {
    let path = PathBuf::from("html").join("index.html");
    serve_file(Some(path), "text/html; charset=utf-8").await
}

/// URL of the crate's rustdoc index. rustdoc names the directory after the
/// crate, which is known at compile time.
pub const DOCS_INDEX: &str = concat!("/", env!("CARGO_CRATE_NAME"), "/index.html");

/// GET /docs
///
/// Redirects (307) to the crate's documentation index.
/// rustdoc pages use links relative to their own directory (`fn.app.html`,
/// `handlers/index.html`), so the page must be opened from its real URL:
/// served at `/docs` those links would resolve to `/fn.app.html` and 404.
pub async fn docs_handler() -> Redirect {
    Redirect::temporary(DOCS_INDEX)
}

/// MIME type for files in the `doc` directory (rustdoc output).
/// Images are delegated to [`image_mime`]; unknown types are served as
/// `application/octet-stream`.
fn doc_mime(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    match ext.as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("md") => "text/markdown; charset=utf-8",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => image_mime(path).unwrap_or("application/octet-stream"),
    }
}

/// GET /{path}
///
/// Serves a static file from the `doc` directory with the appropriate MIME type.
/// Returns a `BAD_REQUEST` response if the path is invalid.
/// Returns a `NOT_FOUND` response if the file does not exist.
/// Returns an `INTERNAL_SERVER_ERROR` response if reading the file fails.
pub async fn path_handler(Path(path): Path<String>) -> Response {
    // Same guard as the other handlers: "..", absolute paths and drive
    // prefixes would escape doc/ and expose any file readable by the process.
    let rel = PathBuf::from(&path);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        log::log!(
            target: "os",
            log::Level::Warn,
            "Path contains any components that are not normal: {}", rel.display(),
        );
        return StatusCode::BAD_REQUEST.into_response();
    }
    let path = PathBuf::from("doc").join(rel);
    let mime = doc_mime(&path);
    serve_file(Some(path), mime).await
}

/// GET /html/{page}/{*html_name}
///
/// Serves an HTML file with the appropriate MIME type.
/// Returns a `BAD_REQUEST` response if the path is invalid.
/// Returns a `NOT_FOUND` response if the file does not exist.
/// Returns an `INTERNAL_SERVER_ERROR` response if reading the file fails.
pub async fn html_handler(Path((page, html_name)): Path<(String, String)>) -> Response {
    serve_file(
        build_path("html", &page, &html_name, "html"),
        "text/html; charset=utf-8",
    )
    .await
}

/// GET /css/{page}/{*css_name}
///
/// Serves a CSS file with the appropriate MIME type.
/// Returns a `BAD_REQUEST` response if the path is invalid.
/// Returns a `NOT_FOUND` response if the file does not exist.
/// Returns an `INTERNAL_SERVER_ERROR` response if reading the file fails.
pub async fn css_handler(Path((page, css_name)): Path<(String, String)>) -> Response {
    serve_file(
        build_path("css", &page, &css_name, "css"),
        "text/css; charset=utf-8",
    )
    .await
}

/// GET /js/{page}/{*js_name}
///
/// Serves a JavaScript file with the appropriate MIME type.
/// Returns a `BAD_REQUEST` response if the path is invalid.
/// Returns a `NOT_FOUND` response if the file does not exist.
/// Returns an `INTERNAL_SERVER_ERROR` response if reading the file fails.
pub async fn js_handler(Path((page, js_name)): Path<(String, String)>) -> Response {
    serve_file(
        build_path("js", &page, &js_name, "js"),
        "text/javascript; charset=utf-8",
    )
    .await
}
/// Determines the MIME type of an image file based on its extension.
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

/// GET /img/{page}/{*img_name}
///
/// Serves an image file with the appropriate MIME type.
/// Returns `BAD_REQUEST` if the path is invalid.
/// Returns `UNSUPPORTED_MEDIA_TYPE` if the file extension is not recognized as an image.
/// Returns `NOT_FOUND` if the file does not exist.
/// Returns `INTERNAL_SERVER_ERROR` if reading the file fails.
pub async fn img_handler(Path((page, img_name)): Path<(String, String)>) -> Response {
    let rel = PathBuf::from(&page).join(&img_name);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        log::log!(
            target: "os",
            log::Level::Warn,
            "Path contains any components that are not normal: {}", rel.display(),
        );
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
        Err(e) => {
            log::log!(
                target: "os",
                log::Level::Error,
                "Error reading file: {}: {}", e, path.display(),
            );
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Converts a temperature from Celsius to Fahrenheit.
fn temp_converter_celsius_to_fahrenheit(celsius: f64) -> f64 {
    celsius * 9.0 / 5.0 + 32.0
}

/// GET /temp_converter?celsius=f64
///
/// Returns the temperature in Fahrenheit as a string if the input is valid.
/// Non-finite input ("NaN", "inf") or a result overflowing to infinity -> 400.
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

/// GET /health
///
/// Returns `200 OK` if the server is healthy.
pub async fn health() -> StatusCode {
    StatusCode::OK
}

/// GET /version
///
/// Returns the version information of the server, including the package version, Rust compiler version, and authors.
pub async fn version() -> String {
    format!(
        "version: {}\nrustc: {}\nauthors: {}\n",
        env!("CARGO_PKG_VERSION"),
        env!("RUSTC_VERSION"),
        env!("CARGO_PKG_AUTHORS").replace(':', ", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests for the `build_path` function.
    #[test]
    fn build_path_adds_default_extension() {
        assert_eq!(
            build_path("css", "main", "main", "css"),
            Some(PathBuf::from("css").join("main").join("main.css"))
        );
    }

    /// Tests for the `build_path` function.
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

    /// Tests for the `build_path` function with nested names.
    #[test]
    fn build_path_allows_nested_names() {
        assert_eq!(
            build_path("css", "main", "sub/dir/style", "css"),
            Some(PathBuf::from("css/main/sub/dir/style.css"))
        );
    }

    /// Tests for the `build_path` function to ensure it rejects directory traversal and absolute paths.
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

    /// Tests for the `image_mime` function with known image types.
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

    /// Tests for the `image_mime` function to ensure it is case-insensitive.
    #[test]
    fn image_mime_is_case_insensitive() {
        assert_eq!(image_mime(std::path::Path::new("A.PNG")), Some("image/png"));
        assert_eq!(
            image_mime(std::path::Path::new("b.JpEg")),
            Some("image/jpeg")
        );
    }

    /// Tests for the `image_mime` function with unknown or missing extensions.
    #[test]
    fn image_mime_unknown_or_missing_extension() {
        assert_eq!(image_mime(std::path::Path::new("a.txt")), None);
        assert_eq!(image_mime(std::path::Path::new("noext")), None);
        assert_eq!(image_mime(std::path::Path::new(".png")), None);
    }

    /// Asserts that two floating-point numbers are approximately equal.
    ///
    /// # Arguments
    ///
    /// * `actual` - The actual floating-point value.
    /// * `expected` - The expected floating-point value.
    ///
    /// # Panics
    ///
    /// Panics if the actual and expected values differ by more than 1e-9.
    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    /// Tests for the `temp_converter_celsius_to_fahrenheit` function with various cases.
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

    /// Helper function to call the `temp_converter` handler with an optional Celsius value.
    ///
    /// # Arguments
    ///
    /// * `celsius` - An optional Celsius value to be converted.
    ///
    /// # Returns
    ///
    /// A `Result` containing the converted Fahrenheit value as a `String` if successful,
    /// or a `StatusCode` indicating the type of error.
    async fn call(celsius: Option<f64>) -> Result<String, StatusCode> {
        let params = celsius
            .map(|v| HashMap::from([("celsius".to_string(), v)]))
            .unwrap_or_default();
        temp_converter(Query(params)).await
    }

    /// Tests for the `temp_converter` handler with various cases.
    #[tokio::test]
    async fn temp_converter_handler() {
        assert_eq!(call(None).await, Ok("32".into()));
        assert_eq!(call(Some(0.0)).await, Ok("32".into()));
        assert_eq!(call(Some(100.0)).await, Ok("212".into()));
        assert_eq!(call(Some(-40.0)).await, Ok("-40".into()));
        assert_close(call(Some(37.0)).await.unwrap().parse().unwrap(), 98.6);
    }

    /// Tests that the `temp_converter` handler rejects non-finite Celsius values.
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

    /// Tests that the `health` handler returns an `OK` status.
    #[tokio::test]
    async fn health_is_ok() {
        assert_eq!(health().await, StatusCode::OK);
    }

    /// Tests that the `version` handler returns a string containing the build information.
    #[tokio::test]
    async fn version_contains_build_info() {
        let v = version().await;
        assert!(
            v.contains(&format!("version: {}\n", env!("CARGO_PKG_VERSION"))),
            "{v}"
        );
        assert!(
            v.contains(&format!("rustc: {}\n", env!("RUSTC_VERSION"))),
            "{v}"
        );
        assert_ne!(env!("RUSTC_VERSION"), "unknown");
        assert!(
            v.contains("authors: Dmitriy Yudin a.k.a HavaniaPrincess\n"),
            "{v}"
        );
    }

    #[tokio::test]
    async fn path_handler_rejects_escaping_paths() {
        let mut cases = vec!["../Cargo.toml", "a/../../Cargo.toml", "/etc/passwd", ".."];
        if cfg!(windows) {
            cases.extend(["C:/Windows/win.ini", r"..\Cargo.toml"]);
        }
        for p in cases {
            let resp = path_handler(Path(p.to_string())).await;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{p}");
        }
    }

    #[tokio::test]
    async fn path_handler_missing_is_404() {
        let resp = path_handler(Path("__no_such_dir/x.html".to_string())).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn doc_mime_types() {
        let cases = [
            ("a.html", "text/html; charset=utf-8"),
            ("a.css", "text/css; charset=utf-8"),
            ("a.js", "text/javascript; charset=utf-8"),
            ("a.json", "application/json"),
            ("a.txt", "text/plain; charset=utf-8"),
            ("a.md", "text/markdown; charset=utf-8"),
            ("a.woff2", "font/woff2"),
            ("a.woff", "font/woff"),
            ("a.ttf", "font/ttf"),
            // images come from image_mime
            ("a.svg", "image/svg+xml"),
            ("a.png", "image/png"),
            ("a.ico", "image/x-icon"),
            // case-insensitive
            ("A.HTML", "text/html; charset=utf-8"),
            ("F.WOFF2", "font/woff2"),
            // unknown or missing extension
            ("search.index", "application/octet-stream"),
            ("noext", "application/octet-stream"),
            (".lock", "application/octet-stream"),
        ];
        for (file, mime) in cases {
            assert_eq!(doc_mime(std::path::Path::new(file)), mime, "{file}");
        }
    }
}
