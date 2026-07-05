#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="${WORKSPACE_DIR:-/home/coder/project}"
CODE_SERVER_BIND_ADDR="${CODE_SERVER_BIND_ADDR:-0.0.0.0:8080}"
GITHUB_AUTHENTICATION_MODE="${GITHUB_AUTHENTICATION_MODE:-official}"
GITHUB_AUTHENTICATION_DIR="/usr/lib/code-server/lib/vscode/extensions/github-authentication"
GITHUB_AUTHENTICATION_OFFICIAL_DIR="/opt/github-authentication/official"
GITHUB_AUTHENTICATION_ENV_TOKEN_DIR="/opt/github-authentication/env-token"

mkdir -p "$PROJECT_DIR"

configure_github_authentication() {
  case "$GITHUB_AUTHENTICATION_MODE" in
    env-token)
      if [ -f "$GITHUB_AUTHENTICATION_ENV_TOKEN_DIR/package.json" ]; then
        echo "GitHub Authentication mode: env-token"
        sudo rm -rf "$GITHUB_AUTHENTICATION_DIR"
        sudo cp -a "$GITHUB_AUTHENTICATION_ENV_TOKEN_DIR" "$GITHUB_AUTHENTICATION_DIR"
      else
        echo "GitHub Authentication env-token build is missing; falling back to official" >&2
      fi
      ;;
    official|"")
      echo "GitHub Authentication mode: official"
      ;;
    *)
      echo "Unknown GITHUB_AUTHENTICATION_MODE='$GITHUB_AUTHENTICATION_MODE'; falling back to official" >&2
      ;;
  esac

  if [ ! -d "$GITHUB_AUTHENTICATION_DIR" ] && [ -d "$GITHUB_AUTHENTICATION_OFFICIAL_DIR" ]; then
    sudo cp -a "$GITHUB_AUTHENTICATION_OFFICIAL_DIR" "$GITHUB_AUTHENTICATION_DIR"
  fi
}

configure_github_authentication

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
