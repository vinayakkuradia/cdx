#!/usr/bin/env sh
# Fill the package-manager templates in packaging/ with a release's version and checksums.
#
#   sh scripts/render-packaging.sh vX.Y.Z DIR_WITH_SHA256_FILES OUT_DIR
#
# DIR_WITH_SHA256_FILES holds the release's `cdx-<target>.tar.gz.sha256` / `.zip.sha256`
# files (the release workflow runs this right after collecting them). The repository
# defaults to the maintainer's; set GITHUB_REPOSITORY=owner/name to override.
set -eu

TAG="${1:?usage: render-packaging.sh vX.Y.Z DIR_WITH_SHA256_FILES OUT_DIR}"
SHADIR="${2:?usage: render-packaging.sh vX.Y.Z DIR_WITH_SHA256_FILES OUT_DIR}"
OUT="${3:?usage: render-packaging.sh vX.Y.Z DIR_WITH_SHA256_FILES OUT_DIR}"
REPO="${GITHUB_REPOSITORY:-vinayakkuradia/cdx}"
VERSION="${TAG#v}"
DATE="$(date -u +%Y-%m-%d)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

script="$(mktemp)"
trap 'rm -f "$script"' EXIT
{
  echo "s|{{TAG}}|$TAG|g"
  echo "s|{{VERSION}}|$VERSION|g"
  echo "s|{{REPO}}|$REPO|g"
  echo "s|{{DATE}}|$DATE|g"
} > "$script"

for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
              x86_64-apple-darwin aarch64-apple-darwin \
              x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do
  file="$(ls "$SHADIR"/cdx-"$target".tar.gz.sha256 "$SHADIR"/cdx-"$target".zip.sha256 2>/dev/null | head -n 1 || true)"
  [ -n "$file" ] || { echo "missing checksum file for $target in $SHADIR" >&2; exit 1; }
  sha="$(awk '{print $1}' "$file")"
  [ -n "$sha" ] || { echo "empty checksum in $file" >&2; exit 1; }
  key="$(printf '%s' "$target" | tr 'a-z-' 'A-Z_')"
  upper="$(printf '%s' "$sha" | tr 'a-f' 'A-F')"
  echo "s|{{SHA_$key}}|$sha|g" >> "$script"
  echo "s|{{SHA_UPPER_$key}}|$upper|g" >> "$script"
done

find "$ROOT/packaging" -name '*.tmpl' | while read -r tmpl; do
  rel="${tmpl#"$ROOT"/packaging/}"
  dest="$OUT/${rel%.tmpl}"
  mkdir -p "$(dirname "$dest")"
  sed -f "$script" "$tmpl" > "$dest"
  if grep -q '{{' "$dest"; then
    echo "unfilled placeholder left in $dest" >&2
    grep -n '{{' "$dest" >&2
    exit 1
  fi
  echo "wrote $dest"
done
