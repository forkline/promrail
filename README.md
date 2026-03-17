# Promrail

Git-native GitOps promotion tool written in Rust.

## Features

- **Multi-repository support**: Configure multiple GitOps repositories with different structures
- **Allowlist-based file selection**: Only promote explicitly allowed files
- **Protected directories**: Never modify `custom/`, `env/`, `local/` directories
- **Git-native operations**: Uses `git2` for repository handling, status checks, and diffs
- **Colored diff output**: See what would change before applying
- **Audit logging**: Track promotions in `.promotion-log.yaml`
- **Version management**: Extract, apply, and diff Helm chart versions and container images
- **Snapshots**: Create restore points before applying changes with rollback support
- **Conflict detection**: Warn on version downgrades during apply

## Installation

```bash
cargo build --release
```

## Quick Start

1. Create a `promrail.yaml` configuration file:

```bash
# Generate example config
promrail config example > promrail.yaml
```

Or see all configuration options:

```bash
promrail config show
```

2. Validate your configuration:

```bash
promrail validate
```

3. Preview changes:

```bash
# Diff all files
promrail diff --source staging --dest production

# Filter by path
promrail diff --source staging --dest production platform/redis-operator
```

4. Promote (requires confirmation or `--yes`):

```bash
# Dry run first
promrail promote --source staging --dest production --dry-run

# Actually promote
promrail promote --source staging --dest production --yes
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
promrail versions extract --path ~/gitops/staging -o versions.json

# Apply versions to another environment
promrail versions apply -f versions.json --path ~/gitops/production

# Compare versions between environments
promrail versions diff --source ~/gitops/staging --dest ~/gitops/production

# Apply with conflict detection and snapshot
promrail versions apply -f versions.json --path ~/gitops/production \
  --check-conflicts --snapshot
```

### Snapshots

Create restore points before applying changes:

```bash
# List snapshots
promrail snapshot list --path ~/gitops/production

# Show snapshot details
promrail snapshot show <id> --path ~/gitops/production

# Rollback to a snapshot
promrail snapshot rollback <id> --path ~/gitops/production

# Delete a snapshot
promrail snapshot delete <id> --path ~/gitops/production
```

### Config Reference

View configuration documentation directly in the CLI:

```bash
# Show all configuration options
promrail config show

# Generate example configuration
promrail config example > promrail.yaml

# Generate to a file
promrail config example -o promrail.yaml
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
| `-s, --source <ENV>` | Source environment name |
| `-d, --dest <ENV>` | Destination environment name |
| `--no-delete` | Do not delete extra files in destination (delete is default) |
| `--dest-based` | Only operate on directories that exist in both environments |
| `--include-protected` | Include protected directories (custom, env, local) |
| `--dry-run` | Don't modify files (promote only) |
| `--diff` | Show file content changes (promote only) |
| `-y, --yes` | Skip confirmation prompt (promote only) |

## Configuration

Run `promrail config show` for embedded documentation, or `promrail config example` for a sample config file.

### Repositories

Define multiple GitOps repositories with their environments:

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

By default, promrail deletes files in destination that don't exist in source (matching the Python promote script):

```bash
# Default: delete extra files in destination
promrail promote --source staging --dest production --yes

# Keep extra files with --no-delete
promrail promote --source staging --dest production --no-delete --yes

# Dest-based: only delete if parent dir exists in source
promrail promote --source staging --dest production --dest-based --yes
```

## Architecture

See [docs/adr-001-architecture.md](docs/adr-001-architecture.md) for design decisions.

## Documentation

- [Usage Guide](docs/usage.md) - Detailed workflows and examples
- [Architecture Decision Record](docs/adr-001-architecture.md) - Design decisions
- `promrail config show` - Embedded configuration reference

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
