# Contributing to cdx

Thanks for your interest! cdx is a small tool and is meant to stay small, fast and
dependable. Bug reports, ideas, documentation fixes and code are all welcome.

By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report a bug** or **suggest a feature** with the issue templates. For a bug, the
  output of `cdx-bin --version` and `cdx-bin config check` helps a lot.
- **Security problems:** do not open an issue; see [SECURITY.md](SECURITY.md).
- **Try it on your setup.** Platforms and terminals we cannot test ourselves
  (Windows ARM64, unusual shells, remote sessions) benefit most from reports.
- **Send a pull request.** For anything bigger than a small fix, please open an issue
  first so we can agree on the approach before you spend time on it.

## Setting up to build

You need the Rust toolchain **and** your operating system's C build tools, because
Rust links through them. Without them `cargo build` fails with a message about a
missing linker.

| System | What to install |
|---|---|
| **Windows** | [Build Tools for Visual Studio](https://visualstudio.microsoft.com/downloads/) with the **C++ (MSVC)** workload |
| **macOS** | Command Line Tools for Xcode: `xcode-select --install` |
| **Linux (Ubuntu / Debian)** | `sudo apt install build-essential` |

Then install Rust with [rustup](https://rustup.rs) (stable channel is fine) and check:

```sh
rustc --version
cargo --version
```

Fetch and build:

```sh
git clone https://github.com/vinayakkuradia/cdx.git
cd cdx
cargo build
```

To try your build in your own shell (builds, installs and wires up the wrapper), use
`scripts/dev-install.sh` (macOS / Linux) or `scripts/dev-install.ps1` (Windows).
To just run the browser without the wrapper (no folder change happens):

```sh
cargo run -- -b        # browse from the current folder
```

## Before you open a pull request

Run all of these; CI runs the same ones on Linux, macOS and Windows:

```sh
cargo fmt --all
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items
cargo build --locked && bash scripts/test-shell-integration.sh target/debug/cdx-bin   # macOS / Linux
```

## Recording the demo

The animated demo is scripted with [VHS](https://github.com/charmbracelet/vhs), so it can
be re-recorded whenever the interface changes. VHS needs `ttyd` and `ffmpeg` (see its
install page); then, from the repository root:

```sh
cargo build --release
PATH="$PWD/target/release:$PATH" vhs assets/demo.tape    # writes assets/demo.gif
```

`assets/demo.tape` is the script; `scripts/demo-setup.sh` builds a throw-away home folder
with sample projects for it to browse, so your own favourites and config are never used.
Keep the demo short (about 25 seconds) and update it when you change a key or a feature
it shows.

## Code guidelines

- **Document everything.** Every function, struct, enum, variant, field, constant and
  `static` gets a `///` comment saying what it is for (not a restatement of its name).
- **Test behaviour.** New behaviour comes with tests. The browser can be tested without
  a terminal by sending keys to `State::on_key` and checking its state (see the tests
  at the bottom of `src/browser.rs`).
- **Keep it cross-platform.** OS-specific code goes behind `cfg` with a fallback for
  every other platform. Paths are `Path`/`PathBuf`, never strings with `/`.
- **Keep it small.** Think hard before adding a dependency; the binary is about 0.5 MB.
- **Never break plain `cd`.** `cdx ..`, `cdx -`, `cdx ~/x` and friends must keep working.
- **stdout is for the shell wrapper only.** The interface draws on stderr, and `main`
  prints at most one line on stdout: a folder, or the `cdx:use-cd` marker.
- **Errors never panic** for things users can cause (bad config lines, missing folders).
  They become a warning or a one-line notice.

## How the code is organised

| File | Role |
|---|---|
| `src/main.rs` | glue: what each kind of request does, printing the result |
| `src/cli.rs` | reads the command line; decides what is cdx's and what is `cd`'s |
| `src/commands.rs` | cdx's own commands (`init`, `config`) and the reserved-name list |
| `src/browser.rs` | the interactive browser: state, keys, drawing, favourites panel |
| `src/config.rs` | settings, key bindings, `config.ini` parsing, data-folder paths |
| `src/favorites.rs` | favourites: data, file format, load and save |
| `src/fsutil.rs` | folder listing, hidden detection, Windows drives, paths |
| `src/newwin.rs` | `cdx -n`: opening a new terminal window |
| `src/init.rs` | the shell wrappers printed by `cdx init` |

### Recipes

**Add a command** (like `config`): add one row to `COMMANDS` in `src/commands.rs`
(name, usage, summary, function). That reserves the name for favourites, lists it in
`--help` and makes every shell wrapper pass it through. Add tests and a README line.

**Add a key action:** in `src/config.rs` add the `Action` variant, its `name()` and
`description()`, put it in `Action::ALL` and in `BROWSE_ACTIONS` or `FAVORITES_ACTIONS`,
give it a default key in `browse_defaults()` / `favorites_defaults()`; then handle it in
`do_nav` / `do_favs` in `src/browser.rs`. The built-in tests check there are no key
conflicts. Update the key tables in the README.

**Add a setting:** add the field (with its default and a doc comment) to `Ui` or
`Config`, write it in `to_ini`, read it in `parse_into`, and add it to the README
example. The round-trip test fails until all three agree.

## Pull request etiquette

- One topic per pull request, with a clear title and a description of *why*.
- Add a line under **Unreleased** in [CHANGELOG.md](CHANGELOG.md).
- Update the README and `--help` when behaviour, keys, commands or settings change.
- Be patient and kind: reviews are done by volunteers.

## Releases (maintainers)

1. Move the **Unreleased** notes in `CHANGELOG.md` under a new version heading.
2. Bump `version` in `Cargo.toml`, run `cargo build` to update `Cargo.lock`, commit, push.
3. Tag with the same number: `git tag vX.Y.Z && git push origin vX.Y.Z`. The release
   workflow builds six targets, publishes the GitHub release and renders the
   Homebrew, Scoop and winget manifests (downloadable from the workflow run).
4. Update the package-manager repositories; see [packaging/README.md](packaging/README.md).

## Licence of contributions

Unless you state otherwise, any contribution you submit is licensed under the same
terms as the project: **MIT OR Apache-2.0**, at the user's option, without additional
terms or conditions.
