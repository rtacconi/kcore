package version

// Version is the semantic version for this kcore build, used by kctl and
// other Go binaries. The canonical source of truth is the repository-level
// VERSION file, which is consumed by Nix for ISO builds. Go binaries can be
// wired to that value at build time via -ldflags, but default to "dev".
//
// Example:
//   go build -ldflags "-X github.com/kcore/kcore/pkg/version.Version=$(cat VERSION)" ...
//
// For now we keep a sensible default so builds always succeed.

var Version = "dev"
