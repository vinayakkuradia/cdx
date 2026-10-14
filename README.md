# cdx

A tiny, fast, cross-platform folder navigator for your terminal.
Type `cdx`, browse, press **Space**, and your shell is now in that folder.

- Windows, macOS, Linux
- Single native binary (~0.5 MB), no runtime, no background service, no GUI
- Hidden folders stay out of the way unless you ask: `cdx -h`
- Up to nine **favourites**, one keypress away: `b` to bookmark the highlighted folder, `1`-`9` to jump
- **Speed dial** from the shell: `cdx 1` or `cdx work` goes straight to a favourite
- **Filter** as you type: press `/`
- `cdx -n` opens the folder in a **new terminal window** instead

## Install

Replace `vinayakkuradia/cdx` with this project's GitHub repository.

**macOS / Linux**
```sh
curl -fsSL https://github.com/vinayakkuradia/cdx/releases/latest/download/install.sh | sh
```

**Windows (PowerShell)**
```powershell
irm https://github.com/vinayakkuradia/cdx/releases/latest/download/install.ps1 | iex
```

Then open a new terminal and type `cdx`.

The installers download a prebuilt binary for your OS and CPU, **verify its SHA-256
checksum**, put it in `~/.local/bin` (macOS/Linux) or `%LOCALAPPDATA%\cdx\bin`
(Windows), add that folder to your `PATH` if needed, and set up your shell.
Nothing needs Rust, admin rights, or `sudo`.

| Setting (environment variable) | Meaning |
|---|---|
| `CDX_VERSION=v0.1.0`        | install a specific release instead of the latest |
| `CDX_INSTALL_DIR=/some/dir` | install somewhere else |
| `CDX_NO_MODIFY_SHELL=1`     | do not edit any shell config or profile |

```sh
curl -fsSL https://github.com/vinayakkuradia/cdx/releases/latest/download/install.sh | CDX_VERSION=v0.1.0 sh
```
```powershell
$env:CDX_VERSION = 'v0.1.0'; irm https://github.com/vinayakkuradia/cdx/releases/latest/download/install.ps1 | iex
```

**Uninstall**
```sh
curl -fsSL https://github.com/vinayakkuradia/cdx/releases/latest/download/install.sh | sh -s -- --uninstall
```
```powershell
$env:CDX_UNINSTALL = '1'; irm https://github.com/vinayakkuradia/cdx/releases/latest/download/install.ps1 | iex
```

Supported platforms: Linux (x86_64, ARM64; static, any distro), macOS (Intel, Apple Silicon),
Windows (x86_64, ARM64). Shells: bash, zsh, fish, PowerShell 5.1 and 7+, cmd.exe.

**Windows note:** if PowerShell's execution policy is `Restricted` (the default on some
Windows editions), profile scripts are blocked. The installer tells you if so; fix with
`Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`.

### Build from source

