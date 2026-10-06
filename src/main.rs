use axum::{Router, routing::get};

use crate::handlers::{css_handler, html_handler, html_handler_main, img_handler, js_handler, temp_converter};

pub mod handlers;

#[tokio::main]
async fn main() {
     let app = Router::new()
        .route("/", get(html_handler_main))
        .route("/html/{page}/{*html_name}", get(html_handler))
        .route("/css/{page}/{*css_name}", get(css_handler))
        .route("/js/{page}/{*js_name}", get(js_handler))
        .route("/img/{page}/{*img_name}", get(img_handler))
        .route("/temp_converter", get(temp_converter));

    let listener = tokio::net::TcpListener::bind("localhost:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
