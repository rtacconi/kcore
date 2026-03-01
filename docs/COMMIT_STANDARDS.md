# Commit Message Standards

This project follows [Conventional Commits](https://www.conventionalcommits.org/) specification for clear and structured commit history.

---

## Format

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

---

## Types

- **feat**: A new feature for the user
- **fix**: A bug fix
- **docs**: Documentation only changes
- **style**: Code style changes (formatting, missing semi-colons, etc)
- **refactor**: Code change that neither fixes a bug nor adds a feature
- **perf**: Performance improvements
- **test**: Adding or updating tests
- **build**: Changes to build system or dependencies
- **ci**: Changes to CI/CD configuration
- **chore**: Other changes that don't modify src or test files
- **revert**: Reverts a previous commit

---

## Examples

### Feature Addition

```
feat(kctl): add VM creation command

Implement kctl create vm command with full flag support for CPU,
memory, disk, and network configuration. Includes help text and
examples.

- Add create.go with VM, volume, network creation
- Integrate Cobra CLI framework
- Add comprehensive help text
- Build for macOS ARM64

Closes #123
```

### Bug Fix

```
fix(installer): handle LVM volumes during disk preparation

The install-to-disk script now deactivates LVM volume groups before
wiping disks, preventing "device is busy" errors.

- Add vgchange -an for all VGs
- Add unmounting for existing partitions
- Add retry logic for wipefs

Fixes #456
```

### Documentation

```
docs: reorganize all documentation into docs/ directory

Move all markdown files from root to docs/ for better organization.
Create comprehensive README with TOC linking to all documentation.

- Move 13 MD files to docs/
- Create new README with categories
- Update internal links
- Add docs/scripts.md
```

### Refactoring

```
refactor(scripts): move inline bash to separate files

Extract all bash logic from devbox.json into scripts/ directory
for better maintainability and testing.

- Create 7 script files in scripts/
- Update Makefile to call scripts
- Simplify devbox.json to call Make
- Update documentation

BREAKING CHANGE: devbox scripts now require Make
```

### Build System

```
build: add kctl CLI compilation target

Add Makefile target and devbox script for building kctl CLI tool
compiled for macOS ARM64.

- Add 'make kctl' target
- Add GOOS/GOARCH flags
- Add to devbox.json
- Add to make help
```

### Chore

```
chore(deps): update cobra to v1.10.1

Update github.com/spf13/cobra dependency to latest version for
improved CLI handling and bug fixes.
```

---

## Scope

The scope is optional and indicates what part of the codebase is affected:

- **kctl**: CLI tool
- **controller**: Controller service
- **node-agent**: Node agent service
- **installer**: install-to-disk script
- **scripts**: Automation scripts
- **docs**: Documentation
- **build**: Build system (Makefile, Nix)
- **iso**: ISO building
- **api**: gRPC API definitions

Examples:
```
feat(kctl): add describe command
fix(node-agent): resolve libvirtd connection issues
docs(kctl): add comprehensive CLI reference
refactor(installer): extract LVM handling to function
build(nix): update flake inputs
```

---

## Body

The body is optional and should include:

- **Motivation**: Why is this change needed?
- **Context**: What problem does it solve?
- **Details**: Implementation specifics
- **Breaking Changes**: Any incompatibilities

Use imperative mood: "add" not "added", "fix" not "fixed"

---

## Footer

The footer is optional and can include:

- **Breaking Changes**: `BREAKING CHANGE: description`
- **Issue References**: `Fixes #123`, `Closes #456`, `Refs #789`
- **Co-authors**: `Co-authored-by: Name <email>`
- **Signed-off**: `Signed-off-by: Name <email>`

---

## Rules

### DO ✅

- Use present tense: "add feature" not "added feature"
- Use imperative mood: "change" not "changes"
- Start with lowercase (after type)
- No period at the end of subject line
- Separate subject from body with blank line
- Wrap body at 72 characters
- Reference issues in footer

### DON'T ❌

- Don't exceed 50 characters in subject (aim for it, 72 max)
- Don't use vague messages like "fix stuff" or "updates"
- Don't mix multiple unrelated changes
- Don't include implementation details in subject

---

## Real Examples from kcore

### Good Examples

```
feat(kctl): implement user-friendly VM management CLI

Create kubectl-style CLI tool for kcore cluster management,
replacing complex grpcurl commands with intuitive commands.

- Implement create, get, describe, delete, apply commands
- Add Cobra framework integration
- Include comprehensive help text
- Support VM, volume, network, node resources
- Compile for macOS ARM64

Example usage:
  kctl create vm web-01 --cpu 4 --memory 8G
  kctl get vms
  kctl describe vm web-01

Closes #42
```

```
fix(installer): prevent "device busy" during installation

Add LVM volume group deactivation and partition unmounting
before disk operations to prevent installation failures.

- Deactivate all VGs with vgchange -an
- Unmount existing partitions on target disk
- Add retry logic for wipefs operations

Fixes #38
```

```
docs: create comprehensive documentation structure

Reorganize all documentation into docs/ directory with clear
categorization and cross-references.

- Move 13 markdown files to docs/
- Create README with categorized TOC
- Add docs/KCTL.md for CLI reference
- Update all internal documentation links
- Create docs/scripts.md for automation
```

```
refactor(build): separate scripts from inline bash

Extract all automation logic from devbox.json into dedicated
script files for improved maintainability and testability.

- Create scripts/{build-iso,create-vm,delete-vm,...}.sh
- Update Makefile with script targets
- Simplify devbox.json to call Make targets
- Add scripts/README.md documentation
- Make all scripts executable with proper headers
```

```
build(nix): enable libvirtd and virtlogd in installed system

Update install-to-disk to configure systemd services for
libvirtd and virtlogd, ensuring node-agent can manage VMs
without manual intervention.

- Set virtualisation.libvirtd.enable = true
- Add virtlogd systemd service configuration
- Include in embedded configuration.nix
- Copy node-agent binary to /opt/kcore/bin

BREAKING CHANGE: Previous installations require manual
libvirtd configuration. Fresh installs are fully automated.
```

---

## Commit Message Template

Create `.gitmessage` in your home directory:

```
# <type>[optional scope]: <description>
# |<----  Using a Maximum Of 50 Characters  ---->|


# Explain why this change is being made
# |<----   Try To Limit Each Line to a Maximum Of 72 Characters   ---->|


# Provide links or keys to any relevant tickets, articles or other resources
# Example: Fixes #23


# --- COMMIT END ---
# Type can be
#    feat     (new feature)
#    fix      (bug fix)
#    refactor (refactoring code)
#    style    (formatting, missing semi colons, etc; no code change)
#    docs     (changes to documentation)
#    test     (adding or refactoring tests; no production code change)
#    chore    (updating build tasks etc; no production code change)
#    perf     (performance improvement)
#    build    (build system or dependencies)
#    ci       (CI/CD configuration)
#    revert   (revert previous commit)
# --------------------
# Remember to
#   - Use the imperative mood in the subject line
#   - Do not end the subject line with a period
#   - Separate subject from body with a blank line
#   - Use the body to explain what and why vs. how
#   - Reference issues and pull requests in footer
# --------------------
```

Configure git to use it:

```bash
git config --global commit.template ~/.gitmessage
```

---

## Tools

### Commitizen

Install for interactive commit message creation:

```bash
# Install globally
npm install -g commitizen cz-conventional-changelog

# Or use with npx
npx commitizen

# Or use devbox (if configured)
devbox run commit
```

### Commitlint

Enforce commit message standards:

```bash
# Install
npm install --save-dev @commitlint/{cli,config-conventional}

# Configure
echo "module.exports = {extends: ['@commitlint/config-conventional']}" > commitlint.config.js

# Use with git hooks
npm install --save-dev husky
npx husky install
npx husky add .husky/commit-msg 'npx --no -- commitlint --edit "$1"'
```

---

## References

- [Conventional Commits](https://www.conventionalcommits.org/)
- [Angular Commit Guidelines](https://github.com/angular/angular/blob/master/CONTRIBUTING.md#commit)
- [Semantic Versioning](https://semver.org/)
- [How to Write a Git Commit Message](https://chris.beams.io/posts/git-commit/)

---

## Quick Reference Card

```bash
# Feature
feat(scope): add new feature

# Bug fix
fix(scope): resolve issue with X

# Documentation
docs: update installation guide

# Refactoring
refactor(scope): simplify logic in X

# Performance
perf(scope): improve query speed

# Tests
test(scope): add unit tests for X

# Build
build: update dependencies

# CI/CD
ci: add GitHub Actions workflow

# Chore
chore: update .gitignore

# Style
style: format code with prettier

# Revert
revert: revert "feat: add feature X"
```

