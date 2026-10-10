#!/usr/bin/env bash
# Forced command for the CD ssh key. Install on the server as the deploy user:
#
#   install -D -m 755 server/cd-deploy.sh ~/bin/cd-deploy
#
# and in ~/.ssh/authorized_keys:
#
#   command="/home/deploy/bin/cd-deploy",restrict ssh-ed25519 AAAA... github-cd
#
# Whatever the client asks to run arrives in SSH_ORIGINAL_COMMAND; the only
# accepted value is a full commit SHA that is part of origin/main. So the key
# can redeploy any main commit (incl. rollback) and nothing else — no shell.
#
# Lives outside the repo on purpose: deploying must not rewrite the script
# that is doing the deploy.

set -euo pipefail

DEPLOY_PATH="${DEPLOY_PATH:-/opt/ops_final_project}"
BRANCH=main

sha="${SSH_ORIGINAL_COMMAND:-}"
if [[ ! "$sha" =~ ^[0-9a-f]{40}$ ]]; then
    echo "usage: ssh <host> <full 40-char commit sha>" >&2
    exit 2
fi

# One deploy at a time, even if someone runs it by hand alongside CD.
exec 9>"$HOME/.cd-deploy.lock"
flock -w 600 9 || { echo "another deploy is still running" >&2; exit 1; }

cd "$DEPLOY_PATH"
git fetch --prune origin
git fetch origin "$sha"

if ! git merge-base --is-ancestor "$sha" "origin/$BRANCH"; then
    echo "$sha is not in origin/$BRANCH, refusing to deploy" >&2
    exit 1
fi

git reset --hard "$sha"
echo "deploying $(git log -1 --format='%h %s')"
exec ./deploy.sh up