Needs the [Rust toolchain](https://rustup.rs):

```sh
./scripts/dev-install.sh        # macOS / Linux
```
```powershell
.\scripts\dev-install.ps1       # Windows
```

### Manual shell setup

The installers do this for you. (Markers `# >>> cdx >>>` ... `# <<< cdx <<<` delimit what they add.) To do it by hand, add one line to your shell config:

| Shell      | Add to                              | Line                                                          |
|------------|-------------------------------------|---------------------------------------------------------------|
| bash       | `~/.bashrc` (`~/.bash_profile` on macOS) | `eval "$(cdx-bin init bash)"`                            |
| zsh        | `~/.zshrc`                          | `eval "$(cdx-bin init zsh)"`                                  |
| fish       | `~/.config/fish/config.fish`        | `cdx-bin init fish \| source`                                 |
| PowerShell | `$PROFILE`                          | `Invoke-Expression (& cdx-bin init powershell \| Out-String)` |
| cmd.exe    | any folder on `PATH`                | `cdx-bin init cmd > cdx.cmd`                                  |

## Usage

```
cdx            browse from the current folder
cdx -h         ...with hidden folders shown
cdx <path>     browse starting at <path>
cdx 1          speed dial: cd to favourite #1 (1-9), no browser
cdx work       speed dial by name or alias (case-insensitive)
cdx -n         browse, then open the chosen folder in a new terminal window
cdx -n 2       open favourite #2 in a new terminal window
cdx --help     full help
```

| Key                        | Action                              |
|----------------------------|-------------------------------------|
| ↑ ↓ / `k` `j`              | move                                |
| → / `l` / Enter            | open the highlighted folder         |
| ← / `h` / Backspace        | go up (on Windows, past `C:\` to the drive list) |
| **Space**                  | select the highlighted folder and `cd` there |
| `.` row, then Space        | select the folder you are currently in |
| `.`                        | toggle hidden folders               |
| `~`                        | jump to your home folder            |
| PageUp / PageDown, `g` / `G` | page, top, bottom                 |
| `/`                        | filter the list (see below)         |
| `b`, `1`-`9`, `Tab`        | favourites (see below)              |
| Esc / `q` / Ctrl-C         | cancel, you stay where you were     |

"Hidden" means dot-folders on macOS/Linux and the Hidden attribute on Windows.

## Filtering

Press `/` and start typing: the list narrows to folders whose name contains what you
typed (case-insensitive). The `.` row stays so you can still select the current folder.

| Key (filter box open) | Action |
|-----------------------|--------|
| any character         | add to the filter (`b`, `1`-`9`, `q` etc. are just letters here) |
| Backspace / Ctrl-U    | delete a character / clear the box |
| ↑ ↓ / PageUp / PageDown | move through the matches |
| Esc / Enter           | close the box; **the list stays filtered** |
| Ctrl-C                | quit cdx |

After the box is closed, the normal keys work on the filtered list. `/` edits the filter
again; Esc (with the box closed) clears it, and a second Esc cancels cdx as usual.
Opening another folder, going up, `~` and the `1`-`9` jumps also clear the filter;
toggling hidden folders keeps it.

## Speed dial and new windows

`cdx 1` ... `cdx 9` change to the folder in that favourite slot, without opening the
browser. `cdx <name>` does the same for the favourite whose alias (or folder name, when
it has no alias) matches, ignoring case. Names with a slash, a drive colon or a leading
`.`/`~` are always treated as paths, and a word that is not a favourite still works as a
folder in the current directory. If there is no such favourite, or its folder was
deleted, cdx prints an error and exits with code 2, so your shell stays put.
(`init` is reserved for `cdx init <shell>`.)

`-n` / `--new-window` works with the browser and with speed dial. The chosen folder opens
in a new window and your current shell does not move.

| OS | Opens |
|----|-------|
| macOS | Terminal (or iTerm2 when you are in it) via `open -a` |
| Windows | Windows Terminal (`wt`) if installed, otherwise a `cmd` window |
| Linux/BSD | the first installed of gnome-terminal, konsole, xfce4-terminal, mate-terminal, tilix, terminator, lxterminal, kitty, alacritty, wezterm, foot, x-terminal-emulator, xterm (needs a graphical session) |

## Favourites

Favourites live in a panel on the left, navigation on the right. They are remembered
between sessions and shared by every terminal.

```
Favourites          │ /home/me/projects
3/9                 │ 4 items  ·  hidden: off
  1 Work            │ ▸ .  (this folder)
● 2 projects        │   api/
  3 Downloads       │   web/
```

| Key (navigation mode)  | Action |
|------------------------|--------|
| `b`                    | bookmark the highlighted folder; the `●` marks the favourite you are standing in |
| `1` - `9`              | jump straight to that favourite (you stay in the browser; press Space to select) |
| `Tab`                  | switch to the favourites panel to edit them |

**Editing favourites** (after pressing `Tab`; the cursor is now on the left):

| Key               | Action |
|-------------------|--------|
| `↑` `↓` / `k` `j` | move the cursor |
| `Space`           | **pick up** the item; move it with `↑` `↓`; press `Space` again to put it down |
| `1` - `9`         | while holding an item: send it straight to that slot |
| `r`               | rename: give it an alias (leave empty to go back to the folder's own name) |
| `x` / Delete      | remove |
| `Enter` / `→`     | open it and return to navigation |
| `Tab`             | **finish**; the arrangement is saved exactly as it is |
| `Esc`             | while holding: undo the move. Otherwise: back to navigation |

Notes:

- Slots are the order on screen, so reordering changes which number reaches which folder.
- Adding, removing and renaming save immediately. A reorder is saved when you press `Tab`
  (or leave the browser, however you leave it).
- A favourite whose folder has been deleted is shown in red with `✗`; its key explains
  rather than failing. Remove it with `x`.
- The panel needs a window at least 48 columns wide. Narrower than that it hides itself,
  though the `1`-`9` keys keep working.
- Stored as plain text, one line per favourite (`alias<TAB>path`), at
  `~/.cdx/favorites.tsv` (macOS/Linux) or `%USERPROFILE%\.cdx\favorites.tsv` (Windows).
  Set `CDX_CONFIG_DIR` to use another folder. Safe to edit by hand or keep in your dotfiles.
- Upgrading: if there is no `~/.cdx/favorites.tsv` yet, the list from the old location
  (`~/.config/cdx/` or `%APPDATA%\cdx\`) is read once and saved to the new one. The old
  file is left in place; delete it when you like.

## Permissions

Some folders are protected by the operating system.

- **macOS** guards Desktop, Documents, Downloads and external volumes behind a privacy
  prompt for your terminal app. If you said "Don't Allow" earlier and later open one of those
  folders again, cdx clears that stored answer for the folder and retries once, so macOS
  asks you again. Decline again and you see the usual `Operation not permitted` error.
  (This needs the terminal to be a normal macOS app; other protected places, such as
  `~/Library`, need *Full Disk Access* in System Settings instead.)
- **Linux and Windows** have no such prompt, so a folder you cannot read shows the system's
  error message, as before.

## How it works

A program cannot change its parent shell's working directory; the OS forbids it.
So the project ships two parts:

1. **`cdx-bin`**, the native binary. It draws its UI on **stderr** and prints
   only the chosen path to **stdout**. Cancelling prints nothing and exits with code 0, since it is a normal outcome.
2. **A tiny shell function named `cdx`** (from `cdx-bin init <shell>`) that
   runs the binary and `cd`s to whatever it printed. `cdx init`, `--help`
   and `--version` pass straight through.

If you run `cdx-bin` directly without the wrapper, it still works, but tells you
that the shell integration is not active.

## Project layout

```
src/main.rs      argument parsing, exit codes, printing the result
src/browser.rs   the interactive browser (state, keys, drawing, favourites panel)
src/favorites.rs favourites: data, file format, load/save
src/fsutil.rs    folder listing, hidden detection, Windows drives, paths
src/init.rs      shell wrappers for bash/zsh/fish/PowerShell/cmd
install.sh       user installer for macOS / Linux (shipped with each release)
install.ps1      user installer for Windows (shipped with each release)
scripts/         developer installers that build from source
.cargo/          static C runtime on Windows (no VC++ redistributable needed)
.github/         CI, and the release workflow
```

## Development

```sh
cargo run -- -h          # runs the UI (no cd happens without the wrapper)
cargo test
cargo build --release    # LTO + strip, around 0.5 MB
```

## Releasing

1. Bump `version` in `Cargo.toml`, run `cargo build` (updates `Cargo.lock`), commit and push.
2. Tag and push: `git tag v0.1.1 && git push origin v0.1.1`
3. The **release** workflow builds six targets, packages them with checksums, stamps your
   repository into `install.sh` / `install.ps1`, and publishes a GitHub release.
   It refuses to run if the tag does not match the version in `Cargo.toml`.

Each release contains `cdx-<target>.tar.gz` / `.zip`, a matching `.sha256`, and the two installers.

## Ideas for next steps

- Homebrew tap and Scoop bucket (the release assets are already in the right shape)
- Signed/notarized macOS binary and signed Windows binary, to avoid SmartScreen/Gatekeeper prompts
- A Windows MSI for managed environments
- `cdx -n`: open the selection in a new terminal window instead of `cd`
- Type-to-filter and recent-folder ranking
- Mouse support
