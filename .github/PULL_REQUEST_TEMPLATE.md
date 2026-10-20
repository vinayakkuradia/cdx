## What does this change, and why?

<!-- Link the issue it fixes, e.g. "Fixes #12". Keep one topic per pull request. -->

## How was it tested?

<!-- Commands run, OS and shell tried. -->

## Checklist

- [ ] `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] `cargo test` passes, and new behaviour has tests
- [ ] Every new item (function, struct, field, constant) has a `///` doc comment
- [ ] README and `--help` are updated if behaviour, keys, commands or settings changed
- [ ] `CHANGELOG.md` has a line under **Unreleased**
- [ ] OS-specific code is behind `cfg` and the other platforms still build
- [ ] Plain `cd` pass-through is unaffected (`cdx ..`, `cdx -`, `cdx ~/x`)
