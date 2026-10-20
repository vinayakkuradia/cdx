# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0]

### Added
- **`cdx` is now an extension of `cd`.** Anything that is not cdx's own (`cdx ..`,
  `cdx -`, `cdx ~/src`, `cdx -L link`, a local folder name) is handed to your shell's
  `cd` unchanged. A single word or digit is looked up among the favourites first, and a
  favourite wins.
- **Speed dial:** `cdx 1`-`cdx 9` and `cdx <name or alias>` go straight to a favourite.
- **Filter:** press `0` and type to narrow the list; Esc closes the box and keeps the
  results.
- **Files:** press `f` to list files too. cdx cannot enter a file, so choosing one (or
  bookmarking it) uses the folder it is in.
- **`cdx -n`:** open the chosen folder in a new terminal window instead of changing the
  current shell's folder; works with favourites and with a folder path.
- **`cdx -b [PATH]`:** open the browser, optionally starting at PATH.
- **Configuration** in `~/.cdx/config.ini`: every key binding, display and filter
  settings, the `-n` command and more. `cdx-bin config init | path | show | check`.
- Reserved command words: no favourite can be named `config`, `init` or any future
  command.
- macOS: when access to Desktop, Documents, Downloads or an external volume was refused
  earlier, cdx makes the system ask again.
- Windows: names starting with `.` count as hidden (setting `windows_dot_is_hidden`).
- Shell integration test, coverage and audit jobs in CI; package-manager manifests for
  Homebrew, Scoop and winget; documentation comments on every item.
- Dual licence: MIT or Apache-2.0. Contributing guide, code of conduct, security policy,
  issue and pull request templates.

### Changed
- Files are stored in `~/.cdx` (`%USERPROFILE%\.cdx` on Windows). Favourites from the
  old location are copied over once; the old file is left alone.
- `b` bookmarks the highlighted folder instead of the folder you are in.
- Cancelling with Esc or `q` now exits with code 0, since it is a normal outcome.
- Opening the browser at a path is now `cdx -b <path>`; `cdx <path>` is plain `cd`.

## [0.2.0]

### Added
- The folder browser with hidden-folder toggle, up to nine favourites with aliases and
  reordering, shell integration for bash, zsh, fish, PowerShell and cmd.exe, and
  installers for macOS, Linux and Windows.

[Unreleased]: https://github.com/vinayakkuradia/cdx/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/vinayakkuradia/cdx/releases/tag/v1.0.0
[0.2.0]: https://github.com/vinayakkuradia/cdx/releases/tag/v0.2.0
