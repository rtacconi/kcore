#!/usr/bin/env bash
# Build release artifacts (Nix ISO + kcore-kctl), package dist/, publish GitHub Release.
# Usage:
#   ./scripts/release.sh build    # nix build ISO + kcore-kctl -> result-iso, result-kctl
#   ./scripts/release.sh dist     # dist/*.tar.gz, ISO copy, dist/SHA256SUMS
#   ./scripts/release.sh publish  # gh release create/upload (needs tag on remote)
# Environment:
#   RELEASE_NOTES   Optional path to release notes file (defaults to RELEASE_NOTES.md if present;
#                   otherwise GitHub auto-generated release notes are used)
#   GH_REPO         owner/repo override for gh (optional; defaults to git remote)
set -euo pipefail

ROOT="$(cd "$(dirname "${0}")/.." && pwd)"
cd "${ROOT}"

VERSION="$(tr -d '\n' < VERSION)"
ISO_NAME="kcoreos-${VERSION}-x86_64-linux.iso"
KCTL_ARCHIVE="kcore-kctl-${VERSION}-linux-x86_64.tar.gz"

die() {
	echo "release.sh: $*" >&2
	exit 1
}

require_cmd() {
	command -v "${1}" >/dev/null 2>&1 || die "missing required command: ${1}"
}

cmd_build() {
	require_cmd nix
	echo "==> Building ISO (${ISO_NAME})..."
	nix build ".#nixosConfigurations.kcore-iso.config.system.build.isoImage" -o result-iso
	echo "==> Building kcore-kctl..."
	nix build ".#kcore-kctl" -o result-kctl
	echo "==> Build outputs:"
	ls -lh result-iso/iso/*.iso
	ls -lh result-kctl/bin/kcore-kctl
}

cmd_dist() {
	require_cmd tar
	require_cmd sha256sum
	[[ -f result-kctl/bin/kcore-kctl ]] || die "run '${0} build' first (missing result-kctl/bin/kcore-kctl)"
	shopt -s nullglob
	iso_candidates=(result-iso/iso/*.iso)
	shopt -u nullglob
	[[ "${#iso_candidates[@]}" -eq 1 ]] || die "expected exactly one ISO under result-iso/iso/; run '${0} build' first"
	ISO_SRC="${iso_candidates[0]}"

	mkdir -p dist
	echo "==> Packaging ${KCTL_ARCHIVE}..."
	tar -C result-kctl/bin -czf "dist/${KCTL_ARCHIVE}" kcore-kctl
	echo "==> Copying $(basename "${ISO_SRC}") to dist/${ISO_NAME}..."
	cp -f "${ISO_SRC}" "dist/${ISO_NAME}"
	echo "==> Writing dist/SHA256SUMS..."
	(
		cd dist
		sha256sum "${ISO_NAME}" "${KCTL_ARCHIVE}" >SHA256SUMS
	)
	echo "==> dist layout:"
	ls -lh dist/
	cat dist/SHA256SUMS
}

cmd_publish() {
	require_cmd nix
	[[ -f "dist/${KCTL_ARCHIVE}" ]] || die "run '${0} dist' first"
	[[ -f "dist/${ISO_NAME}" ]] || die "run '${0} dist' first"
	[[ -f dist/SHA256SUMS ]] || die "run '${0} dist' first"

	TAG="v${VERSION}"
	assets=("dist/${KCTL_ARCHIVE}" "dist/${ISO_NAME}" dist/SHA256SUMS)
	notes_args=()
	if [[ -n "${RELEASE_NOTES:-}" ]]; then
		[[ -f "${RELEASE_NOTES}" ]] || die "missing RELEASE_NOTES file: ${RELEASE_NOTES}"
		notes_args=(--notes-file "${RELEASE_NOTES}")
	elif [[ -f RELEASE_NOTES.md ]]; then
		notes_args=(--notes-file RELEASE_NOTES.md)
	else
		notes_args=(--generate-notes)
	fi

	if nix develop --command gh release view "${TAG}" >/dev/null 2>&1; then
		echo "==> GitHub release ${TAG} exists; uploading assets with --clobber..."
		nix develop --command gh release upload "${TAG}" --clobber "${assets[@]}"
	else
		echo "==> Creating GitHub release ${TAG} (verify-tag)..."
		nix develop --command gh release create "${TAG}" \
			--verify-tag \
			--title "kcore ${VERSION}" \
			"${notes_args[@]}" \
			"${assets[@]}"
	fi
	echo "==> Done: nix develop --command gh release view ${TAG}"
}

usage() {
	echo "Usage: ${0} {build|dist|publish}"
	exit 1
}

case "${1:-}" in
	build) cmd_build ;;
	dist) cmd_dist ;;
	publish) cmd_publish ;;
	*) usage ;;
esac
