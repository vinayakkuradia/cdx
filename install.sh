#!/usr/bin/env sh
# cdx installer for macOS and Linux.
#
#   curl -fsSL https://github.com/__REPO__/releases/latest/download/install.sh | sh
#
# Environment variables:
#   CDX_VERSION           release tag to install, e.g. vX.Y.Z   (default: latest)
#   CDX_INSTALL_DIR       where to put cdx-bin                  (default: ~/.local/bin)
#   CDX_NO_MODIFY_SHELL=1 do not touch any shell config file
#   CDX_BASE_URL          override the download location (mirrors, testing)
#   CDX_REPO              GitHub "owner/name" (baked in at release time)
#
# Uninstall:  curl -fsSL <same url> | sh -s -- --uninstall
set -eu

REPO="${CDX_REPO:-__REPO__}"
INSTALL_DIR="${CDX_INSTALL_DIR:-$HOME/.local/bin}"
MARK_BEGIN="# >>> cdx >>>"
MARK_END="# <<< cdx <<<"

say() { printf 'cdx: %s\n' "$*"; }
err() { printf 'cdx: error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

fetch() { # url dest
  if have curl; then curl -fsSL "$1" -o "$2"
  elif have wget; then wget -qO "$2" "$1"
  else err "curl or wget is required"; fi
}

sha256_of() {
  if have sha256sum; then sha256sum "$1" | cut -d' ' -f1
  elif have shasum; then shasum -a 256 "$1" | cut -d' ' -f1
  else err "sha256sum or shasum is required to verify the download"; fi
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux)  os_part="unknown-linux-musl" ;;
    Darwin) os_part="apple-darwin" ;;
    *) err "unsupported OS '$os' (on Windows, use install.ps1)" ;;
  esac
  case "$arch" in
    x86_64|amd64)  arch_part="x86_64" ;;
    aarch64|arm64) arch_part="aarch64" ;;
    *) err "unsupported CPU '$arch'" ;;
  esac
  # A shell running under Rosetta reports x86_64; prefer the native build.
  if [ "$os" = "Darwin" ] && [ "$arch_part" = "x86_64" ]; then
    if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
      arch_part="aarch64"
    fi
  fi
  TARGET="${arch_part}-${os_part}"
}

install_cdx() {
  case "$REPO" in
    ""|__REPO*) err "no repository configured. Use the install.sh attached to a GitHub release, or set CDX_REPO=owner/name" ;;
  esac
  detect_target
  asset="cdx-${TARGET}.tar.gz"

  if [ -n "${CDX_BASE_URL:-}" ]; then
    base="$CDX_BASE_URL"
  elif [ -n "${CDX_VERSION:-}" ]; then
    base="https://github.com/${REPO}/releases/download/${CDX_VERSION}"
  else
    base="https://github.com/${REPO}/releases/latest/download"
  fi

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  say "downloading ${asset}"
  fetch "${base}/${asset}" "${tmp}/${asset}" || err "download failed: ${base}/${asset}"
  fetch "${base}/${asset}.sha256" "${tmp}/${asset}.sha256" || err "checksum download failed"

  want="$(head -n1 "${tmp}/${asset}.sha256" | cut -d' ' -f1)"
  got="$(sha256_of "${tmp}/${asset}")"
  [ "$want" = "$got" ] || err "checksum mismatch (expected ${want}, got ${got}); aborting"
  say "checksum verified"

  tar -xzf "${tmp}/${asset}" -C "$tmp"
  mkdir -p "$INSTALL_DIR"
  # Copy then rename, so replacing a binary that is currently running is safe.
  cp "${tmp}/cdx-${TARGET}/cdx-bin" "${INSTALL_DIR}/cdx-bin.new"
  chmod 755 "${INSTALL_DIR}/cdx-bin.new"
  mv -f "${INSTALL_DIR}/cdx-bin.new" "${INSTALL_DIR}/cdx-bin"
  say "installed ${INSTALL_DIR}/cdx-bin"
}

configure_shell() {
  if [ "${CDX_NO_MODIFY_SHELL:-0}" = "1" ]; then
    say "skipping shell configuration (CDX_NO_MODIFY_SHELL=1)"
    return
  fi

  shell_name="$(basename "${SHELL:-}")"
  fish=0
  case "$shell_name" in
    zsh)  rc="${ZDOTDIR:-$HOME}/.zshrc"; init_shell="zsh" ;;
    bash)
      if [ "$(uname -s)" = "Darwin" ]; then rc="$HOME/.bash_profile"; else rc="$HOME/.bashrc"; fi
      init_shell="bash" ;;
    fish) rc="$HOME/.config/fish/config.fish"; init_shell="fish"; fish=1 ;;
    *)
      say "unrecognised shell '${shell_name}'. Add the output of 'cdx-bin init <shell>' to your shell config manually."
      return ;;
  esac

  mkdir -p "$(dirname "$rc")"
  touch "$rc"
  if grep -qF "$MARK_BEGIN" "$rc"; then
    say "shell already configured: $rc"
    return
  fi

  on_path=0
  case ":${PATH}:" in *":${INSTALL_DIR}:"*) on_path=1 ;; esac

  {
    printf '\n%s\n' "$MARK_BEGIN"
    if [ "$on_path" -eq 0 ]; then
      if [ "$fish" -eq 1 ]; then
        printf 'fish_add_path "%s"\n' "$INSTALL_DIR"
      else
        printf 'export PATH="%s:$PATH"\n' "$INSTALL_DIR"
      fi
    fi
    if [ "$fish" -eq 1 ]; then
      printf 'cdx-bin init fish | source\n'
    else
      printf 'eval "$(cdx-bin init %s)"\n' "$init_shell"
    fi
    printf '%s\n' "$MARK_END"
  } >> "$rc"
  say "updated $rc"
}

uninstall_cdx() {
  rm -f "${INSTALL_DIR}/cdx-bin"
  say "removed ${INSTALL_DIR}/cdx-bin"
  for rc in "${ZDOTDIR:-$HOME}/.zshrc" "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.config/fish/config.fish"; do
    if [ -f "$rc" ] && grep -qF "$MARK_BEGIN" "$rc"; then
      sed "/${MARK_BEGIN}/,/${MARK_END}/d" "$rc" > "${rc}.cdx-tmp"
      cat "${rc}.cdx-tmp" > "$rc"
      rm -f "${rc}.cdx-tmp"
      say "cleaned $rc"
    fi
  done
  say "uninstalled. Open a new terminal to finish."
}

usage() {
  sed -n '2,15p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//' || true
}

case "${1-}" in
  "")          install_cdx
               configure_shell
               echo
               say "done. Open a new terminal and type: cdx" ;;
  --uninstall) uninstall_cdx ;;
  -h|--help)   usage ;;
  *)           err "unknown option '$1' (try --help)" ;;
esac
