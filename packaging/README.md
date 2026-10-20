# Package-manager manifests

These are **templates**. Every release renders them with the real version and
checksums, so there is nothing to fill in by hand except the one-time setup below.

| Folder | Package manager | Where the rendered file goes |
|---|---|---|
| `homebrew/cdx.rb.tmpl` | Homebrew (macOS, Linux) | `Formula/cdx.rb` in a tap repository |
| `scoop/cdx.json.tmpl` | Scoop (Windows) | `bucket/cdx.json` in a bucket repository |
| `winget/*.yaml.tmpl` | winget (Windows) | a pull request to [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) |

## Each release

1. Push the version tag. The **release** workflow renders the manifests and attaches them
   to the run as the artifact **packaging-manifests**.
2. Download that artifact, then copy the files to the places in the table above.

To render by hand (for example for an older release), put the release's `.sha256` files
in a folder and run:

```sh
sh scripts/render-packaging.sh vX.Y.Z folder-with-sha256-files out
```

`GITHUB_REPOSITORY=owner/name` overrides the repository used in the URLs.

## One-time setup

**Homebrew.** Create a public repository named `homebrew-tap` under your account and
add the rendered file as `Formula/cdx.rb`. Users then run:

```sh
brew install vinayakkuradia/tap/cdx
```

Check a formula locally with `brew audit --new --strict cdx` and `brew test cdx`.

**Scoop.** Create a public repository (for example `scoop-bucket`) with the rendered file
in `bucket/cdx.json`. Users then run:

```powershell
scoop bucket add vinayakkuradia https://github.com/vinayakkuradia/scoop-bucket
scoop install cdx
```

The `checkver` and `autoupdate` blocks let Scoop's own tooling update the manifest.

**winget.** The three `vinayakkuradia.Cdx*.yaml` files go in
`manifests/v/vinayakkuradia/Cdx/<version>/` of a fork of `microsoft/winget-pkgs`.
Validate with `winget validate --manifest <folder>` and open a pull request, or let
[`wingetcreate`](https://github.com/microsoft/winget-create) do it:
`wingetcreate update vinayakkuradia.Cdx --version X.Y.Z --urls <the two .zip URLs> --submit`.
Users then run `winget install vinayakkuradia.Cdx`.

## After installing with any of these

cdx still needs its small shell function so it can change your shell's folder. Each
package prints the one line to add (`cdx-bin init <shell>`); see the README.
