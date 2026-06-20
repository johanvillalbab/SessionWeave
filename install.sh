#!/usr/bin/env bash
#
# SessionWeave installer
# Usage: curl -fsSL ... | bash
#

set -e

REPO="johanvillalba/sessionweave"
BINARY_NAME="sw"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

echo "🚀 Installing SessionWeave (sw)..."

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)  OS="apple-darwin" ;;
  Linux)   OS="unknown-linux-gnu" ;;
  *)       echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64 | amd64) ARCH="x86_64" ;;
  arm64 | aarch64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Get latest release or use main for now
VERSION="latest"

echo "Downloading latest release for ${ARCH}-${OS}..."

# For simplicity in early days we build from source or download prebuilt if available.
# This script prefers cargo if present (best for dev users).

if command -v cargo >/dev/null 2>&1; then
  echo "Found cargo — building from source (recommended)"
  TMPDIR=$(mktemp -d)
  git clone --depth 1 https://github.com/${REPO}.git "$TMPDIR/sessionweave" 2>/dev/null || true
  cd "$TMPDIR/sessionweave"
  cargo build --release --quiet
  sudo mkdir -p "$INSTALL_DIR"
  sudo cp "target/release/${BINARY_NAME}" "$INSTALL_DIR/${BINARY_NAME}"
  cd - >/dev/null
  rm -rf "$TMPDIR"
else
  echo "cargo not found. Please install Rust first: https://rustup.rs"
  echo "Or download a prebuilt binary manually from GitHub releases."
  exit 1
fi

echo "✅ Installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"
echo ""
echo "Run 'sw --help' to get started."
echo "Run 'sw config show' to create your first config."
echo ""
echo "Recommended: install Ollama + nomic-embed-text for full semantic power."
