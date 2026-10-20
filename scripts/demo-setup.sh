# Sets up a throw-away home folder for recording the demo (assets/demo.tape).
#
# SOURCE this file, do not run it: it changes the current shell's HOME, prompt and
# folder. It never touches your real home folder, favourites or config.
#
#   source scripts/demo-setup.sh
#
# Needs `cdx-bin` on PATH and bash.

if ! command -v cdx-bin >/dev/null 2>&1; then
  echo "demo-setup: cdx-bin is not on PATH (try: PATH=\"\$PWD/target/release:\$PATH\")" >&2
  return 1 2>/dev/null || exit 1
fi

# A fresh pretend home, so the prompt shows short, tidy paths like ~/dev/web.
export HOME="$(cd "$(mktemp -d)" && pwd -P)"
unset CDX_CONFIG_DIR CDPATH

mkdir -p "$HOME"/dev/{api/src,web/src,docs/guide,cdx/src,scripts} \
         "$HOME"/Documents/notes "$HOME"/Downloads "$HOME"/.cdx
touch "$HOME"/dev/api/{README.md,main.go} "$HOME"/dev/web/{README.md,package.json} \
      "$HOME"/dev/docs/index.md

# One favourite is already there, with an alias: `cdx work` or `cdx 1`.
printf 'work\t%s\n' "$HOME/dev/api" > "$HOME/.cdx/favorites.tsv"

# Orange prompt (matches the logo) that shows where you are.
export PS1='\[\e[38;5;208m\]\w\[\e[0m\] \$ '

eval "$(cdx-bin init bash)"
cd "$HOME" || return 1 2>/dev/null
