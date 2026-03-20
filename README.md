<h1 align="center">
  <br>
  <img src="assets/logo.svg" alt="logo" width="200">
  <br>
  Promrail
  <br>
  <br>
</h1>

<p align="center">
Git-native GitOps promotion tool written in Rust.
</p>

![Build status](https://img.shields.io/github/actions/workflow/status/forkline/promrail/rust.yml?branch=main)
![Promrail license](https://img.shields.io/github/license/forkline/promrail)

## Features

- **Simple CLI**: `prl` is the default promote command - just run it!
- **Multi-repository support**: Configure multiple GitOps repositories with different structures
- **Allowlist-based file selection**: Only promote explicitly allowed files
- **Protected directories**: Never modify `custom/`, `env/`, `local/` directories
- **Git-native operations**: Uses `git2` for repository handling, status checks, and diffs
- **Colored diff output**: See what would change before applying
- **Version management**: Extract, apply, and diff Helm chart versions and container images
- **Snapshots**: Create restore points before applying changes with rollback support
- **Conflict detection**: Warn on version downgrades during apply

## Installation

### Cargo

```bash
cargo install prl
```

### Arch Linux

```bash
yay -S prl
```

or the binary from AUR:

```bash
yay -S prl-bin
```

### Binaries

Binaries are made available each release for the Linux and MacOS operating systems.

You can download a prebuilt binary from our [Releases](https://github.com/forkline/prl/releases).

```bash
curl -s https://api.github.com/repos/forkline/promrail/releases/latest \
  | grep browser_download_url \
  | grep -v sha256 \
  | grep $(uname -m) \
  | grep linux \
  | cut -d '"' -f 4 \
  | xargs curl -L \
  | tar xvz
sudo mv prl /usr/local/bin
```

## Quick Start

1. Create a `promrail.yaml` configuration file:

```bash
# Generate example config
prl config example > promrail.yaml
```

Or see all configuration options:

```bash
prl config show
```

2. Validate your configuration:

```bash
prl validate
```

3. Preview changes:

```bash
# Diff all files
prl diff --source staging --dest production

# Filter by path
prl diff --source staging --dest production platform/redis-operator
```

4. Promote (default command, applies changes immediately):

```bash
# With defaults configured, just run:
prl

# Or with explicit options:
prl --source staging --dest production

# Dry run first
prl --dry-run

# With confirmation prompt
prl --confirm
```

## Commands

| Command | Description |
|---------|-------------|
| `diff` | Show what would change without applying |
| `promote` | Copy allowlisted files from source to destination |
| `validate` | Validate configuration file |
| `versions` | Version extraction and management |
| `snapshot` | Snapshot management for rollbacks |
| `config` | Configuration reference and examples |

### Version Management

Extract and apply Helm chart versions and container image tags:

```bash
# Extract versions from a repository
prl versions extract --path ~/gitops/staging -o versions.json

# Apply versions to another environment
prl versions apply -f versions.json --path ~/gitops/production

# Compare versions between environments
prl versions diff --source ~/gitops/staging --dest ~/gitops/production

# Apply with conflict detection and snapshot
prl versions apply -f versions.json --path ~/gitops/production \
  --check-conflicts --snapshot
```

### Snapshots

Create restore points before applying changes:

```bash
# List snapshots
prl snapshot list --path ~/gitops/production

# Show snapshot details
prl snapshot show <id> --path ~/gitops/production

# Rollback to a snapshot
prl snapshot rollback <id> --path ~/gitops/production

# Delete a snapshot
prl snapshot delete <id> --path ~/gitops/production
```

### Multi-Source Merge

Merge versions from multiple staging sources:

```bash
# Merge versions from multiple sources
prl versions merge \
  --source ~/gitops/staging-homelab \
  --source ~/gitops/staging-work \
  --explain \
  -o merged-versions.json

# Apply merged versions
prl versions apply -f merged-versions.json \
  --path ~/gitops/production --snapshot
```

### Automatic Review Artifacts

For multi-source `prl promote`, promrail now separates straightforward common changes from changes that need review:

- common file updates are promoted normally
- existing version-managed files such as `values.yaml`, `Chart.yaml`, and `kustomization.yaml` are updated through the structured version merge/apply flow so destination-specific config can stay in place
- ambiguous non-version changes create a review artifact under `.promrail/review/`

Typical flow:

```bash
# 1. Run the promotion normally
prl

# 2. If review is needed, promrail prints the artifact path
#    Example: .promrail/review/grigri_cloud__homelab__nbg1_c01.yaml

# 3. Classify the artifact with a coding agent or by editing the YAML
#    - set status: classified
#    - set each item decision: promote | skip
#    - set selected_source for promoted conflicting items

# 4. Run prl again
prl
```

When the artifact still matches the current repo fingerprint, `prl` consumes it automatically on the second run and applies only the approved changes.

You can also avoid repeated review by adding `preserve` rules to a component. A preserve rule keeps destination-specific YAML or JSON paths while still promoting the rest of the file from the chosen source:

```yaml
rules:
  components:
    platform/minio:
      action: always
      preserve:
        - file: templates/kanidm-oauth2-client.yaml
          paths:
            - spec.origin
            - spec.redirectUrl
```

This is designed for agent-generated rules: inspect a real promotion diff once, identify env-specific paths, write them into `promrail.yaml`, and let future promotions run automatically.

After `prl --force`, a practical loop is:

```bash
# 1. Apply the promotion
prl --force

# 2. Inspect what changed
git diff

# 3. Update promrail.yaml with preserve/denylist rules
#    for the env-specific parts of the diff

# 4. Reset only the affected destination files
git checkout -- <affected-files>

# 5. Re-run the promotion with the new rules
prl --force
```

You can hand the rule-tuning step to any coding agent with a prompt like:

```text
Inspect the current git diff after `prl --force`.

Goal:
- keep common promoted changes
- prevent environment-specific config from being promoted again in future runs
- avoid manual review for the same issue next time

What to do:
1. Analyze the current diff file by file.
2. Identify which changes are common/shared, which are environment-specific fields inside mixed files, and which files are fully environment-specific.
3. Update `promrail.yaml` accordingly:
   - add `rules.components.<component>.preserve` entries for YAML/JSON paths that should remain destination-specific
   - add `denylist` entries for files that are entirely environment-specific or not safe to auto-merge
   - keep using `action: always` where automatic promotion should continue
4. Reset only the affected destination files whose env-specific changes should be re-evaluated.
5. Run `prl --force` again.
6. Verify that the new diff preserves env-specific values while still promoting common changes.

Important constraints:
- Do not commit anything.
- Do not revert unrelated changes.
- Prefer automatic rules over review artifacts.
- For Helm-template-heavy files that are not safe for path preservation, use `denylist`.
```

### Config Reference

View configuration documentation directly in the CLI:

```bash
# Show all configuration options
prl config show

# Generate example configuration
prl config example > promrail.yaml

# Generate to a file
prl config example -o promrail.yaml
```

## Options

| Option | Description |
|--------|-------------|
| `-c, --config <FILE>` | Path to config file (env: `PROMRAIL_CONFIG`) |
| `-r, --repo <NAME>` | Repository name from config (env: `PROMRAIL_REPO`) |
| `-l, --log-level <LEVEL>` | Log level: error, warn, info, debug, trace (default: info) |

### Diff/Promote Options

| Option | Description |
|--------|-------------|
| `-s, --source <ENV>` | Source environment (uses default_source if not set) |
| `-d, --dest <ENV>` | Destination environment (uses default_dest if not set) |
| `--no-delete` | Do not delete extra files in destination (delete is default) |
| `--dest-based` | Only operate on directories that exist in both environments |
| `--include-protected` | Include protected directories (custom, env, local) |
| `--dry-run` | Don't modify files (promote only) |
| `--diff` | Show file content changes (promote only) |
| `--confirm` | Ask for confirmation before applying (promote only) |

## Configuration

Run `prl config show` for embedded documentation, or `prl config example` for a sample config file.

### Simple Single-Repo Config

Place `promrail.yaml` in your repo root:

```yaml
version: 1

environments:
  staging: { path: clusters/staging }
  production: { path: clusters/production }

# Optional: enables `prl promote` without args
default_source: staging
default_dest: production

protected_dirs:
  - custom
  - env

allowlist:
  - "platform/**/*.yaml"

denylist:
  - "**/*secret*"
```

Usage:

```bash
# All equivalent when defaults are set:
prl promote
prl promote --no-delete
prl promote --source staging --dest production
```

### Multi-Repo Config

For cross-repo promotion, define multiple repos:

```yaml
repos:
  homelab:
    path: ~/gitops/homelab
    environments:
      staging: { path: clusters/staging }
      production: { path: clusters/production }

  work:
    path: ~/gitops/work
    environments:
      dev: { path: overlays/dev }
      prod: { path: overlays/prod }

default_repo: homelab
```

### File Selection

Files must match the allowlist AND not match the denylist:

```yaml
allowlist:
  - "platform/**/*.yaml"    # All YAML files under platform/
  - "system/**/*.yaml"      # All YAML files under system/
  - "apps/specific/**/*.yaml"  # Specific app only

denylist:
  - "**/secrets*"           # Any file with "secrets" in the name
  - "**/test/**"            # Test directories
```

### Protected Directories

These directories are never modified during promotion:

```yaml
protected_dirs:
  - custom      # Environment-specific customizations
  - env         # Environment variables
  - local       # Local development configs
```

### Delete Behavior

By default, prl deletes files in destination that don't exist in source (matching the Python promote script):

```bash
# Default: delete extra files in destination
prl promote --source staging --dest production

# Keep extra files with --no-delete
prl promote --source staging --dest production --no-delete

# Dest-based: only delete if parent dir exists in source
prl promote --source staging --dest production --dest-based
```

### Promotion Rules

For multi-source promotions, define rules in `promrail.yaml`:

```yaml
rules:
  # Define sources with priorities
  sources:
    staging-homelab:
      priority: 1
      include: [platform/*, system/monitoring/*]
      exclude: [platform/homeassistant/*]
    staging-work:
      priority: 2
      include: [apps/*, system/auth/*]

  # Conflict resolution
  conflict_resolution:
    version_strategy: highest  # highest | source_priority
    source_order: [staging-work, staging-homelab]

  # Component-level rules
  components:
    platform/postgres-operator:
      action: always
    platform/homeassistant:
      action: never
      notes: "Home-specific, not for work production"
    system/auth/keycloak:
      action: review
      notes: "Check for env-specific configs"

  # Global rules
  global:
    exclude: ["*/custom/*", "*/env/*"]
    version_rules:
      allow_downgrade: false
```

Actions: `always` (promote), `review` (flag for review), `never` (exclude).

## Architecture

See [docs/adr-001-architecture.md](docs/adr-001-architecture.md) for design decisions.

## Documentation

- [Usage Guide](docs/usage.md) - Detailed workflows and examples
- [Workflows & Secrets](docs/workflows.md) - CI/CD setup and secrets
- [Architecture Decision Record](docs/adr-001-architecture.md) - Design decisions
- [AGENTS.md](AGENTS.md) - Opencode AI assistant guidelines
- `prl config show` - Embedded configuration reference

## Development

### Build

```bash
cargo build --release
```

### Test

```bash
# Unit tests
cargo test

# E2E tests
cargo test --test e2e

# All tests
just test-all
```

### Lint

```bash
just lint
```

## License

MIT
