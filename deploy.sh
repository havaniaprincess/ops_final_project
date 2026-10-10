#!/usr/bin/env bash
# Deploys the docker-compose stack (app + nginx + certbot) on the server.
#
# Usage: ./deploy.sh [command] [options]
#
# Commands:
#   up        build the app and start everything; on first run obtains
#             the Let's Encrypt certificate (default)
#   down      stop and remove containers (volumes and certificates are kept)
#   restart   restart all containers without rebuilding
#   status    show containers and check the app health endpoint
#   logs      follow container logs (extra args go to `docker compose logs`)
#   app-logs  follow the app request log file (logs/server.log in the volume)
#
# Options for `up`:
#   --staging     use Let's Encrypt staging (for testing, avoids rate limits;
#                 the browser will not trust the certificate)
#   --force-cert  drop the existing certificate and request a new one
#                 (e.g. to replace a staging certificate with a real one)
#   --no-cache    rebuild the app image without docker layer cache

set -Eeuo pipefail

cd "$(dirname "$(readlink -f "$0")")"

# ---------- output helpers ----------

if [[ -t 1 ]]; then
    C_INFO=$'\e[1;34m' C_OK=$'\e[1;32m' C_WARN=$'\e[1;33m' C_ERR=$'\e[1;31m' C_RESET=$'\e[0m'
else
    C_INFO='' C_OK='' C_WARN='' C_ERR='' C_RESET=''
fi

info() { echo "${C_INFO}==>${C_RESET} $*"; }
ok()   { echo "${C_OK}OK${C_RESET}  $*"; }
warn() { echo "${C_WARN}WARN${C_RESET} $*" >&2; }
die()  { echo "${C_ERR}ERROR${C_RESET} $*" >&2; exit 1; }

trap 'die "command failed (line $LINENO): $BASH_COMMAND"' ERR

# ---------- environment ----------

COMPOSE=()

detect_compose() {
    command -v docker >/dev/null 2>&1 || die "docker is not installed"
    docker info >/dev/null 2>&1 \
        || die "cannot talk to the docker daemon (rootless: 'systemctl --user status docker' and 'docker context use rootless'; rootful: is ${USER:-$(id -un)} in the 'docker' group?)"

    if docker compose version >/dev/null 2>&1; then
        COMPOSE=(docker compose)
    elif command -v docker-compose >/dev/null 2>&1; then
        COMPOSE=(docker-compose)
    else
        die "docker compose plugin is not installed"
    fi
}

compose() { "${COMPOSE[@]}" "$@"; }

load_env() {
    if [[ ! -f .env ]]; then
        cp .env.example .env
        die ".env was missing, created it from .env.example — fill in DOMAIN and EMAIL and run again"
    fi

    set -a
    # shellcheck disable=SC1091
    source .env
    set +a

    : "${APP_NAME:?APP_NAME is not set in .env}"
    : "${DOMAIN:?DOMAIN is not set in .env}"
    : "${EMAIL:?EMAIL is not set in .env}"
    APP_PORT="${APP_PORT:-8080}"

    [[ "$DOMAIN" != "example.com" ]] || die "set your real DOMAIN in .env"
    [[ "$EMAIL" != "admin@example.com" ]] || die "set your real EMAIL in .env"

    # The Dockerfile copies target/release/$APP_NAME, it must be the [[bin]] name.
    local bin_name
    bin_name="$(awk '/^\[\[bin\]\]/{b=1} b && /^name *=/{gsub(/[" ]/,""); sub(/^name=/,""); print; exit}' Cargo.toml)"
    if [[ -n "$bin_name" && "$APP_NAME" != "$bin_name" ]]; then
        die "APP_NAME=$APP_NAME in .env, but the binary in Cargo.toml is '$bin_name'"
    fi
}

# ---------- certificates ----------

CERT_DIR=/etc/letsencrypt/live

# Runs a command inside the certbot image as root: the certificate files
# are owned by root and are not readable from the host as a regular user.
certbot_exec() {
    local entrypoint="$1"; shift
    compose run --rm --no-deps --entrypoint "$entrypoint" certbot "$@"
}

# 0 = a real (not our temporary) certificate for $DOMAIN exists.
has_real_cert() {
    certbot_exec sh -c \
        "test -e '$CERT_DIR/$DOMAIN/fullchain.pem' && test ! -e '$CERT_DIR/$DOMAIN/DUMMY'" \
        >/dev/null 2>&1
}

