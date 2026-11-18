#!/bin/sh
# rustidian installer — downloads the latest release binary and (optionally) wires MCP clients.
#   curl -fsSL https://raw.githubusercontent.com/lurexh/rustidian/main/install.sh | sh
#   ...or with a vault: curl -fsSL ... | sh -s -- /path/to/vault
set -eu

REPO="lurexh/rustidian"
BIN="rustidian"

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64)  TARGET="aarch64-apple-darwin" ;;
  Darwin/*)      TARGET="x86_64-apple-darwin" ;;
  Linux/arm64|Linux/aarch64) TARGET="aarch64-unknown-linux-musl" ;;
  Linux/*)       TARGET="x86_64-unknown-linux-musl" ;;
  MINGW*|MSYS*|CYGWIN*) TARGET="x86_64-pc-windows-msvc" ;;
  *) echo "unsupported platform: $(uname -s)/$(uname -m)" >&2; exit 1 ;;
esac

URL="https://github.com/$REPO/releases/latest/download/$BIN-$TARGET.tar.gz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "downloading $URL"
if command -v curl >/dev/null 2>&1; then
  curl -fL "$URL" -o "$TMP/$BIN.tar.gz"
else
  wget -qO "$TMP/$BIN.tar.gz" "$URL"
fi
tar xzf "$TMP/$BIN.tar.gz" -C "$TMP"

DEST="$HOME/.local/bin"
mkdir -p "$DEST"
mv "$TMP/$BIN" "$DEST/$BIN"
chmod +x "$DEST/$BIN"
echo "installed: $DEST/$BIN"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "note: add $DEST to your PATH" ;;
esac

if [ "$#" -ge 1 ]; then
  "$DEST/$BIN" install "$@"
else
  echo "next step: $BIN install /path/to/your/vault"
fi
