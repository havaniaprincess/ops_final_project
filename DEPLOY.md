# Деплой

Сервис запускается на сервере через docker compose: `app` (Rust) + `nginx` (TLS) + `certbot` (сертификаты Let's Encrypt).
Деплой автоматический: push в `main` → CI → CD по SSH → `deploy.sh` на сервере.

```
push в main ──► CI-rust (fmt, clippy, тесты, ...) ──► CD ──ssh──► ~/bin/cd-deploy <sha>
                                                                  ├─ git fetch + проверка, что sha есть в main
                                                                  ├─ git reset --hard <sha>
                                                                  └─ deploy.sh up ──► build, сертификат, compose up, /health
```

| Файл | Что делает |
|---|---|
| `deploy.sh` | поднимает стек из того, что лежит в папке: сборка, сертификат, запуск, проверка `/health` |
| `server/cd-deploy.sh` | обёртка для ключа CD: принимает только SHA коммита из `main`, обновляет код и вызывает `deploy.sh` |
| `.github/workflows/cd.yaml` | GitHub Actions: после зелёного CI заходит на сервер и вызывает обёртку |

Далее везде: сервер `havaniaprincess.space`, пользователь для деплоя `deploy`, проект в `/opt/ops_final_project`.

---

## 1. Подготовка сервера (администратор, с sudo)

### Пользователь deploy

Без пароля (вход только по ключу) и без sudo:

```bash
sudo adduser --disabled-password --gecos "" deploy
```

Папка проекта, сразу принадлежит `deploy`:

```bash
sudo install -d -o deploy -g deploy /opt/ops_final_project
```

Папка для SSH-ключей:

```bash
sudo install -d -m 700 -o deploy -g deploy /home/deploy/.ssh
```

Свой личный публичный ключ (для обслуживания сервера) — **дописать** через `tee -a`, `cp` перезапишет файл:

```bash
echo "ВАШ_ЛИЧНЫЙ_ПУБЛИЧНЫЙ_КЛЮЧ" | sudo tee -a /home/deploy/.ssh/authorized_keys
```

```bash
sudo chown deploy:deploy /home/deploy/.ssh/authorized_keys && sudo chmod 600 /home/deploy/.ssh/authorized_keys
```

> `ls` не показывает `.ssh`, потому что имена с точкой скрыты — смотреть через `ls -la`.
> Права должны быть `drwx------ deploy deploy` на `.ssh` и `-rw------- deploy deploy` на `authorized_keys`,
> иначе sshd молча игнорирует ключи.

### Rootless Docker

Docker-демон работает от имени `deploy`, без root. Группа `docker` не нужна (она равна root-доступу).

```bash
sudo apt-get install -y uidmap docker-ce-rootless-extras
```

Разрешить обычным пользователям слушать порты 80/443:

```bash
echo 'net.ipv4.ip_unprivileged_port_start=80' | sudo tee /etc/sysctl.d/99-rootless-ports.conf && sudo sysctl --system
```

Демон пользователя стартует при загрузке, без входа в систему:

```bash
sudo loginctl enable-linger deploy
```

**Ubuntu 24.04+**: разрешить user namespaces для rootlesskit — файл `/etc/apparmor.d/usr.bin.rootlesskit`:

```
abi <abi/4.0>,
include <tunables/global>

/usr/bin/rootlesskit flags=(unconfined) {
  userns,
  include if exists <local/usr.bin.rootlesskit>
}
```

```bash
sudo systemctl restart apparmor
```

### Запретить вход по паролю

Сначала убедитесь, что сами входите по ключу, и держите открытой вторую сессию.

```bash
printf 'PasswordAuthentication no\nKbdInteractiveAuthentication no\n' | sudo tee /etc/ssh/sshd_config.d/00-no-password.conf
```

```bash
sudo sshd -t && sudo sshd -T | grep -Ei 'passwordauthentication|kbdinteractive'
```

```bash
sudo systemctl reload ssh
```

Имя `00-…` важно: sshd берёт первое найденное значение, а облачные образы кладут `50-cloud-init.conf` с `PasswordAuthentication yes`.

---

## 2. Настройка под deploy

Входить именно по SSH (`ssh deploy@havaniaprincess.space`), не через `sudo -iu deploy` — иначе не работает `systemctl --user`.

### Docker

```bash
dockerd-rootless-setuptool.sh install
```

```bash
systemctl --user enable --now docker
```

Контекст сохраняется в `~/.docker/config.json` и работает и в неинтерактивном SSH (CD), в отличие от `DOCKER_HOST` в `.bashrc`:

```bash
docker context use rootless
```

```bash
docker run --rm hello-world
```

Чтобы в логах приложения был реальный IP клиентов (по умолчанию rootless его подменяет) —
`~/.config/systemd/user/docker.service.d/override.conf`:

```ini
[Service]
Environment="DOCKERD_ROOTLESS_ROOTLESSKIT_NET=slirp4netns"
Environment="DOCKERD_ROOTLESS_ROOTLESSKIT_PORT_DRIVER=slirp4netns"
```

```bash
systemctl --user daemon-reload && systemctl --user restart docker
```

### Доступ к репозиторию (deploy key)

Репозиторий приватный, а CD не может ввести логин. Серверу нужен свой ключ только на чтение:

```bash
ssh-keygen -t ed25519 -N "" -C "deploy@havaniaprincess.space" -f ~/.ssh/github_repo
```

```bash
cat ~/.ssh/github_repo.pub
```

GitHub → репозиторий → **Settings → Deploy keys → Add deploy key**, вставить ключ, **Allow write access — не ставить**.

```bash
printf 'Host github.com\n  IdentityFile ~/.ssh/github_repo\n  IdentitiesOnly yes\n' >> ~/.ssh/config && chmod 600 ~/.ssh/config
```

```bash
ssh-keyscan github.com >> ~/.ssh/known_hosts
```

Сверить отпечаток ED25519 с [официальным](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/githubs-ssh-key-fingerprints)
(`SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU`):

```bash
ssh-keygen -lf ~/.ssh/known_hosts | grep -i github
```

### Проект и первый деплой

```bash
git clone git@github.com:havaniaprincess/ops_final_project.git /opt/ops_final_project
```

Если уже склонирован по https — переключить:

```bash
git -C /opt/ops_final_project remote set-url origin git@github.com:havaniaprincess/ops_final_project.git
```

Все файлы проекта должны принадлежать `deploy`. Проверка (пустой вывод = ок):

```bash
sudo find /opt/ops_final_project ! -user deploy
```

Настройки — `DOMAIN`, `EMAIL`, остальное можно оставить:

```bash
cd /opt/ops_final_project && cp .env.example .env && nano .env
```

Первый раз — с тестовым сертификатом (у staging нет лимитов на выпуск):

```bash
bash deploy.sh up --staging
```

Работает — выпустить настоящий:

```bash
bash deploy.sh up --force-cert
```

Остальные команды: `bash deploy.sh status | restart | down | logs [сервис] | app-logs`, справка — `--help`.

### Обёртка для CD

```bash
install -D -m 755 /opt/ops_final_project/server/cd-deploy.sh ~/bin/cd-deploy
```

Обёртка лежит вне репозитория специально: деплой не должен переписывать выполняющийся скрипт,
а правила доступа определяет сервер, а не код из GitHub. После изменений `server/cd-deploy.sh`
переустанавливать вручную, предварительно посмотрев разницу:

```bash
diff ~/bin/cd-deploy /opt/ops_final_project/server/cd-deploy.sh
```

---

## 3. Ключ для CD (на своей машине, Windows / PowerShell)

Отдельный ключ, **без пароля** (GitHub не сможет его ввести). На вопросы passphrase — дважды Enter.
В PowerShell 5.1 нельзя писать `-N ""` — пустая строка теряется и получается «too many arguments».

```powershell
ssh-keygen -t ed25519 -C github-cd -f $HOME\.ssh\deploy_key
```

Если ключ случайно создан с паролем — убрать (старый пароль, затем дважды Enter):

```powershell
ssh-keygen -p -f $HOME\.ssh\deploy_key
```

> Не создавайте ключ в папке проекта — `deploy_key` не в `.gitignore`, его легко закоммитить.

Скопировать публичную часть:

```powershell
Get-Content $HOME\.ssh\deploy_key.pub | Set-Clipboard
```

На сервере под `deploy` дописать её **с ограничением**: по этому ключу всегда выполняется только обёртка,
без терминала, проброса портов и scp/sftp — откуда бы ни подключались:

```bash
echo 'command="/home/deploy/bin/cd-deploy",restrict ВСТАВИТЬ_deploy_key.pub' >> ~/.ssh/authorized_keys
```

Проверка с Windows, из папки проекта. Без SHA — должно вывести `usage: ...` и отключиться (shell не дают):

```powershell
ssh -i $HOME\.ssh\deploy_key deploy@havaniaprincess.space
```

С SHA — запускает деплой текущего `main`:

```powershell
git fetch origin
```

```powershell
ssh -i $HOME\.ssh\deploy_key deploy@havaniaprincess.space (git rev-parse origin/main)
```

Строку `PTY allocation request failed` можно игнорировать — это работает `restrict`.

---

## 4. GitHub

Репозиторий → **Settings → Environments → New environment** → `production` → **Configure environment**:

1. **Deployment branches and tags** → *Selected branches and tags* → `main`.
2. **Required reviewers** — по желанию: каждый деплой ждёт ручного Approve.
3. **Environment secrets**:

| Имя | Значение |
|---|---|
| `SSH_HOST` | `havaniaprincess.space` |
| `SSH_USER` | `deploy` |
| `SSH_PRIVATE_KEY` | весь файл `deploy_key`, включая строки `-----BEGIN…` / `-----END…` |
| `SSH_KNOWN_HOSTS` | вывод `ssh-keyscan` (см. ниже) |
| `SSH_PORT` | только если SSH не на 22 |

4. **Environment variables**: `DOMAIN` = `havaniaprincess.space` (для проверки `/health` снаружи после деплоя).

Приватный ключ в буфер:

```powershell
Get-Content $HOME\.ssh\deploy_key -Raw | Set-Clipboard
```

`SSH_KNOWN_HOSTS` — запускать на своей машине. Адрес должен **в точности** совпадать с `SSH_HOST`
(домен ↔ домен, IP ↔ IP), для нестандартного порта добавить `-p <порт>`:

```powershell
ssh-keyscan havaniaprincess.space 2>$null | Set-Clipboard
```

Сверить отпечаток (своя машина ↔ сервер, строки ED25519 должны совпасть):

```powershell
ssh-keyscan havaniaprincess.space 2>$null | ssh-keygen -lf -
```

```bash
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

После того как секрет добавлен — удалить приватный ключ локально:

```powershell
Remove-Item $HOME\.ssh\deploy_key
```

Также стоит включить защиту `main` (Settings → Branches): изменения только через PR с обязательным CI.
Кто может пушить в `main`, тот через CD деплоит любой код.

---

## 5. Запуск CD

- Автоматически: push в `main` → после зелёного CI запускается **CD** (вкладка Actions).
  `cd.yaml` должен уже лежать в `main` — иначе `workflow_run` не срабатывает.
- Вручную / откат: **Actions → CD → Run workflow**, в `ref` — ветка, тег или SHA.
  Обёртка пропустит только коммиты из `main`.
- Деплой блокирует падение **любой** джобы CI, включая `coverage` и `audit`.

---

## 6. Смена порта SSH

Уменьшает шум от ботов в логах, но защищает не он, а вход только по ключу. Делать с открытой
второй сессией — ошибка в конфиге или файрволе отрезает доступ к серверу.

1. Открыть новый порт в файрволе — **до** смены. Если включён ufw:

   ```bash
   sudo ufw allow 2222/tcp
   ```

   У облачного провайдера проверьте также файрвол / security group в панели управления.

2. Слушать временно **оба** порта (несколько строк `Port` складываются):

   ```bash
   printf 'Port 22\nPort 2222\n' | sudo tee /etc/ssh/sshd_config.d/10-port.conf
   ```

   ```bash
   sudo sshd -t
   ```

3. Применить. В Ubuntu 22.10+ ssh запускается через systemd socket, и порт берётся при `daemon-reload`:

   ```bash
   systemctl is-active ssh.socket
   ```

   Если `active`:

   ```bash
   sudo systemctl daemon-reload && sudo systemctl restart ssh.socket
   ```

   Если `inactive` (старые версии):

   ```bash
   sudo systemctl restart ssh
   ```

   ```bash
   sudo ss -tlnp | grep -E ':(22|2222)\b'
   ```

4. Проверить вход на новый порт с Windows:

   ```powershell
   ssh -p 2222 havaniaprincess@havaniaprincess.space
   ```

5. Работает — оставить только новый порт и закрыть 22. Затем повторить шаг 3:

   ```bash
   echo 'Port 2222' | sudo tee /etc/ssh/sshd_config.d/10-port.conf
   ```

   Посмотреть, как называется правило для 22 (`OpenSSH` или `22/tcp`), и удалить его:

   ```bash
   sudo ufw status
   ```

   ```bash
   sudo ufw delete allow OpenSSH
   ```

6. Обновить всё, что ходит по SSH:
   - GitHub secret `SSH_PORT` = `2222`;
   - `SSH_KNOWN_HOSTS` — пересканировать с портом, строки станут вида `[havaniaprincess.space]:2222 ...`:

     ```powershell
     ssh-keyscan -p 2222 havaniaprincess.space 2>$null | Set-Clipboard
     ```

   - у себя — `-p 2222` в командах или в `~/.ssh/config`:

     ```
     Host havaniaprincess.space
       Port 2222
     ```

---

## Частые ошибки

| Ошибка | Причина и решение |
|---|---|
| `mkdir: …/.ssh: File exists`, а `ls` её не показывает | Скрытая папка — `ls -la` |
| `cp: cannot create … No such file or directory` | Опечатка в пути (`.shh` вместо `.ssh`) |
| `too many arguments` у `ssh-keygen` в PowerShell | `-N ""` теряется — не указывать `-N`, нажать Enter |
| `Enter passphrase for key` | У ключа CD есть пароль — `ssh-keygen -p -f …`, обновить секрет |
| `Identity file … not accessible`, затем запрос пароля | Неверный путь к ключу в `-i` |
| `fatal: could not read Username for 'https://github.com'` | Приватный репозиторий по https — нужен deploy key, раздел 2 |
| `fatal: detected dubious ownership in repository` | Файлы не принадлежат `deploy` — `sudo chown -R deploy:deploy /opt/ops_final_project` |
| `deploy.sh: Permission denied` | Нет бита исполняемости в git (коммит из Windows). Обёртка запускает через `bash`, вручную — `bash deploy.sh`. Исправить в git: `git update-index --chmod=+x deploy.sh server/cd-deploy.sh` |
| `PTY allocation request failed` | Норма для ключа CD — работает `restrict` |
| `usage: ssh <host> <full 40-char commit sha>` | Обёртке не передан SHA (например, `git rev-parse` запущен вне папки проекта) |
| `the input device is not a TTY` | `docker compose exec` без `-T` в неинтерактивной сессии |
