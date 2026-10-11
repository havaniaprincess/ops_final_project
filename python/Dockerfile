# Образ сервиса документации. Собирать из КОРНЯ репозитория:
#   docker build -f python/Dockerfile -t rust-docs .
# (контексту нужны и Rust-код для cargo doc, и папка python/)

# ---------- 1. Генерация документации Rust-проекта ----------
FROM rust:1-slim-bookworm AS docs
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
RUN cargo doc --workspace --no-deps --all-features --locked

# ---------- 2. Python-сервис ----------
FROM python:3.12-slim
WORKDIR /app

# Сначала только зависимости: этот слой кэшируется, пока requirements.txt не менялся
COPY python/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY python/app.py .
COPY --from=docs /src/target/doc ./doc
ENV DOCS_DIR=/app/doc
# Вывод Python сразу уходит в поток контейнера (docker logs), без буферизации
ENV PYTHONUNBUFFERED=1

# Запускаем не от root
RUN useradd -r -u 10001 appuser
USER appuser

EXPOSE 8000
HEALTHCHECK CMD python -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:8000/health')"
# --no-access-log: запросы логирует само приложение (подробнее, с временем),
# стандартный лог uvicorn дублировал бы каждую строку
CMD ["uvicorn", "app:app", "--host", "0.0.0.0", "--port", "8000", "--no-access-log"]
