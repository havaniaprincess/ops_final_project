use std::path::PathBuf;

use axum::{Router, middleware, routing::get};

use crate::handlers::{
    css_handler, docs_handler, health, html_handler, html_handler_main, img_handler, js_handler,
    path_handler, temp_converter, version,
};

pub mod handlers;
mod logging;

/// Constructs the application router with all routes and middleware.
fn app() -> Router {
    Router::new()
        .route("/", get(html_handler_main))
        .route("/{*path}", get(path_handler))
        .route("/html/{page}/{*html_name}", get(html_handler))
        .route("/css/{page}/{*css_name}", get(css_handler))
        .route("/js/{page}/{*js_name}", get(js_handler))
        .route("/img/{page}/{*img_name}", get(img_handler))
        .route("/temp_converter", get(temp_converter))
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/docs", get(docs_handler))
        .layer(middleware::from_fn(logging::log_requests))
}

/// Application main entry point. Initializes logging and starts the HTTP server.
#[tokio::main]
async fn main() {
    let log_file = std::env::var_os("LOG_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(logging::DEFAULT_LOG_FILE));
    if let Err(e) = logging::init(&log_file) {
        eprintln!("failed to init logging to {}: {e}", log_file.display());
        std::process::exit(1);
    }

    let port = std::env::var("APP_PORT").unwrap_or_else(|_| "3000".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    log::info!("listening on {addr}, logging to {}", log_file.display());
    axum::serve(listener, app()).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::app;
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Sends a GET request to the specified URI and returns the status code, content type, and body.
    ///
    /// # Arguments
    ///
    /// * `uri` - The URI to send the GET request to.
    ///
    /// # Returns
    ///
    /// A tuple containing the status code, an optional content type, and the response body as a byte vector.
    async fn get(uri: &str) -> (StatusCode, Option<String>, Vec<u8>) {
        let resp = app()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let ctype = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_owned());
        let body = resp
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        (status, ctype, body)
    }

    /// Sends a GET request to the specified URI and returns the status code and response body as a string.
    ///
    /// # Arguments
    ///
    /// * `uri` - The URI to send the GET request to.
    ///
    /// # Returns
    ///
    /// A tuple containing the status code and the response body as a string.
    async fn get_text(uri: &str) -> (StatusCode, String) {
        let (status, _, body) = get(uri).await;
        (status, String::from_utf8(body).unwrap())
    }

    /// Tests for the root endpoint to ensure it serves the index HTML file.
    #[tokio::test]
    async fn root_serves_index_html() {
        let (status, ctype, body) = get("/").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ctype.as_deref(), Some("text/html; charset=utf-8"));
        assert_eq!(body, std::fs::read("html/index.html").unwrap());
    }

    /// Tests for the CSS endpoint to ensure it serves CSS files with the correct MIME type.
    #[tokio::test]
    async fn css_served_with_css_mime() {
        let expected = std::fs::read("css/main/main.css").unwrap();
        for uri in ["/css/main/main.css", "/css/main/main"] {
            let (status, ctype, body) = get(uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert_eq!(ctype.as_deref(), Some("text/css; charset=utf-8"), "{uri}");
            assert_eq!(body, expected, "{uri}");
        }
    }

    /// Tests for the JavaScript endpoint to ensure it serves JS files with the correct MIME type.
    #[tokio::test]
    async fn js_served_with_js_mime() {
        let expected = std::fs::read("js/main/main.js").unwrap();
        for uri in ["/js/main/main.js", "/js/main/main"] {
            let (status, ctype, body) = get(uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert_eq!(
                ctype.as_deref(),
                Some("text/javascript; charset=utf-8"),
                "{uri}"
            );
            assert_eq!(body, expected, "{uri}");
        }
    }

    /// Tests that missing static files return a 404 Not Found status.
    #[tokio::test]
    async fn missing_static_files_return_404() {
        for uri in [
            "/html/main/nope",
            "/css/main/nope.css",
            "/js/nope/main.js",
            "/img/main/nope.png",
        ] {
            assert_eq!(get(uri).await.0, StatusCode::NOT_FOUND, "{uri}");
        }
    }

    /// Tests on path traversal to ensure it is rejected with a 400 Bad Request status.
    #[tokio::test]
    async fn path_traversal_is_rejected() {
        for uri in [
            "/css/..%2F..%2Fsrc/main.rs",
            "/css/main/%2E%2E/%2E%2E/Cargo.toml",
            "/js/main/..%2F..%2FCargo.toml",
            "/html/%2E%2E/Cargo.toml",
            "/img/main/%2E%2E/%2E%2E/x.png",
        ] {
            assert_eq!(get(uri).await.0, StatusCode::BAD_REQUEST, "{uri}");
        }
    }

    /// Tests that images with unsupported extensions return a 415 Unsupported Media Type status.
    #[tokio::test]
    async fn img_with_unsupported_extension_is_415() {
        for uri in ["/img/main/file.txt", "/img/main/noext"] {
            assert_eq!(
                get(uri).await.0,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "{uri}"
            );
        }
    }

    /// Tests that unknown routes return a 404 Not Found status.
    #[tokio::test]
    async fn unknown_route_is_404() {
        assert_eq!(get("/does/not/exist").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get("/css/only_page").await.0, StatusCode::NOT_FOUND);
    }

    /// Tests that non-GET methods return a 405 Method Not Allowed status.
    #[tokio::test]
    async fn non_get_method_is_405() {
        let resp = app()
            .oneshot(Request::post("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    /// Sends a GET request to the temperature converter endpoint with the specified Celsius value and returns the Fahrenheit result as a floating-point number.
    ///
    /// # Arguments
    ///
    /// * `celsius` - The Celsius value as a string to be sent as a query parameter.
    ///
    /// # Returns
    ///
    /// The Fahrenheit result as a floating-point number.
    async fn fahrenheit(celsius: &str) -> f64 {
        let (status, body) = get_text(&format!("/temp_converter?celsius={celsius}")).await;
        assert_eq!(status, StatusCode::OK, "celsius={celsius}");
        body.parse().unwrap()
    }

    /// Tests for the temperature converter endpoint to ensure it correctly converts Celsius to Fahrenheit.
    #[tokio::test]
    async fn temp_converter_converts_celsius() {
        let cases = [
            ("0", 32.0),
            ("100", 212.0),
            ("37", 98.6),
            ("36.6", 97.88),
            ("-40", -40.0),
            ("-273.15", -459.67),
            ("1e2", 212.0),
        ];
        for (c, f) in cases {
            let got = fahrenheit(c).await;
            assert!((got - f).abs() < 1e-9, "celsius={c}: {got} != {f}");
        }
    }

    /// Tests that the temperature converter returns an integer result without a fractional part when appropriate.
    #[tokio::test]
    async fn temp_converter_integer_result_has_no_fraction() {
        assert_eq!(
            get_text("/temp_converter?celsius=100").await,
            (StatusCode::OK, "212".into())
        );
    }

    /// Tests that the temperature converter defaults to 0 Celsius when no query parameter is provided.
    #[tokio::test]
    async fn temp_converter_defaults_to_zero() {
        assert_eq!(
            get_text("/temp_converter").await,
            (StatusCode::OK, "32".into())
        );
        // unrelated params are ignored
        assert_eq!(
            get_text("/temp_converter?foo=1").await,
            (StatusCode::OK, "32".into())
        );
    }

    /// Tests that the temperature converter rejects invalid input and returns a 400 Bad Request status.
    #[tokio::test]
    async fn temp_converter_rejects_invalid_input() {
        for uri in [
            "/temp_converter?celsius=abc",
            "/temp_converter?celsius=",
            "/temp_converter?celsius=1,5",
            "/temp_converter?celsius=NaN",
            "/temp_converter?celsius=inf",
            "/temp_converter?celsius=-inf",
            "/temp_converter?celsius=1e308",
        ] {
            assert_eq!(get(uri).await.0, StatusCode::BAD_REQUEST, "{uri}");
        }
    }

    /// Tests that the health endpoint returns a 200 OK status with an empty body.
    #[tokio::test]
    async fn health_returns_200_with_empty_body() {
        let (status, _, body) = get("/health").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.is_empty());
    }

    /// Tests that the version endpoint returns the build information, including the package version, Rust compiler version, and authors.
    #[tokio::test]
    async fn version_returns_build_info() {
        let (status, ctype, body) = get("/version").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ctype.as_deref(), Some("text/plain; charset=utf-8"));
        let body = String::from_utf8(body).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 3, "{body}");
        assert_eq!(lines[0], format!("version: {}", env!("CARGO_PKG_VERSION")));
        assert!(lines[1].starts_with("rustc: rustc "), "{body}");
        assert_eq!(lines[2], "authors: Dmitriy Yudin a.k.a HavaniaPrincess");
    }

    /// Tests that requests are logged with the appropriate log level based on the response status.
    #[tokio::test]
    async fn requests_are_logged_with_level_by_status() {
        let cap = crate::logging::log_capture();

        get("/temp_converter?celsius=12345").await;
        get("/css/main/log_test_missing.css").await;

        let ok = cap.find("/temp_converter?celsius=12345");
        assert_eq!(ok.len(), 1, "{ok:?}");
        let (level, target, msg) = &ok[0];
        assert_eq!(*level, log::Level::Info);
        assert_eq!(target, "http");
        assert!(
            msg.starts_with("- \"GET /temp_converter?celsius=12345\" 200 "),
            "{msg}"
        );
        assert!(msg.ends_with("ms"), "{msg}");

        let missing = cap.find("/css/main/log_test_missing.css");
        assert_eq!(missing.len(), 1, "{missing:?}");
        assert_eq!(missing[0].0, log::Level::Warn);
        assert!(missing[0].2.contains("\" 404 "), "{}", missing[0].2);
    }

    /// Tests that the request log uses the `X-Real-IP` header to determine the client's IP address.
    #[tokio::test]
    async fn request_log_uses_x_real_ip() {
        let cap = crate::logging::log_capture();
        app()
            .oneshot(
                Request::get("/health?log_test_ip")
                    .header("x-real-ip", "203.0.113.7")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let lines = cap.find("/health?log_test_ip");
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0]
                .2
                .starts_with("203.0.113.7 \"GET /health?log_test_ip\" 200 "),
            "{}",
            lines[0].2
        );
    }

    /// Temporary files under `doc/` (the docs dir only exists in the Docker
    /// image, it's gitignored). Each test uses its own dir; removed on drop.
    struct DocFixture(std::path::PathBuf);

    impl DocFixture {
        fn new(name: &str, files: &[(&str, &str)]) -> Self {
            let dir = std::path::PathBuf::from("doc").join(format!("__test_{name}"));
            for (file, content) in files {
                let path = dir.join(file);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, content).unwrap();
            }
            DocFixture(dir)
        }
    }

    impl Drop for DocFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn path_handler_serves_doc_files_with_mime() {
        let _fx = DocFixture::new(
            "mime",
            &[
                ("page.html", "<h1>doc</h1>"),
                ("style.css", "body{}"),
                ("nested/app.js", "let a=1;"),
                ("search.index", "idx"),
                ("static.files/favicon.svg", "<svg/>"),
                ("static.files/font.woff2", "wOF2"),
                ("static.files/LICENSE.txt", "MIT"),
            ],
        );
        let cases = [
            (
                "/__test_mime/page.html",
                "text/html; charset=utf-8",
                "<h1>doc</h1>",
            ),
            (
                "/__test_mime/style.css",
                "text/css; charset=utf-8",
                "body{}",
            ),
            (
                "/__test_mime/nested/app.js",
                "text/javascript; charset=utf-8",
                "let a=1;",
            ),
            (
                "/__test_mime/search.index",
                "application/octet-stream",
                "idx",
            ),
            (
                "/__test_mime/static.files/favicon.svg",
                "image/svg+xml",
                "<svg/>",
            ),
            ("/__test_mime/static.files/font.woff2", "font/woff2", "wOF2"),
            (
                "/__test_mime/static.files/LICENSE.txt",
                "text/plain; charset=utf-8",
                "MIT",
            ),
        ];
        for (uri, mime, body) in cases {
            let (status, ctype, got) = get(uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert_eq!(ctype.as_deref(), Some(mime), "{uri}");
            assert_eq!(got, body.as_bytes(), "{uri}");
        }
    }

    #[tokio::test]
    async fn path_handler_missing_file_is_404() {
        assert_eq!(get("/__test_none/nope.html").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get("/no_such_page").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn path_handler_rejects_traversal() {
        // Without the check these would read files outside doc/
        // (Cargo.toml here; /etc/passwd, .env and so on on the server).
        for uri in [
            "/%2E%2E/Cargo.toml",
            "/..%2FCargo.toml",
            "/static.files/%2E%2E/%2E%2E/Cargo.toml",
            "/%2E%2E/src/main.rs",
        ] {
            assert_eq!(get(uri).await.0, StatusCode::BAD_REQUEST, "{uri}");
        }
    }

    #[tokio::test]
    async fn path_handler_rejects_absolute_paths() {
        // "//etc/passwd" captures "/etc/passwd"; joining an absolute path
        // would replace doc/ entirely.
        for uri in ["//etc/passwd", "//Cargo.toml"] {
            let (status, _, _) = get(uri).await;
            assert_ne!(status, StatusCode::OK, "{uri}");
        }
    }

    #[tokio::test]
    async fn catch_all_does_not_shadow_other_routes() {
        let (status, ctype, _) = get("/css/main/main.css").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ctype.as_deref(), Some("text/css; charset=utf-8"));
        assert_eq!(get("/health").await.0, StatusCode::OK);
        assert_eq!(
            get_text("/temp_converter?celsius=100").await,
            (StatusCode::OK, "212".into())
        );
        let (_, _, body) = get("/").await;
        assert_eq!(body, std::fs::read("html/index.html").unwrap());
    }

    #[tokio::test]
    async fn docs_redirects_to_crate_index() {
        let resp = app()
            .oneshot(Request::get("/docs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            resp.headers().get(header::LOCATION).unwrap(),
            "/ops_app/index.html"
        );
        assert_eq!(crate::handlers::DOCS_INDEX, "/ops_app/index.html");
    }

    #[tokio::test]
    async fn docs_redirect_target_is_served() {
        let index = std::path::PathBuf::from("doc")
            .join("ops_app")
            .join("index.html");

        // Use real docs if they were generated locally, otherwise a fixture.
        let created = !index.exists();
        if created {
            std::fs::create_dir_all(index.parent().unwrap()).unwrap();
            std::fs::write(&index, "<title>docs fixture</title>").unwrap();
        }
        let expected = std::fs::read(&index).unwrap();
        let (status, ctype, body) = get(crate::handlers::DOCS_INDEX).await;
        if created {
            let _ = std::fs::remove_file(&index);
            let _ = std::fs::remove_dir(index.parent().unwrap());
        }

        assert_eq!(status, StatusCode::OK);
        assert_eq!(ctype.as_deref(), Some("text/html; charset=utf-8"));
        assert_eq!(body, expected);
    }
}
