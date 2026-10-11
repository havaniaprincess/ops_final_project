# Сервис документации Rust-проекта

Дополнительный сервис к итоговому проекту: отдаёт документацию Rust-сервиса (результат `cargo doc`) как обычный сайт.

## Краткое описание

HTTP-сервер написанный на `Python`, с использованием фреймворка `FastAPI` и сервера `uvicorn`. Файлы документации раздаются встроенным в FastAPI `StaticFiles`, для логирования используется стандартная библиотека `logging`. Кроме документации сервис предоставляет `/health` и несколько endpoint-ов в качестве примеров.

## Сам сервис

Что сервис умеет для пользователя:

- **Документация** Rust-проекта. Страницы, стили, скрипты, шрифты и картинки, которые сгенерировал `cargo doc`.
    - `/` - перенаправляет (307) на главную страницу документации `/ops_app/index.html`
    - все остальные пути - файлы документации, например `/ops_app/fn.app.html`
- **Other Endpoints**
    - `/health` - возвращает `{"status": "ok"}`, или статус 503 `{"status": "no docs"}`, если документация не найдена
    - `/docs` - Swagger самого FastAPI-сервиса, все endpoint-ы можно вызвать из браузера
- **Примеры endpoint-ов** (все под `/api/`, чтобы не пересекаться с файлами документации)
    - `GET /api/echo/{text}?upper=true` - параметр в пути и необязательный query-параметр
    - `GET /api/sum?a=2&b=3.5` - обязательные query-параметры с типами, на не-числа - 422
    - `POST /api/temperature` с телом `{"celsius": 37}` - JSON-тело с проверкой через модель Pydantic, ниже −273.15 - 422
    - `GET /api/pages` - список страниц документации, без документации - 503
    - `GET /api/whoami` - адрес клиента и заголовок `User-Agent`

Что сервис еще умеет внутри:

- `unit-test-ы` - все endpoint-ы покрыты тестами `pytest`, тесты используют свою маленькую документацию и не требуют `cargo doc`
- `logging` - сервис логирует каждый запрос, по какому endpoint-у он пришел, какой ответ получил(статус) и за сколько был выполнен. Логи пишутся в stdout, в Docker это поток контейнера
- `безопасность путей` - запросы вида `/../secret` не выходят за пределы папки с документацией

## Схема архитектуры

Путь запроса: клиент → uvicorn (порт 8000) → FastAPI: сначала маршруты `/`, `/health`, `/api/...`, всё остальное - файлы из папки с документацией. В Docker-образе документация генерируется на этапе сборки и лежит внутри образа в `/app/doc`.

### Паспорт

| Поле |Значение |
|---|---|
| Git repository | https://github.com/havaniaprincess/ops_final_project, ветка `python_touch`, папка `python/` |
| Platform | Сервис кроссплатформенный (Python 3.12). Тесты в CI - на Linux. Docker-образ - Linux. |
| Local URL | http://127.0.0.1:8000 |
| Process manager | `uvicorn` локально или Docker |
| Log | stdout: терминал или `docker logs -f rust-docs` |
| Restart | `Ctrl+C` и повторный запуск или `docker restart rust-docs` |
| CD | нет, сервис запускается вручную |

## Структура проекта

```text
python/
├── app.py                     # сервис: маршруты, раздача документации, логирование
├── test_app.py                # тесты pytest
├── requirements.txt           # зависимости сервиса
├── requirements_dev.txt       # зависимости для тестов и проверок (pytest, httpx2, ruff)
├── .env.example               # шаблон настроек (скопировать в .env)
├── Dockerfile                 # образ: cargo doc + Python-сервис
├── Dockerfile.dockerignore    # что отправляется в Docker при сборке
└── README.md

.github/workflows/
└── ci_python.yaml             # CI: ruff, pytest, сборка и проверка Docker-образа
```

## Окружение

