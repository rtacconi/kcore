# Release process (Make + Nix + GitHub Releases)

Releases are **operator-driven**: build artifacts locally with Nix, publish to GitHub with `gh`. There is no release workflow in GitHub Actions; CI on `main` remains the quality gate only.

## Version sources (policy)

| Source | Role |
|--------|------|
| [`VERSION`](../VERSION) (single line, e.g. `0.2.0`) | **Product / packaging version**: Nix `kcoreVersion`, ISO filename `kcoreos-$(VERSION)-x86_64-linux.iso`, Git tag `v$(VERSION)`, release assets. **Bump this for every release.** |
| `crates/*/Cargo.toml` `version = "…"` | Rust crate semver (currently `0.1.0` for workspace members). **Not automatically tied to `VERSION`.** Optional: bump crate versions in the same PR as `VERSION` if you want `kcore-kctl --version` to match the product version everywhere. |

## Preconditions

- **Host**: Linux **x86_64** with Nix and flakes working (`nix build .#kcore-kctl`).
- **GitHub CLI**: `gh` installed and authenticated (`gh auth login`) with `contents: write` on the repo. Optional: set `GH_REPO=owner/repo` if not using the default remote.
- **Tag**: The annotated tag `v$(cat VERSION)` must exist **on the remote** before `make release-publish` (the script uses `gh release create --verify-tag`).

## Steps

1. **Bump version**  
   Edit [`VERSION`](../VERSION) to `X.Y.Z` (and optionally align `crates/kctl/Cargo.toml` and other crates if you follow the policy above). Open a PR, get CI green, merge to `main`.

2. **Tag the release commit** (on `main` after merge):

   ```bash
   git fetch origin main && git checkout main && git pull
   git tag -a "v$(tr -d '\n' < VERSION)" -m "kcore $(tr -d '\n' < VERSION)"
   git push origin "v$(tr -d '\n' < VERSION)"
   ```

3. **Quality gate (recommended)**  
   `make check` and/or `make test-all` per [rust-quality-checks](../.cursor/rules/rust-quality-checks.mdc).

4. **Build artifacts (Nix)**  
   At the same commit as the tag (or clean tree on `main` at that commit):

   ```bash
   make release-build
   ```

   This runs [`scripts/release.sh`](../scripts/release.sh) `build`: ISO → `result-iso/`, `kcore-kctl` → `result-kctl/`.

5. **Package `dist/`**  
   Produces the GitHub upload set:

   ```bash
   make release-dist
   ```

   - `dist/kcore-kctl-$(VERSION)-linux-x86_64.tar.gz` (binary at archive root: `kcore-kctl`)  
  - `dist/kcoreos-$(VERSION)-x86_64-linux.iso` (release asset name; copied from the single ISO produced under `result-iso/iso/`)  
   - `dist/SHA256SUMS` for both files  

   Or in one step after a successful build: `make release` (build + dist; does not publish).

6. **Release notes**  
   Copy the template and edit:

   ```bash
   cp RELEASE_NOTES.template.md RELEASE_NOTES.md
   # edit RELEASE_NOTES.md (not committed; see .gitignore)
   ```

   Publish uses `RELEASE_NOTES` env if you need another path: `RELEASE_NOTES=path/to/notes.md make release-publish`.

7. **Publish the GitHub Release**  

   ```bash
   make release-publish
   ```

   This runs `gh release create v$(VERSION) --verify-tag` and uploads the tarball, ISO, and `SHA256SUMS`.

## Artifact notes

- **kctl** in the tarball is the **Nix-built** `kcore-kctl` from `.#kcore-kctl` (same lineage as the ISO), not a raw `cargo build`.
- **Platform**: **linux x86_64** (glibc via Nix). No musl/static build in this flow.
- **Large files**: ISOs are ~1–2 GiB; GitHub per-file limit is 2 GiB. Stay under that or split hosting for huge artifacts.

## Troubleshooting

- **`gh release create` fails on `--verify-tag`**: push the tag first: `git push origin vX.Y.Z`.
- **Wrong ISO name**: Nix may place the built ISO under a NixOS-derived name in `result-iso/iso/`; the dist step discovers the single `*.iso` there and copies it to the release asset name `kcoreos-$(VERSION)-x86_64-linux.iso`.
- **Token in automation**: set `GH_TOKEN` in the environment for non-interactive `gh` (e.g. CI in the future); this doc targets local operator use.
