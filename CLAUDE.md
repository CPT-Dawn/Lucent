# Lucent

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `CPT-Dawn/Lucent`, managed via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default canonical labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Commands

- Build: `cargo build --release`
- Test: `cargo test --all-targets` and `cargo test --all-targets --features test-hooks` (CI runs both; the second compiles in the hooks the bus tests click Popups with)
- Lint: `cargo clippy --all-targets -- -D warnings`, again with `--features test-hooks`; `cargo fmt --check`
- Run on the desktop: when nothing owns `org.freedesktop.Notifications` (`busctl --user status org.freedesktop.Notifications` fails), start `./target/release/lucent`, send with `notify-send`, then kill it; D-Bus activation goes back to `/usr/bin/lucent`. `lucent osd volume show` shows the OSD without changing the volume.
- Release: bump `Cargo.toml`, `cargo update -p lucent`, commit `chore(release): prepare for vX.Y.Z`, wait for CI, `git tag -s vX.Y.Z -m "Lucent X.Y.Z"`, push the tag, then `gh release create vX.Y.Z --title "Lucent X.Y.Z" --notes-file ...`. Publishing the release (not the tag) runs `aur-update.yml` (AUR `lucent`) and `release-bin.yml` (tarballs + AUR `lucent-bin`).
