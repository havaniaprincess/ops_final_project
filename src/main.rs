use axum::{Router, routing::get};

use crate::handlers::{css_handler, health, html_handler, html_handler_main, img_handler, js_handler, temp_converter, version};

pub mod handlers;

fn app() -> Router {
    Router::new()
        .route("/", get(html_handler_main))
        .route("/html/{page}/{*html_name}", get(html_handler))
        .route("/css/{page}/{*css_name}", get(css_handler))
        .route("/js/{page}/{*js_name}", get(js_handler))
        .route("/img/{page}/{*img_name}", get(img_handler))
        .route("/temp_converter", get(temp_converter))
        .route("/health", get(health))
        .route("/version", get(version))
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
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
        let body = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, ctype, body)
    }

    async fn get_text(uri: &str) -> (StatusCode, String) {
        let (status, _, body) = get(uri).await;
        (status, String::from_utf8(body).unwrap())
    }

    #[tokio::test]
    async fn root_serves_index_html() {
        let (status, ctype, body) = get("/").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ctype.as_deref(), Some("text/html; charset=utf-8"));
        assert_eq!(body, std::fs::read("html/index.html").unwrap());
    }

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

    #[tokio::test]
    async fn js_served_with_js_mime() {
        let expected = std::fs::read("js/main/main.js").unwrap();
        for uri in ["/js/main/main.js", "/js/main/main"] {
            let (status, ctype, body) = get(uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert_eq!(ctype.as_deref(), Some("text/javascript; charset=utf-8"), "{uri}");
            assert_eq!(body, expected, "{uri}");
        }
    }

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

    #[tokio::test]
    async fn img_with_unsupported_extension_is_415() {
        for uri in ["/img/main/file.txt", "/img/main/noext"] {
            assert_eq!(get(uri).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{uri}");
        }
    }

    #[tokio::test]
    async fn unknown_route_is_404() {
        assert_eq!(get("/does/not/exist").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get("/css/only_page").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn non_get_method_is_405() {
        let resp = app()
            .oneshot(Request::post("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    async fn fahrenheit(celsius: &str) -> f64 {
        let (status, body) = get_text(&format!("/temp_converter?celsius={celsius}")).await;
        assert_eq!(status, StatusCode::OK, "celsius={celsius}");
        body.parse().unwrap()
    }

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

    #[tokio::test]
    async fn temp_converter_integer_result_has_no_fraction() {
        assert_eq!(get_text("/temp_converter?celsius=100").await, (StatusCode::OK, "212".into()));
    }

    #[tokio::test]
    async fn temp_converter_defaults_to_zero() {
        assert_eq!(get_text("/temp_converter").await, (StatusCode::OK, "32".into()));
        // unrelated params are ignored
        assert_eq!(get_text("/temp_converter?foo=1").await, (StatusCode::OK, "32".into()));
    }

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

    #[tokio::test]
    async fn health_returns_200_with_empty_body() {
        let (status, _, body) = get("/health").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn version_returns_package_version() {
        let (status, ctype, body) = get("/version").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ctype.as_deref(), Some("text/plain; charset=utf-8"));
        assert_eq!(String::from_utf8(body).unwrap(), env!("CARGO_PKG_VERSION"));
    }
}
