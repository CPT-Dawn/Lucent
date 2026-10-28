# Releasing Lucent

One tag push publishes everything. `.github/workflows/release.yml` then:

1. builds the x86_64 binary in an Arch container;
2. creates the GitHub Release with two assets: `lucent-X.Y.Z.tar.gz` (source)
   and `lucent-X.Y.Z-x86_64.tar.gz` (binary, D-Bus activation file, licence,
   README), plus `SHA256SUMS`;
3. pushes PKGBUILD and .SRCINFO to the AUR for **`lucent`** (built from the
   source asset, x86_64 and aarch64) and **`lucent-bin`** (the prebuilt
   x86_64 asset).

## Cutting a release

1. Set `version` in `Cargo.toml` to the new version, run `cargo build` so
   `Cargo.lock` follows, and commit. The workflow refuses a tag that doesn't
   match `Cargo.toml`.
2. Tag and push: `git tag -s v2.0.0 -m "Lucent 2.0.0" && git push origin v2.0.0`.
3. Edit the generated release notes on GitHub if needed (for example, the
   "upgrading from v1" section).

To retry a failed release, run the workflow by hand with the same tag and
"dry run" unticked. It reuses the existing release and replaces its assets.

## Dry run

Run the **Release** workflow by hand (Actions tab, or
`gh workflow run release.yml --ref <branch> -f tag=v9.9.9 -f dry_run=true`).
It builds the selected branch, borrows the version from the tag (which need not
exist), generates both PKGBUILDs and .SRCINFOs, runs namcap and test-builds
`lucent-bin`, and uploads the results as workflow artifacts. Nothing is
published. GitHub only offers manual runs for workflows that exist on the
default branch.

Locally, as a normal user on Arch:

```sh
bash packaging/test.sh                          # generator and tarball checks
cargo build --release --locked
bash packaging/release-tarballs.sh 9.9.9 target/release/lucent /tmp/dist
bash packaging/aur/prepare.sh 9.9.9 /tmp/dist /tmp/aur
```

The PKGBUILD templates live in `packaging/aur/generate-pkgbuild.sh`. Change
packaging there, never in the AUR repos, because each release overwrites them.

## One-time setup

- **Secret `AUR_SSH_PRIVATE_KEY`** (repository secret, same name as in v1): a
  private SSH key whose public half is registered on the AUR account that
  maintains `lucent`. Plain or base64-encoded both work.
- **`lucent-bin` on the AUR:** the first push of a new package name creates
  it, owned by the pushing account, so normally there's nothing to do. If that
  push is rejected, create the package by hand with the same key, then re-run
  the release:

  ```sh
  git clone ssh://aur@aur.archlinux.org/lucent-bin.git && cd lucent-bin
  # copy PKGBUILD and .SRCINFO from a dry run's "aur" artifact
  git add PKGBUILD .SRCINFO && git commit -m "Initial import" && git push origin HEAD:master
  ```
