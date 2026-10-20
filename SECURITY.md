# Security policy

## Supported versions

Only the **latest release** receives security fixes. cdx is a small, young tool;
please update before reporting.

## Reporting a vulnerability

Please **do not open a public issue** for a security problem.

Report it privately in either of these ways:

1. **GitHub private vulnerability reporting** (preferred): open the repository's
   **Security** tab and choose **Report a vulnerability**.
2. Email incognitifier@gmail.com: contact email with "cdx security" in the subject.

Please include the cdx version (`cdx-bin --version`), your OS and shell, and the
steps to reproduce. You can expect an acknowledgement within a few days. This is a
volunteer project, so there is no guaranteed fix time, but real vulnerabilities get
priority and you will be credited in the release notes if you wish.

## What cdx does, for threat modelling

- It makes **no network connections**. (The separate install scripts download a
  release from GitHub and verify its SHA-256 checksum.)
- It writes only inside its own folder, `~/.cdx` (or `%USERPROFILE%\.cdx`, or
  `CDX_CONFIG_DIR`): `config.ini` and `favorites.tsv`. After an upgrade it also reads
  the favourites file from its former location once, to copy it over.
- It lists the names of the folders (and, if you show them, files) you browse. It
  never opens your files.
- `config.ini` is **trusted input**. Its `[new_window] command` is run as a program
  when you use `cdx -n`, with the arguments you wrote, so only edit your own config.
- The shell wrapper from `cdx-bin init <shell>` runs `cdx-bin` and then your shell's
  own `cd`. It does not use `eval` on anything cdx-bin prints.
- On macOS, when a protected folder was refused, cdx runs `tccutil reset` for your
  terminal app and that folder category so that the system asks you again. You can
  switch this off with `macos_ask_again = false`.

## Out of scope

Problems that need an attacker who already controls your account, your config
files or your shell startup files are not vulnerabilities in cdx.
