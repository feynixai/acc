#!/usr/bin/env bash
set -euo pipefail

REPO="feynixai/acc"
INSTALL_DIR="${ACC_INSTALL_DIR:-$HOME/.local/bin}"

info()  { printf "\033[1;34m=>\033[0m %s\n" "$*"; }
error() { printf "\033[1;31merror:\033[0m %s\n" "$*" >&2; exit 1; }

# ── Detect OS and arch ──
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) os="apple-darwin" ;;
  Linux)  os="unknown-linux-gnu" ;;
  *)      error "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64)  arch="x86_64" ;;
  aarch64|arm64) arch="aarch64" ;;
  *)       error "Unsupported architecture: $ARCH" ;;
esac

TARGET="${arch}-${os}"

# ── Check for tmux ──
if ! command -v tmux &>/dev/null; then
  info "tmux not found — installing..."
  if command -v brew &>/dev/null; then
    brew install tmux
  elif command -v apt-get &>/dev/null; then
    sudo apt-get update && sudo apt-get install -y tmux
  elif command -v dnf &>/dev/null; then
    sudo dnf install -y tmux
  elif command -v pacman &>/dev/null; then
    sudo pacman -S --noconfirm tmux
  else
    error "Cannot install tmux automatically. Please install tmux and re-run."
  fi
fi

# ── Try downloading a prebuilt binary ──
RELEASE_URL="https://github.com/${REPO}/releases/latest/download/acc-${TARGET}"

if curl -fsSL --head "$RELEASE_URL" &>/dev/null; then
  info "Downloading prebuilt binary for ${TARGET}..."
  mkdir -p "$INSTALL_DIR"
  curl -fsSL "$RELEASE_URL" -o "${INSTALL_DIR}/acc"
  chmod +x "${INSTALL_DIR}/acc"
else
  # ── Fallback: build from source ──
  info "No prebuilt binary for ${TARGET} — building from source..."

  if ! command -v cargo &>/dev/null; then
    info "Rust not found — installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
  fi

  TMPDIR="$(mktemp -d)"
  trap 'rm -rf "$TMPDIR"' EXIT

  info "Cloning ${REPO}..."
  git clone --depth 1 "https://github.com/${REPO}.git" "$TMPDIR/acc"

  info "Building (release)..."
  cargo build --release --manifest-path "$TMPDIR/acc/Cargo.toml"

  mkdir -p "$INSTALL_DIR"
  cp "$TMPDIR/acc/target/release/acc" "${INSTALL_DIR}/acc"
  chmod +x "${INSTALL_DIR}/acc"
fi

# ── Ensure INSTALL_DIR is in PATH ──
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
  SHELL_NAME="$(basename "$SHELL")"
  case "$SHELL_NAME" in
    zsh)  RC="$HOME/.zshrc" ;;
    bash) RC="$HOME/.bashrc" ;;
    fish) RC="$HOME/.config/fish/config.fish" ;;
    *)    RC="" ;;
  esac

  if [ -n "$RC" ]; then
    if ! grep -q "$INSTALL_DIR" "$RC" 2>/dev/null; then
      echo "export PATH=\"${INSTALL_DIR}:\$PATH\"" >> "$RC"
      info "Added ${INSTALL_DIR} to $RC — restart your shell or run:"
      info "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi
  else
    info "Add ${INSTALL_DIR} to your PATH manually."
  fi
fi

info "Installed acc to ${INSTALL_DIR}/acc"
"${INSTALL_DIR}/acc" --help
