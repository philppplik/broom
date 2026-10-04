#!/bin/sh
# Broom installer for macOS and Linux:  curl -fsSL https://raw.githubusercontent.com/philppplik/broom/main/scripts/install.sh | sh
set -eu
REPO="philppplik/broom"
OS="$(uname -s)"; ARCH="$(uname -m)"
case "$OS" in
  Darwin) ASSET="broom-macos-universal.tar.gz" ;;
  Linux)
    case "$ARCH" in
      x86_64|amd64) ASSET="broom-linux-x64.tar.gz" ;;
      aarch64|arm64) ASSET="broom-linux-arm64.tar.gz" ;;
      *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac ;;
  *) echo "Unsupported OS: $OS (on Windows use install.ps1)" >&2; exit 1 ;;
esac
DEST="${BROOM_INSTALL_DIR:-$HOME/.local/bin}"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
echo "Downloading $ASSET ..."
curl -fsSL "https://github.com/$REPO/releases/latest/download/$ASSET" -o "$TMP/broom.tgz"
curl -fsSL "https://github.com/$REPO/releases/latest/download/SHA256SUMS.txt" -o "$TMP/SHA256SUMS.txt"
EXPECTED="$(grep " $ASSET\$" "$TMP/SHA256SUMS.txt" | cut -d' ' -f1)"
ACTUAL="$( (sha256sum "$TMP/broom.tgz" 2>/dev/null || shasum -a 256 "$TMP/broom.tgz") | cut -d' ' -f1)"
[ "$EXPECTED" = "$ACTUAL" ] || { echo "Checksum mismatch!" >&2; exit 1; }
tar -xzf "$TMP/broom.tgz" -C "$TMP"
mkdir -p "$DEST" && install -m 755 "$TMP/broom" "$DEST/broom"
echo "Installed to $DEST/broom (checksum verified)"
case ":$PATH:" in *":$DEST:"*) ;; *) echo "Add $DEST to your PATH, e.g.: echo 'export PATH=\"$DEST:\$PATH\"' >> ~/.profile" ;; esac
echo "Run: broom      (system-wide cleaning: sudo broom)"
