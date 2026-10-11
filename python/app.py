"""Сервис, который отдаёт документацию Rust-проекта (результат `cargo doc`)."""

import logging
import mimetypes
import os
import sys
import time
from pathlib import Path

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field

# Папка с документацией. По умолчанию — target/doc Rust-проекта,
# который лежит на уровень выше (туда её кладёт `cargo doc`).
DEFAULT_DOCS_DIR = Path(__file__).resolve().parent.parent / "target" / "doc"

# rustdoc кладёт главную страницу в папку с именем крейта: doc/ops_app/index.html
CRATE_NAME = os.getenv("CRATE_NAME", "ops_app")

# StaticFiles определяет тип файла по расширению через mimetypes. На Windows
# там нет шрифтов rustdoc (.woff2), без этой строки они уходят как octet-stream.
mimetypes.add_type("font/woff2", ".woff2")

# Логи пишем в stdout: в Docker это и есть поток контейнера, его показывает
# `docker logs`. Уровень — из LOG_LEVEL (DEBUG, INFO, WARNING, ERROR).
logging.basicConfig(
    stream=sys.stdout,
    level=os.getenv("LOG_LEVEL", "INFO").upper(),
    format="%(asctime)s %(levelname)s %(name)s: %(message)s",
)
log = logging.getLogger("docs")


class Temperature(BaseModel):
    """Тело запроса POST /api/temperature."""

    # Ниже абсолютного нуля температуры не бывает — FastAPI сам вернёт 422
    celsius: float = Field(ge=-273.15)


def create_app(docs_dir):
    """Создаёт приложение, которое раздаёт документацию из папки docs_dir."""
    docs_dir = Path(docs_dir)
    app = FastAPI(title="Документация Rust-проекта")

    if (docs_dir / CRATE_NAME / "index.html").exists():
        log.info("serving docs from %s", docs_dir)
    else:
        log.warning("docs not found in %s (/health will return 503)", docs_dir)

    # Одна строка на каждый запрос: клиент, метод, путь, статус, время.
    # Ошибки (4xx/5xx) — уровнем WARNING, чтобы их было легко найти.
    @app.middleware("http")
    async def log_requests(request: Request, call_next):
        start = time.perf_counter()
        response = await call_next(request)
        ms = (time.perf_counter() - start) * 1000
        client = request.client.host if request.client else "-"
        level = logging.WARNING if response.status_code >= 400 else logging.INFO
        log.log(
            level,
            '%s "%s %s" %s %.1fms',
            client,
            request.method,
            request.url.path,
            response.status_code,
            ms,
        )
        return response

    @app.get("/")
    def root():
        # Страницы rustdoc ссылаются на соседей относительными путями
        # (fn.app.html, handlers/index.html), поэтому открывать главную нужно
        # по её настоящему адресу, а не по "/".
        return RedirectResponse(f"/{CRATE_NAME}/index.html")

    @app.get("/health")
    def health():
        if (docs_dir / CRATE_NAME / "index.html").exists():
            return {"status": "ok"}
        return JSONResponse({"status": "no docs"}, status_code=503)

    # ---------- Примеры эндпоинтов ----------
    # Все под /api/, чтобы не пересекаться с файлами документации.

    # 1. Параметр в пути + необязательный query-параметр:
    #    /api/echo/hello?upper=true -> {"text": "HELLO"}
    @app.get("/api/echo/{text}")
    def echo(text: str, upper: bool = False):
        return {"text": text.upper() if upper else text}

    # 2. Обязательные query-параметры с типами. FastAPI сам проверит,
    #    что это числа, и вернёт 422, если нет: /api/sum?a=2&b=3.5 -> {"result": 5.5}
    @app.get("/api/sum")
    def sum_numbers(a: float, b: float):
        return {"result": a + b}

    # 3. POST с JSON-телом, которое проверяется моделью Pydantic:
    #    POST /api/temperature {"celsius": 100} -> {"celsius": 100, "fahrenheit": 212}
    @app.post("/api/temperature")
    def temperature(body: Temperature):
        return {"celsius": body.celsius, "fahrenheit": body.celsius * 9 / 5 + 32}

    # 4. Работа с файлами и своя ошибка через HTTPException:
    #    список страниц документации крейта, 503 если документации нет
    @app.get("/api/pages")
    def pages():
        crate_dir = docs_dir / CRATE_NAME
        if not crate_dir.is_dir():
            raise HTTPException(status_code=503, detail="no docs")
        names = sorted(
            p.relative_to(crate_dir).as_posix() for p in crate_dir.rglob("*.html")
        )
        return {"count": len(names), "pages": names}

    # 5. Данные о самом запросе: заголовки и адрес клиента
    @app.get("/api/whoami")
    def whoami(request: Request):
        return {
            "client": request.client.host if request.client else None,
            "user_agent": request.headers.get("user-agent"),
        }

    # Всё остальное — файлы документации. StaticFiles сам определяет
    # Content-Type и не выпускает запросы вида /../secret за пределы папки.
    # Монтируем последним: маршруты выше проверяются раньше.
    # check_dir=False: приложение запускается и без документации (/health вернёт 503).
    app.mount("/", StaticFiles(directory=docs_dir, check_dir=False), name="docs")

    return app


app = create_app(os.getenv("DOCS_DIR", DEFAULT_DOCS_DIR))
