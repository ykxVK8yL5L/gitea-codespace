#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UPSTREAM_DIR="$SCRIPT_DIR/upstream"
BUILD_DIR="$SCRIPT_DIR/build"
DIST_DIR="$SCRIPT_DIR/dist/github-authentication-env-token"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

require_command node
require_command npm
require_command python3

if [ ! -f "$UPSTREAM_DIR/package.json" ]; then
  echo "vendored upstream source is missing: $UPSTREAM_DIR" >&2
  exit 1
fi

rm -rf "$BUILD_DIR" "$DIST_DIR"
mkdir -p "$BUILD_DIR" "$DIST_DIR"
cp -a "$UPSTREAM_DIR/." "$BUILD_DIR/"

npm --prefix "$BUILD_DIR" install --omit=dev
npm --prefix "$BUILD_DIR" install --no-save esbuild
node "$SCRIPT_DIR/scripts/bundle-node-extension.js" "$BUILD_DIR" "$DIST_DIR"

if [ -f "$SCRIPT_DIR/UPSTREAM_INFO.json" ]; then
  cp "$SCRIPT_DIR/UPSTREAM_INFO.json" "$DIST_DIR/UPSTREAM_INFO.json"
fi

echo "Built $DIST_DIR"