remove_cert() {
    certbot_exec rm -rf \
        "/etc/letsencrypt/live/$DOMAIN" \
        "/etc/letsencrypt/archive/$DOMAIN" \
        "/etc/letsencrypt/renewal/$DOMAIN.conf"
}

# nginx refuses to start without the files its config points to, while
# certbot needs a running nginx to pass the HTTP challenge. So: put a
# self-signed placeholder in place, start nginx, then get the real one.
create_dummy_cert() {
    info "Creating temporary self-signed certificate for $DOMAIN"
    remove_cert >/dev/null
    certbot_exec sh -c "
        mkdir -p '$CERT_DIR/$DOMAIN' &&
        openssl req -x509 -nodes -newkey rsa:2048 -days 1 \
            -keyout '$CERT_DIR/$DOMAIN/privkey.pem' \
            -out '$CERT_DIR/$DOMAIN/fullchain.pem' \
            -subj '/CN=localhost' 2>/dev/null &&
        touch '$CERT_DIR/$DOMAIN/DUMMY'"
}

request_cert() {
    local staging_arg=()
    [[ "$STAGING" == 1 ]] && staging_arg=(--staging)

    info "Requesting Let's Encrypt certificate for $DOMAIN${staging_arg[*]:+ (staging)}"
    remove_cert >/dev/null

    if ! certbot_exec certbot certonly --webroot -w /var/www/certbot \
            "${staging_arg[@]}" \
            -d "$DOMAIN" \
            --email "$EMAIL" \
            --rsa-key-size 4096 \
            --agree-tos --no-eff-email --non-interactive; then
        warn "certbot failed. Check that DNS for $DOMAIN points to this server"
        warn "and that port ${HTTP_PORT:-80} is reachable from the internet."
        # Put the placeholder back so nginx keeps running.
        create_dummy_cert
        compose exec -T nginx nginx -s reload || true
        die "certificate was not issued"
    fi
}

ensure_cert() {
    if [[ "$FORCE_CERT" == 0 ]] && has_real_cert; then
        ok "certificate for $DOMAIN already exists"
        return
    fi

    create_dummy_cert
    info "Starting nginx with the temporary certificate"
    compose up -d nginx
    request_cert

    info "Reloading nginx with the new certificate"
    compose exec -T nginx nginx -s reload
    ok "certificate for $DOMAIN installed"
}

# ---------- health ----------

# The app image has no curl; check from the nginx container (busybox wget),
# the same path real traffic takes.
wait_for_app() {
    info "Waiting for the app to respond on app:$APP_PORT/health"
    for _ in $(seq 1 30); do
        if compose exec -T nginx wget -q -O /dev/null "http://app:$APP_PORT/health" 2>/dev/null; then
            ok "app is healthy"
            return
        fi
        sleep 1
    done
    compose logs --tail 50 app >&2 || true
    die "app did not become healthy in 30s (logs above)"
}

# ---------- commands ----------

cmd_up() {
    mkdir -p certbot/conf certbot/www

    info "Building app image"
    compose build ${NO_CACHE:+--no-cache} app

    ensure_cert

    info "Starting all services"
    compose up -d --remove-orphans

    wait_for_app
    compose ps
    ok "deployed: https://$DOMAIN"
}

cmd_status() {
    compose ps
    wait_for_app
}

cmd_app_logs() {
    compose exec app sh -c 'tail -n 100 -F "${LOG_FILE:-logs/server.log}"'
}

main() {
    local cmd="up"
    if [[ $# -gt 0 && "$1" != -* ]]; then
        cmd="$1"; shift
    fi

    STAGING=0 FORCE_CERT=0 NO_CACHE=
    local extra=()
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --staging)    STAGING=1 ;;
            --force-cert) FORCE_CERT=1 ;;
            --no-cache)   NO_CACHE=1 ;;
            -h|--help)    sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
            *)            extra+=("$1") ;;
        esac
        shift
    done

    detect_compose
    load_env

    case "$cmd" in
        up)       cmd_up ;;
        down)     compose down ;;
        restart)  compose restart; wait_for_app ;;
        status)   cmd_status ;;
        logs)     compose logs -f --tail 100 "${extra[@]}" ;;
        app-logs) cmd_app_logs ;;
        *)        die "unknown command '$cmd' (see --help)" ;;
    esac
}

main "$@"
