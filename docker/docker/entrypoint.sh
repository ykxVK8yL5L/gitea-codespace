#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="${WORKSPACE_DIR:-/home/coder/project}"
CODE_SERVER_BIND_ADDR="${CODE_SERVER_BIND_ADDR:-0.0.0.0:8080}"

mkdir -p "$PROJECT_DIR"

if [ -n "${WORKSPACE_TOKEN:-}" ] && [ -n "${WORKSPACE_MANAGER_URL:-}" ]; then
  git config --global credential.helper workspace-manager
  git config --global credential.useHttpPath true
  git config --global --add safe.directory "$PROJECT_DIR"

  if [ -n "${GIT_USER_NAME:-}" ]; then
    git config --global user.name "$GIT_USER_NAME"
  fi

  if [ -n "${GIT_USER_EMAIL:-}" ]; then
    git config --global user.email "$GIT_USER_EMAIL"
  fi

  if [ ! -d "$PROJECT_DIR/.git" ] && [ -n "${CLONE_URL:-}" ]; then
    git clone "$CLONE_URL" "$PROJECT_DIR"
  fi
fi

CODE_SERVER_ARGS=(--bind-addr "$CODE_SERVER_BIND_ADDR")
if [ -n "${WORKSPACE_TOKEN:-}" ]; then
  CODE_SERVER_ARGS=(--disable-workspace-trust "${CODE_SERVER_ARGS[@]}")
fi

if [ "${CODE_SERVER_AUTH:-}" = "none" ]; then
  CODE_SERVER_ARGS=(--auth none "${CODE_SERVER_ARGS[@]}")
fi

exec code-server "${CODE_SERVER_ARGS[@]}" "$PROJECT_DIR"
