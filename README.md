# cdx

A tiny, fast, cross-platform folder navigator for your terminal, and a drop-in
extension of `cd`. Type `cdx`, browse, press **Space**, and your shell is now in that
folder. Type `cdx ..` or `cdx -` and it is just `cd`.

- Windows, macOS, Linux
- Single native binary (~0.5 MB), no runtime, no background service, no GUI
- Hidden folders stay out of the way unless you ask: `cdx -h` (or the `.` key)
- Files can be shown with `f`; cdx cannot enter a file, so choosing one takes you to its folder
- Up to nine **favourites**, one keypress away: `b` to bookmark the highlighted folder, `1`-`9` to jump
- **Speed dial** from the shell: `cdx 1` or `cdx work` goes straight to a favourite
- **Still `cd`**: anything that is not cdx's own is handed to your shell's `cd` unchanged
- **Filter** as you type: press `0`
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
| `CDX_VERSION=vX.Y.Z`        | install a specific release (a tag from the Releases page) instead of the latest |
| `CDX_INSTALL_DIR=/some/dir` | install somewhere else |
| `CDX_NO_MODIFY_SHELL=1`     | do not edit any shell config or profile |

```sh
curl -fsSL https://github.com/vinayakkuradia/cdx/releases/latest/download/install.sh | CDX_VERSION=vX.Y.Z sh
```
```powershell
$env:CDX_VERSION = 'vX.Y.Z'; irm https://github.com/vinayakkuradia/cdx/releases/latest/download/install.ps1 | iex
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

The bash, zsh, fish and PowerShell lines run `cdx-bin init` every time a shell starts, so they
pick up new versions by themselves. `cdx.cmd` is a saved file: regenerate it after upgrading
(`cdx-bin init cmd > cdx.cmd`; the Windows installer does this for you).

## Usage

```
cdx              open the browser (plain `cd` would go home; this is cdx's job)
cdx 1            go to favourite #1 (1-9)
cdx work         go to the favourite named or aliased "work" (case-insensitive)
cdx ..  cdx -    anything else is plain `cd`: cdx .., cdx -, cdx ~/src, cdx -L link
cdx -b [path]    open the browser, starting at path
cdx -h           browser with hidden folders shown
cdx -n           browse, then open the chosen folder in a new terminal window
cdx -n 2         open favourite #2 in a new terminal window
cdx config ...   cdx's own commands (see Configuration)
cdx --help       full help
```

## Working as `cd`

With the shell integration installed, `cdx` can stand in for `cd`. It decides in this order:

1. **No arguments**: the browser. (This is the one place cdx differs from `cd`, which
   would go home. Use `cdx ~` for that.)
2. **cdx's own words and options**: `init`, `config`, `-b`, `-h`, `-n`, `--help`, `-V`.
3. **One bare word or digit**: if it names a favourite (a slot number, an alias, or the
   folder name of an unaliased favourite), cdx goes there. **Favourites win** over a
   folder of the same name in the current directory; write `cdx ./name` for the folder.
4. **Everything else goes to `cd`, exactly as typed**: `cdx ..`, `cdx -`, `cdx ~/src`,
   `cdx /some/dir`, `cdx -L link`, `cdx -- -odd-name`, `cdx some-local-folder`, and
   errors such as "no such file or directory" come from `cd` itself, and so do
   `CDPATH`, `cd -`, and any `cd` function or hook you have defined.

Mixing cdx's options with `cd`'s (`cdx -n -L x`) is an error, because nothing is passed
on once a cdx option is used. Opening the browser at a folder used to be `cdx <path>`;
it is now `cdx -b <path>`.

Without the shell integration `cdx-bin` cannot run `cd` for you; it says so instead.

**Reserved words.** Every cdx command name is reserved (today `init` and `config`),
case-insensitively. A favourite cannot be given such a name as alias, and a folder with
that name cannot be bookmarked (cdx tells you why). Otherwise `cdx config` could never
reach that favourite. A favourite that already has such a name, e.g. from a hand-edited
`favorites.tsv`, still works by number and cdx points it out when the browser opens.
New commands are added in one table (`src/commands.rs`); adding a row reserves the name
and teaches the shell wrappers to pass it through, with no other change.

| Default key                | Action                              |
|----------------------------|-------------------------------------|
| ↑ ↓ / `k` `j`              | move                                |
| → / `l` / Enter            | open the highlighted folder         |
| ← / `h` / Backspace        | go up (on Windows, past `C:\` to the drive list) |
| **Space**                  | select the highlighted folder and `cd` there |
| `.` row, then Space        | select the folder you are currently in |
| `.`                        | toggle hidden folders and files     |
| `f`                        | show / hide files (see below)       |
| `~`                        | jump to your home folder            |
| PageUp / PageDown, `g` / `G` | page, top, bottom                 |
| `0`                        | filter the list (see below)         |
| `b`, `1`-`9`, `Tab`        | favourites (see below)              |
| Esc / `q` / Ctrl-C         | cancel, you stay where you were     |

"Hidden" means names starting with a dot on every system, plus, on Windows, anything with
the Hidden attribute. (The dot rule on Windows can be switched off in `config.ini` with
`windows_dot_is_hidden = false`; it is a setting only, there is no key for it.)

## Files

Only folders are listed at first. Press `f` to list files too (or set `show_files = true`
in `config.ini`): folders come first, then files, dimmed, and the `.` key hides or shows
hidden files along with hidden folders.

cdx cannot enter a file or work on one, so wherever it would act on a file it uses the
folder the file is in:

- **Space** (select) or **Enter / →** (open) on a file `cd`s to the folder it is in.
- `b` on a file bookmarks that folder.
- With `-n`, the folder opens in a new window.

Every key above can be rebound (see [Configuration](#configuration)). Only `1`-`9`
(favourites) and Ctrl-C (quit) are fixed.

## Filtering

Press `0` and start typing: the list narrows to the folders (and files, when shown) whose
name contains what you typed (case-insensitive by default). The `.` row stays so you can still select the current folder.

| Key (filter box open) | Action |
|-----------------------|--------|
| any character         | add to the filter (`b`, `1`-`9`, `q` etc. are just letters here) |
| Backspace / Ctrl-U    | delete a character / clear the box |
| ↑ ↓ / PageUp / PageDown | move through the matches |
| Esc / Enter           | close the box; **the list stays filtered** |
| Ctrl-C                | quit cdx |

After the box is closed, the normal keys work on the filtered list. `0` edits the filter
again; Esc (with the box closed) clears it, and a second Esc cancels cdx as usual.
Opening another folder, going up, `~` and the `1`-`9` jumps also clear the filter;
toggling hidden folders or files keeps it.

## Speed dial and new windows

`cdx 1` ... `cdx 9` change to the folder in that favourite slot, without opening the
browser. `cdx <name>` does the same for the favourite whose alias (or folder name, when
it has no alias) matches: an exact match first, then ignoring case. Words with a slash,
a drive colon or a leading `.`, `~` or `-` are never looked up; they go to `cd`. If the
word is not a favourite, `cd` gets it (see "Working as cd"), so `cdx 5` with only three
favourites is `cd 5`. A favourite whose folder has been deleted is reported by cdx
(exit code 2) instead, so your shell stays put.

`-n` / `--new-window` works with the browser, with speed dial, and with a folder path
(`cdx -n ~/src` opens it in a new window). The chosen folder opens in a new window and
your current shell does not move.

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
| `b`                    | bookmark the highlighted folder (on the `.` row, the folder you are in); the `●` marks the favourite you are standing in |
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
- The panel needs a window at least 48 columns wide (configurable). Narrower than that it
  hides itself, though the `1`-`9` keys keep working.
- Stored as plain text, one line per favourite (`alias<TAB>path`), at
  `~/.cdx/favorites.tsv` (macOS/Linux) or `%USERPROFILE%\.cdx\favorites.tsv` (Windows).
  Set `CDX_CONFIG_DIR` to use another folder. Safe to edit by hand or keep in your dotfiles.
- Upgrading: if there is no `~/.cdx/favorites.tsv` yet, the list from the old location
  (`~/.config/cdx/` or `%APPDATA%\cdx\`) is read once and saved to the new one. The old
  file is left in place; delete it when you like.

## Configuration

Everything lives in one folder: `~/.cdx` (macOS/Linux) or `%USERPROFILE%\.cdx` (Windows),
or wherever `CDX_CONFIG_DIR` points. It holds `favorites.tsv` and, if you want one,
`config.ini`. Nothing is required: without a config file you get the defaults.

```
cdx-bin config init      write a fully commented config.ini to start from
cdx-bin config path      print where it is
cdx-bin config show      print the settings in effect
cdx-bin config check     report mistakes in the file (exit code 1 if any)
```

(`cdx config ...` works too once the shell integration is set up; a folder literally
named `config` can be opened with `cdx ./config`. The same goes for `init`.)

The file is a plain ini file. Lines starting with `#` or `;` are comments, and deleting a
line brings back its default. A mistake never stops cdx: the bad line is ignored, cdx
prints what was wrong in your terminal, and shows a one-line note on the first screen.

```ini
[paths]
# relative to the cdx folder, or absolute
favorites_file = favorites.tsv

[ui]
# same as always passing -h
show_hidden = false
# list files as well as folders from the start
show_files = false
# Windows only: names starting with a dot count as hidden (no key for this)
windows_dot_is_hidden = true
filter_case_sensitive = false
# narrower windows hide the favourites panel
min_width_for_favorites = 48
max_alias_len = 40

[permissions]
# see "Permissions" below
macos_ask_again = true

[new_window]
# what `cdx -n` runs; empty or missing = built-in choice
command = kitty --directory {path}

[keys]
# while browsing
bookmark = b B
filter = /
parent = left h backspace

[keys.favorites]
# while editing favourites
remove = x delete
```

Comments must be on their own lines; a `#` after a value is part of the value.

**Key bindings.** One line per action, several keys separated by spaces, an empty value
unbinds the action. Names: single characters (`k`, `G`, `/`), `space enter esc tab backtab
backspace delete insert home end pageup pagedown up down left right f1`-`f12`, with optional
`ctrl-`, `alt-` and `shift-` prefixes (`ctrl-x`, `alt-left`). Setting an action replaces its
default keys, so write all the keys you want. The on-screen help line and the hints follow
whatever you configure.

| Browsing (`[keys]`) | Default | | Favourites (`[keys.favorites]`) | Default |
|---|---|---|---|---|
| `move_up` | `up k` | | `move_up` | `up k` |
| `move_down` | `down j` | | `move_down` | `down j` |
| `page_up` / `page_down` | `pageup` / `pagedown` | | `pick_up` | `space` |
| `top` / `bottom` | `home g` / `end G` | | `open` | `enter right l` |
| `open` | `right l enter` | | `rename` | `r` |
| `parent` | `left h backspace` | | `remove` | `x delete` |
| `select` | `space` | | `back` | `esc` |
| `cancel` | `esc q` | | `cancel` | `q` |
| `toggle_hidden` | `.` | | `toggle_favorites` | `tab backtab` |
| `toggle_files` | `f` | | | |
| `home` | `~` | | | |
| `bookmark` | `b` | | | |
| `filter` | `0` | | | |
| `toggle_favorites` | `tab backtab` | | | |

Not configurable, on purpose: `1`-`9`, Ctrl-C, and the keys inside the filter box and the
rename box. Two actions on the same key are reported, and the one listed first in the tables
above wins.

**Precedence.** Command-line flags add to the config (`-h` turns hidden folders on, it
cannot turn them off). `CDX_CONFIG_DIR` decides where the folder is; it cannot be set in
the file.

**New-window command.** `{path}` is replaced by the folder, which is also the working
directory of the new process. Quotes group words and there are no escape characters, so
Windows paths work as typed. Examples: `open -a Ghostty {path}` (macOS),
`wezterm start --cwd {path}`, `wt -d "{path}"`.

## Permissions

Some folders are protected by the operating system.

- **macOS** guards Desktop, Documents, Downloads and external volumes behind a privacy
  prompt for your terminal app. If you said "Don't Allow" earlier and later open one of those
  folders again, cdx clears that stored answer for the folder and retries once, so macOS
  asks you again (switch this off with `macos_ask_again = false` in `config.ini`). Decline again and you see the usual `Operation not permitted` error.
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
   runs the binary and acts on what it printed: a folder path means `cd` there; the
   marker `cdx:use-cd` means "this was ordinary `cd` usage", so the function runs `cd`
   with your original arguments; nothing means stay put. Commands (`init`, `config`),
   `--help` and `--version` pass straight through.

If you run `cdx-bin` directly without the wrapper, it still works, but tells you
that the shell integration is not active.

## Project layout

```
src/main.rs      glue: what each kind of request does, printing the result
src/cli.rs       reading the command line, deciding what is cdx's and what is `cd`'s
src/commands.rs  cdx's own commands (init, config) and the reserved-name list
src/browser.rs   the interactive browser (state, keys, drawing, favourites panel)
src/config.rs    settings, key bindings, config.ini parsing, data folder paths
src/favorites.rs favourites: data, file format, load/save
src/newwin.rs    `cdx -n`: opening a new terminal window
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
2. Tag and push, using the same `X.Y.Z` as in `Cargo.toml`: `git tag vX.Y.Z && git push origin vX.Y.Z`
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
