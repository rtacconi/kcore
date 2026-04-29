# Automated Release Process (GitHub Actions + Nix)

Releases are automated from Git tags. Pushing a version tag (`vX.Y.Z`) starts
the [`release`](../.github/workflows/release.yml) workflow, which builds the Nix
ISO and `kcore-kctl`, packages `dist/`, uploads a workflow artifact bundle, and
publishes the GitHub Release assets.

## Version sources (policy)

| Source | Role |
|--------|------|
| [`VERSION`](../VERSION) (single line, e.g. `0.2.0`) | **Product / packaging version**: Nix `kcoreVersion`, ISO filename `kcoreos-$(VERSION)-x86_64-linux.iso`, Git tag `v$(VERSION)`, release assets. **Bump this for every release.** |
| `crates/*/Cargo.toml` `version = "…"` | Rust crate semver. **Not automatically tied to `VERSION`.** This repo usually bumps crate versions in the same PR as `VERSION` so `kcore-kctl --version` matches the product version. |

## Preconditions

- The version bump PR is merged to `main`, including [`VERSION`](../VERSION)
  (and crate versions, when applicable).
- The GitHub Actions `release` workflow has `contents: write` permission. This is
  declared in the workflow and uses GitHub's `GITHUB_TOKEN`.
- The release tag must match `VERSION`: if `VERSION` is `0.2.0`, the tag must be
  `v0.2.0`. The workflow validates this before building.

## Steps

1. **Bump version**  
   Edit [`VERSION`](../VERSION) to `X.Y.Z` (and optionally align `crates/kctl/Cargo.toml` and other crates if you follow the policy above). Open a PR, get CI green, merge to `main`.

2. **Tag the release commit** (on `main` after merge):

   ```bash
   git fetch origin main && git checkout main && git pull
   git tag -a "v$(tr -d '\n' < VERSION)" -m "kcore $(tr -d '\n' < VERSION)"
   git push origin "v$(tr -d '\n' < VERSION)"
   ```

3. **GitHub Actions publishes the release**
   The tag push starts `.github/workflows/release.yml`. The workflow:

   - checks out the tag,
   - verifies `v$(cat VERSION)` matches the tag,
   - installs Nix,
   - runs `make release` (`release-build` + `release-dist`),
   - uploads `dist/*` as a workflow artifact,
   - runs `make release-publish` with `GH_TOKEN=${{ github.token }}`.

   `scripts/release.sh publish` uses GitHub-generated release notes unless a
   `RELEASE_NOTES` file is explicitly supplied.

4. **Manual re-run if needed**
   If the workflow fails after the tag exists, re-run it in GitHub Actions or use
   `workflow_dispatch` with the existing tag (for example `v0.2.0`). Publishing is
   idempotent for assets: if the release already exists, the script re-uploads
   assets with `gh release upload --clobber`.

## Local debugging targets

The Make targets remain useful for reproducing the release build locally on
Linux x86_64 with Nix:

```bash
make release-build
make release-dist
```

`make release` runs both. It produces:

- `dist/kcore-kctl-$(VERSION)-linux-x86_64.tar.gz` (binary at archive root: `kcore-kctl`)
- `dist/kcoreos-$(VERSION)-x86_64-linux.iso` (release asset name; copied from the single ISO produced under `result-iso/iso/`)
- `dist/SHA256SUMS` for both files

Manual publishing is still available for break-glass recovery:

```bash
GH_TOKEN=... make release-publish
```

By default this uses GitHub-generated release notes. To force custom notes:

```bash
RELEASE_NOTES=path/to/notes.md GH_TOKEN=... make release-publish
```

## Artifact notes

- **kctl** in the tarball is the **Nix-built** `kcore-kctl` from `.#kcore-kctl` (same lineage as the ISO), not a raw `cargo build`.
- **Platform**: **linux x86_64** (glibc via Nix). No musl/static build in this flow.
- **Large files**: ISOs are ~1–2 GiB; GitHub per-file limit is 2 GiB. Stay under that or split hosting for huge artifacts.

## Troubleshooting

- **Workflow rejects the tag**: ensure `VERSION` contains `X.Y.Z` and the tag is exactly `vX.Y.Z`.
- **`gh release create` fails on `--verify-tag` locally**: push the tag first: `git push origin vX.Y.Z`.
- **Wrong ISO name**: Nix may place the built ISO under a NixOS-derived name in `result-iso/iso/`; the dist step discovers the single `*.iso` there and copies it to the release asset name `kcoreos-$(VERSION)-x86_64-linux.iso`.
- **Manual publish authentication**: set `GH_TOKEN` in the environment for non-interactive `gh`. The GitHub Actions workflow already sets this from `${{ github.token }}`.