Для запуска сервиса требуется `Python 3.12`, установить можно с [python.org](https://www.python.org/downloads/). Для генерации документации нужен `cargo`: [Rust Book. Installation.](https://doc.rust-lang.org/book/ch01-01-installation.html)

Зависимости указаны в файле `requirements.txt` и ставятся в виртуальное окружение `.venv` в папке `python/`.

## Docs

Документацию, которую раздаёт сервис, генерирует Rust-проект. Из корня репозитория:

```
cargo doc --no-deps
```

Документация появится в `target/doc`, сервис по умолчанию берёт её оттуда. После изменений в Rust-коде команду нужно повторить.

## Запуск сервиса

Все команды - из папки `python/`.

### 1. Виртуальное окружение и зависимости

Linux / macOS:

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements.txt
```

Windows (PowerShell):

```powershell
python -m venv .venv
.venv\Scripts\Activate.ps1
python -m pip install -r requirements.txt
```

Если PowerShell пишет, что выполнение сценариев отключено, окружение можно не активировать, а вызывать Python из него напрямую: `.venv\Scripts\python -m ...`.

После активации в начале строки терминала появится `(.venv)`. Без неё `python` - это системный интерпретатор, и будет ошибка `No module named uvicorn`.

### 2. Настройки

```bash
cp .env.example .env
```

Windows (PowerShell): `Copy-Item .env.example .env`. Если документация лежит в `target/doc`, менять ничего не нужно.

### 3. Запуск

```bash
python -m uvicorn app:app --reload --env-file .env --no-access-log
```

- `--env-file .env` - взять настройки из `.env`;
- `--reload` - перезапускать сервис при изменении `app.py`;
- `--no-access-log` - запросы логирует само приложение, без этого флага каждый запрос попадёт в вывод дважды;
- другой порт - `--port 8080`.

Сервис будет доступен по пути `127.0.0.1:8000`. Остановить - `Ctrl+C`.

## Запуск в Docker

Образ сам генерирует документацию (`cargo doc`) и кладёт её внутрь, на машине нужен только Docker.

Собрать - **из корня репозитория** (образу нужны и Rust-код, и папка `python/`):

```bash
docker build -f python/Dockerfile -t rust-docs .
```

Запустить:

```bash
docker run -d --name rust-docs -p 8000:8000 rust-docs
```

С настройками из `.env`:

```bash
docker run -d --name rust-docs -p 8000:8000 --env-file python/.env rust-docs
```

`DOCS_DIR` в Docker задавать не нужно: образ использует свою папку `/app/doc`.

Проверить:

```bash
docker ps
```

В колонке `STATUS` через несколько секунд появится `(healthy)` - Docker сам опрашивает `/health`.

Логи и остановка:

```bash
docker logs -f rust-docs
```

```bash
docker rm -f rust-docs
```

После изменений в Rust-коде образ нужно пересобрать, чтобы обновилась документация. Если порт 8000 занят, укажите другой слева: `-p 8080:8000`.

## Запуск тестов

Из папки `python/`:

```bash
python -m pip install -r requirements_dev.txt
```

```bash
pytest -v
```

```bash
ruff check .
```

```bash
ruff format --check .
```

## Переменные окружения

Задаются в `.env` (шаблон - `.env.example`) или переменными окружения:

- `DOCS_DIR` папка с результатом `cargo doc`(по умолчанию `../target/doc`, относительный путь считается от папки запуска)
- `CRATE_NAME` имя крейта, в этой папке лежит главная страница(по умолчанию `ops_app`)
- `LOG_LEVEL` уровень логирования(по умолчанию `INFO`): `DEBUG`, `INFO`, `WARNING`, `ERROR`

## Логгирование

Логи сервиса пишутся в stdout в формате:

```
INFO:     Will watch for changes in these directories: ['D:\\https_server\\python']
INFO:     Loading environment from '.env'
INFO:     Uvicorn running on http://127.0.0.1:8000 (Press CTRL+C to quit)
INFO:     Started reloader process [23088] using StatReload
2026-10-11 13:57:12,292 INFO docs: serving docs from D:\https_server\target\doc
INFO:     Started server process [31192]
INFO:     Waiting for application startup.
INFO:     Application startup complete.
2026-10-11 13:57:20,775 INFO docs: 127.0.0.1 "GET /" 307 2.1ms
2026-10-11 13:57:25,941 INFO docs: 127.0.0.1 "GET /ops_app/index.html" 200 3.6ms
2026-10-11 13:57:25,962 INFO docs: 127.0.0.1 "GET /static.files/normalize-9960930a.css" 200 4.0ms
2026-10-11 13:57:25,964 INFO docs: 127.0.0.1 "GET /static.files/rustdoc-0c87338a.css" 200 4.6ms
2026-10-11 13:57:25,968 INFO docs: 127.0.0.1 "GET /crates.js" 200 5.5ms
2026-10-11 13:57:25,969 INFO docs: 127.0.0.1 "GET /static.files/storage-41dd4d93.js" 200 5.3ms
2026-10-11 13:57:25,969 INFO docs: 127.0.0.1 "GET /static.files/main-417ebf0a.js" 200 6.1ms
2026-10-11 13:57:25,973 INFO docs: 127.0.0.1 "GET /static.files/SourceSerif4-Regular-6b053e98.ttf.woff2" 200 3.1ms
2026-10-11 13:57:25,976 INFO docs: 127.0.0.1 "GET /static.files/FiraSans-Italic-81dc35de.woff2" 200 3.5ms
2026-10-11 13:57:25,981 INFO docs: 127.0.0.1 "GET /static.files/FiraSans-Regular-0fe48ade.woff2" 200 4.3ms
```

Уровень зависит от статуса (2xx/3xx → INFO, 4xx/5xx → WARNING). В лог пишется только путь запроса, без query-параметров. Если документация не найдена, при старте будет `WARNING docs: docs not found in ...`.

В Docker логи - это поток контейнера, файлов логов нет. Найти ошибки:

```bash
docker logs rust-docs 2>&1 | grep WARNING
```

HEALTHCHECK опрашивает `/health` каждые 30 секунд, поэтому в логе будут регулярные строки `127.0.0.1 "GET /health" 200`.

## CI on GitHub

GitHub выполняет проверку при push в ветку `python_touch` и в pull request, если менялись `python/`, сам workflow или Rust-код (`src/`, `build.rs`, `Cargo.toml`, `Cargo.lock`) - из него собирается документация в образе. Можно запустить вручную: Actions → CI-python → Run workflow (кнопка появляется, когда workflow есть в ветке `main`).

- `test`:
    - `ruff check`: статический анализ кода, поиск ошибок
    - `ruff format --check`: тест форматирования кода
    - `pytest`: тестирование юнит тестами
- `docker`: сборка Docker-образа, запуск контейнера и проверка `/health` и главной страницы документации. Если проверка упала - выводятся логи контейнера

## Проверка работоспособности

```
curl http://127.0.0.1:8000/health
```

Ответ `{"status":"ok"}` - всё работает. `{"status":"no docs"}` - документация не найдена: выполните `cargo doc --no-deps` или проверьте `DOCS_DIR` в `.env`.

## Безопасность

- запросы не выходят за пределы папки с документацией (`StaticFiles`);
- проверка входных данных моделями Pydantic и типами параметров;
- в Docker сервис работает не от root;
- в лог не попадают query-параметры запросов.
