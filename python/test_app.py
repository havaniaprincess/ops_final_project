import logging

import pytest
from fastapi.testclient import TestClient

from app import CRATE_NAME, create_app


@pytest.fixture
def docs_dir(tmp_path):
    """Маленькая подделка результата `cargo doc` во временной папке."""
    docs = tmp_path / "doc"
    (docs / CRATE_NAME).mkdir(parents=True)
    (docs / CRATE_NAME / "index.html").write_text("<h1>index</h1>", encoding="utf-8")
    (docs / CRATE_NAME / "fn.app.html").write_text("<h1>fn app</h1>", encoding="utf-8")
    (docs / "static.files").mkdir()
    (docs / "static.files" / "rustdoc.css").write_text("body{}", encoding="utf-8")
    (docs / "static.files" / "favicon.svg").write_text("<svg/>", encoding="utf-8")
    (docs / "static.files" / "font.woff2").write_bytes(b"wOF2")
    # Файл рядом с папкой документации — его отдавать нельзя
    (tmp_path / "secret.txt").write_text("secret", encoding="utf-8")
    return docs


@pytest.fixture
def client(docs_dir):
    return TestClient(create_app(docs_dir))


def test_root_redirects_to_crate_index(client):
    response = client.get("/", follow_redirects=False)

    assert response.status_code == 307
    assert response.headers["location"] == f"/{CRATE_NAME}/index.html"


def test_index_page(client):
    response = client.get(f"/{CRATE_NAME}/index.html")

    assert response.status_code == 200
    assert response.headers["content-type"].startswith("text/html")
    assert response.text == "<h1>index</h1>"


def test_relative_link_from_index_works(client):
    # Ссылка fn.app.html со страницы /ops_app/index.html
    response = client.get(f"/{CRATE_NAME}/fn.app.html")

    assert response.status_code == 200
    assert response.text == "<h1>fn app</h1>"


def test_static_files_content_type(client):
    css = client.get("/static.files/rustdoc.css")
    svg = client.get("/static.files/favicon.svg")
    font = client.get("/static.files/font.woff2")

    assert css.status_code == 200
    assert css.headers["content-type"].startswith("text/css")
    assert svg.status_code == 200
    assert svg.headers["content-type"].startswith("image/svg+xml")
    assert font.status_code == 200
    assert font.headers["content-type"] == "font/woff2"


def test_missing_file_is_404(client):
    response = client.get(f"/{CRATE_NAME}/nope.html")

    assert response.status_code == 404


@pytest.mark.parametrize(
    "path", ["/%2E%2E/secret.txt", "/static.files/%2E%2E/%2E%2E/secret.txt"]
)
def test_cannot_escape_docs_dir(client, path):
    response = client.get(path)

    assert response.status_code == 404
    assert "secret" not in response.text


def test_health_ok(client):
    response = client.get("/health")

    assert response.status_code == 200
    assert response.json() == {"status": "ok"}


def test_health_without_docs(tmp_path):
    client = TestClient(create_app(tmp_path / "empty"))

    response = client.get("/health")

    assert response.status_code == 503
    assert response.json() == {"status": "no docs"}


# ---------- Примеры эндпоинтов ----------


def test_echo(client):
    assert client.get("/api/echo/hello").json() == {"text": "hello"}
    assert client.get("/api/echo/hello?upper=true").json() == {"text": "HELLO"}


def test_sum(client):
    response = client.get("/api/sum?a=2&b=3.5")

    assert response.status_code == 200
    assert response.json() == {"result": 5.5}


def test_sum_rejects_non_numbers(client):
    assert client.get("/api/sum?a=two&b=3").status_code == 422
    assert client.get("/api/sum?a=2").status_code == 422


def test_temperature(client):
    response = client.post("/api/temperature", json={"celsius": 100})

    assert response.status_code == 200
    assert response.json() == {"celsius": 100, "fahrenheit": 212}


def test_temperature_validation(client):
    # ниже абсолютного нуля и без обязательного поля
    assert client.post("/api/temperature", json={"celsius": -300}).status_code == 422
    assert client.post("/api/temperature", json={}).status_code == 422


def test_pages(client):
    response = client.get("/api/pages")

    assert response.status_code == 200
    assert response.json() == {"count": 2, "pages": ["fn.app.html", "index.html"]}


def test_pages_without_docs(tmp_path):
    client = TestClient(create_app(tmp_path / "empty"))

    response = client.get("/api/pages")

    assert response.status_code == 503
    assert response.json() == {"detail": "no docs"}


def test_whoami(client):
    response = client.get("/api/whoami", headers={"User-Agent": "pytest"})

    assert response.status_code == 200
    assert response.json()["user_agent"] == "pytest"


# ---------- Логирование ----------


def test_request_is_logged(client, caplog):
    caplog.set_level(logging.INFO, logger="docs")

    client.get("/api/sum?a=1&b=2")

    record = next(r for r in caplog.records if "/api/sum" in r.getMessage())
    assert record.levelno == logging.INFO
    assert '"GET /api/sum" 200 ' in record.getMessage()
    assert record.getMessage().endswith("ms")


def test_error_is_logged_as_warning(client, caplog):
    caplog.set_level(logging.INFO, logger="docs")

    client.get("/no/such/page.html")

    record = next(r for r in caplog.records if "/no/such/page.html" in r.getMessage())
    assert record.levelno == logging.WARNING
    assert '"GET /no/such/page.html" 404 ' in record.getMessage()


def test_missing_docs_logged_on_start(tmp_path, caplog):
    caplog.set_level(logging.INFO, logger="docs")

    create_app(tmp_path / "empty")

    assert any("docs not found" in r.getMessage() for r in caplog.records)
