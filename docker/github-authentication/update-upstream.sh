#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_URL="${VSCODE_REPO_URL:-https://github.com/microsoft/vscode.git}"
REF="${VSCODE_REF:-main}"
UPSTREAM_DIR="$SCRIPT_DIR/upstream"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

require_command git

mkdir -p "$UPSTREAM_DIR"

git clone --depth 1 --filter=blob:none --sparse --branch "$REF" "$REPO_URL" "$TMP_DIR/vscode"
git -C "$TMP_DIR/vscode" sparse-checkout set extensions/github-authentication

rm -rf "$UPSTREAM_DIR"
mkdir -p "$UPSTREAM_DIR"
cp -a "$TMP_DIR/vscode/extensions/github-authentication/." "$UPSTREAM_DIR/"

git -C "$TMP_DIR/vscode" rev-parse HEAD > "$SCRIPT_DIR/VSCODE_COMMIT"
cat > "$SCRIPT_DIR/UPSTREAM_INFO.json" <<EOF
{
  "repo": "$REPO_URL",
  "ref": "$REF",
  "commit": "$(cat "$SCRIPT_DIR/VSCODE_COMMIT")",
  "updatedAt": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

echo "Updated github-authentication upstream to $(cat "$SCRIPT_DIR/VSCODE_COMMIT")"
