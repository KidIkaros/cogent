#!/usr/bin/env sh
# Cogent installer — detects OS/arch, downloads the right binary, installs to /usr/local/bin
# Usage: curl -fsSL https://raw.githubusercontent.com/KidIkaros/cogent/master/scripts/install.sh | sh
set -e

REPO="KidIkaros/cogent"
BINARY="cogent"
INSTALL_DIR="/usr/local/bin"

# ── Detect OS and arch ────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)  TARGET="linux-x86_64";  EXT="tar.gz" ;;
      aarch64) TARGET="linux-arm64";   EXT="tar.gz" ;;
      arm64)   TARGET="linux-arm64";   EXT="tar.gz" ;;
      *)       echo "Unsupported Linux arch: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      x86_64)  TARGET="macos-x86_64"; EXT="tar.gz" ;;
      arm64)   TARGET="macos-arm64";  EXT="tar.gz" ;;
      *)       echo "Unsupported macOS arch: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS. On Windows, download from:" >&2
    echo "  https://github.com/$REPO/releases/latest" >&2
    exit 1
    ;;
esac

# ── Resolve latest version ────────────────────────────────────────────────────
if command -v curl >/dev/null 2>&1; then
  FETCH="curl -fsSL"
elif command -v wget >/dev/null 2>&1; then
  FETCH="wget -qO-"
else
  echo "Error: curl or wget is required" >&2
  exit 1
fi

VERSION="$($FETCH "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | sed 's/.*"tag_name": *"\(.*\)".*/\1/' | head -1)"

if [ -z "$VERSION" ]; then
  echo "Error: could not determine latest version" >&2
  exit 1
fi

# ── Download and extract ──────────────────────────────────────────────────────
ARCHIVE="cogent-${TARGET}.${EXT}"
URL="https://github.com/$REPO/releases/download/$VERSION/$ARCHIVE"
TMPDIR="$(mktemp -d)"

echo "Installing cogent $VERSION ($TARGET)..."
$FETCH "$URL" > "$TMPDIR/$ARCHIVE"

tar xzf "$TMPDIR/$ARCHIVE" -C "$TMPDIR"

# ── Install ───────────────────────────────────────────────────────────────────
BIN_SRC="$TMPDIR/cogent-${TARGET}/${BINARY}"

if [ ! -f "$BIN_SRC" ]; then
  echo "Error: binary not found in archive" >&2
  rm -rf "$TMPDIR"
  exit 1
fi

chmod +x "$BIN_SRC"

if [ -w "$INSTALL_DIR" ]; then
  mv "$BIN_SRC" "$INSTALL_DIR/$BINARY"
else
  echo "Installing to $INSTALL_DIR (requires sudo)..."
  sudo mv "$BIN_SRC" "$INSTALL_DIR/$BINARY"
fi

rm -rf "$TMPDIR"

# ── Verify ────────────────────────────────────────────────────────────────────
if command -v cogent >/dev/null 2>&1; then
  echo "✓ Installed: $(cogent --version)"
  echo "  Run: cogent check ."
else
  echo "✓ Installed to $INSTALL_DIR/$BINARY"
  echo "  Make sure $INSTALL_DIR is in your PATH"
fi
