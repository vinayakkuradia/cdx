#!/usr/bin/env sh
# Developer installer: build & install cdx-bin from source, then wire up your shell (bash, zsh, fish).
set -eu

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust toolchain not found. Install it from https://rustup.rs and re-run." >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cargo install --path "$ROOT" --locked

add_line() { # file, line
  mkdir -p "$(dirname "$1")"
  touch "$1"
  if grep -qF "cdx-bin init" "$1"; then
    echo "  already configured: $1"
  else
    printf '\n# cdx\n%s\n' "$2" >> "$1"
    echo "  updated: $1"
  fi
}

case "$(basename "${SHELL:-}")" in
  zsh)  add_line "$HOME/.zshrc" 'eval "$(cdx-bin init zsh)"' ;;
  bash)
    if [ "$(uname -s)" = "Darwin" ]; then rc="$HOME/.bash_profile"; else rc="$HOME/.bashrc"; fi
    add_line "$rc" 'eval "$(cdx-bin init bash)"' ;;
  fish) add_line "$HOME/.config/fish/config.fish" 'cdx-bin init fish | source' ;;
  *)    echo "Unrecognised shell. Add the output of 'cdx-bin init <shell>' to your shell config manually." ;;
esac

echo
echo "Done. Make sure ~/.cargo/bin is on your PATH, then open a new terminal and type: cdx"
