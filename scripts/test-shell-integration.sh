#!/usr/bin/env bash
# Checks the real bash wrapper (`cdx-bin init bash`) against a real cdx-bin binary:
# favourites, plain `cd` pass-through, error cases and cdx's own commands.
#
#   cargo build && bash scripts/test-shell-integration.sh target/debug/cdx-bin
#
# Uses a throw-away HOME, so your own favourites and config are never touched.
set -u

BIN="${1:?usage: test-shell-integration.sh path/to/cdx-bin}"
BIN="$(cd "$(dirname "$BIN")" && pwd -P)/$(basename "$BIN")"

work="$(cd "$(mktemp -d)" && pwd -P)" # -P: macOS /tmp is a symlink
trap 'rm -rf "$work"' EXIT

unset CDX_CONFIG_DIR CDPATH
export HOME="$work/home"
mkdir -p "$HOME/.cdx" "$work/with space" "$work/proj" "$work/other"
export PATH="$(dirname "$BIN"):$PATH"

# One favourite with an alias, one without (its folder is called "other").
printf 'work\t%s\n\t%s\n' "$work/proj" "$work/other" > "$HOME/.cdx/favorites.tsv"

eval "$(cdx-bin init bash)"

failures=0
fail() { echo "FAIL: $*"; failures=$((failures + 1)); }
pass() { echo "ok:   $*"; }

# expect_cwd <description> <expected folder> <cdx arguments...>
expect_cwd() {
  local what="$1" want="$2"
  shift 2
  cd "$work" || exit 1
  cdx "$@" >/dev/null 2>&1
  if [ "$(pwd -P)" = "$want" ]; then pass "$what"; else fail "$what (in $(pwd -P), wanted $want)"; fi
}

# expect_stay <description> <cdx arguments...>: must fail and not move.
expect_stay() {
  local what="$1"
  shift
  cd "$work" || exit 1
  if cdx "$@" >/dev/null 2>&1; then fail "$what (succeeded)"; return; fi
  if [ "$(pwd -P)" = "$work" ]; then pass "$what"; else fail "$what (moved to $(pwd -P))"; fi
}

# Favourites win.
expect_cwd "slot number"            "$work/proj"  1
expect_cwd "alias"                  "$work/proj"  work
expect_cwd "alias, other case"      "$work/proj"  WORK
expect_cwd "folder name of an unaliased favourite" "$work/other" other

# Everything else is plain cd.
expect_cwd "absolute path"          "$work/with space" "$work/with space"
expect_cwd "relative path"          "$work/proj" ./proj
expect_cwd "local folder that is not a favourite" "$work/with space" "with space"
expect_cwd "parent folder"          "$work"      proj/..
expect_cwd "home with ~"            "$HOME"      ~

cd "$work/proj" || exit 1
cdx - >/dev/null 2>&1 # no previous folder yet: must not crash the shell
cd "$work/other" && cd "$work/proj"
cdx - >/dev/null 2>&1
if [ "$(pwd -P)" = "$work/other" ]; then pass "cdx - goes back"; else fail "cdx - (in $(pwd -P))"; fi

# Errors come from cd, and the shell stays put.
expect_stay "folder that does not exist" no-such-folder-xyz
expect_stay "slot with no favourite"     5
expect_stay "path that does not exist"   ./nope/nope

# A favourite whose folder was deleted is reported, not passed to cd.
rmdir "$work/other"
expect_stay "deleted favourite" other
mkdir "$work/other"

# cdx's own commands print their output and never move the shell.
cd "$work" || exit 1
out="$(cdx config path 2>&1)"
case "$out" in
  *"/.cdx/config.ini") pass "cdx config path" ;;
  *) fail "cdx config path printed: $out" ;;
esac
out="$(cdx --version 2>&1)"
case "$out" in
  "cdx "*) pass "cdx --version" ;;
  *) fail "cdx --version printed: $out" ;;
esac
if [ "$(pwd -P)" = "$work" ]; then pass "commands do not move the shell"; else fail "commands moved the shell"; fi

# The bare word of a command is never a favourite, even if one is named like it.
printf 'config\t%s\n' "$work/proj" > "$HOME/.cdx/favorites.tsv"
out="$(cdx config path 2>&1)"
case "$out" in
  *"/.cdx/config.ini") pass "a favourite named 'config' does not shadow the command" ;;
  *) fail "reserved name was shadowed: $out" ;;
esac

echo
if [ "$failures" -eq 0 ]; then echo "all shell integration checks passed"; else echo "$failures check(s) failed"; exit 1; fi
