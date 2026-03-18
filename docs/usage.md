# Usage Guide

This guide covers common workflows and patterns for using promrail.

## Table of Contents

- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [Commands](#commands)
- [File Selection](#file-selection)
- [Workflows](#workflows)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)

## Quick Start

```bash
# 1. Create a configuration file
cat > promrail.yaml << 'EOF'
version: 1
repos:
  gitops:
    path: ~/gitops
    environments:
      staging: { path: clusters/staging }
      production: { path: clusters/production }
default_repo: gitops
allowlist:
  - "**/*.yaml"
protected_dirs:
  - custom
  - env
EOF

# 2. Validate configuration
prl validate

# 3. Preview changes
prl diff --source staging --dest production

# 4. Apply changes
prl promote --source staging --dest production
```

## Configuration

### Configuration File Locations

Promrail searches for configuration in this order:

1. `--config` flag or `PROMRAIL_CONFIG` environment variable
2. `promrail.yaml` in current directory
3. `promrail.yml` in current directory
4. `.promrail.yaml` in current directory
5. `.promrail.yml` in current directory

### Multi-Repository Setup

```yaml
version: 1

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

Use `--repo` to select a different repository:

```bash
prl diff --repo work --source dev --dest prod
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `PROMRAIL_CONFIG` | Path to configuration file |
| `PROMRAIL_REPO` | Default repository name |

## Commands

### `prl validate`

Validates the configuration file and checks that:
- All repositories exist
- All environments exist
- Allowlist patterns are valid
- Git repository is clean (if configured)

```bash
prl validate

# With verbose output
prl -v validate
```

### `prl diff`

Shows what would change without applying:

```bash
# Diff all files
prl diff --source staging --dest production

# Filter by path
prl diff --source staging --dest production platform/nginx

# Multiple filters
prl diff --source staging --dest production platform system

# Show what would be deleted
prl diff --source staging --dest production --delete
```

Output symbols:
- `+` (green) - File will be added
- `~` (yellow) - File will be modified
- `-` (red) - File will be deleted (with `--delete`)

### `prl promote`

Copies files from source to destination. **Delete is enabled by default** (files in destination that don't exist in source will be removed):

```bash
# Apply changes (deletes extra files by default)
prl promote --source staging --dest production

# With confirmation prompt
prl promote --source staging --dest production --confirm

# Dry run (no changes)
prl promote --source staging --dest production --dry-run

# Keep extra files (don't delete)
prl promote --source staging --dest production --no-delete

# Dest-based (only copy/delete in directories that exist in both environments)
prl promote --source staging --dest production --dest-based

# Show file content changes during promotion
prl promote --source staging --dest production --diff

# Include protected directories
prl promote --source staging --dest production --include-protected
```

### `--dest-based` Flag

The `--dest-based` flag is useful for **partial environments** - destinations that don't have all the components from the source.

**For copy**: Only copy files to directories that already exist in the destination.

**For delete**: Only delete files if their parent directory exists in the source.

Example scenario:
```
# Staging has: platform/, system/, apps/
# Production only has: platform/, system/  (no apps/)

# Without --dest-based: would try to copy apps/ to production
# With --dest-based: only copies platform/ and system/
prl promote --source staging --dest production --dest-based
```

This prevents:
1. Creating new directories in partial environments
2. Deleting files from directories that don't exist in source

### `--diff` Flag

Show file content changes during promotion:

```bash
prl promote --source staging --dest production --diff
```

Output includes unified diff with colored lines:
- Green (`+`): Added lines
- Red (`-`): Removed lines

### `--include-protected` Flag

Override protected directory exclusion at runtime:

```bash
# Normally custom/ is excluded, but with this flag it will be promoted
prl promote --source staging --dest production --include-protected
```

### `--log-level` Option

Control verbosity of output:

```bash
# Show debug messages
prl --log-level debug diff --source staging --dest production

# Only show errors
prl --log-level error promote --source staging --dest production
```

Levels: `error`, `warn`, `info` (default), `debug`, `trace`

## File Selection

### Allowlist Patterns

Only files matching allowlist patterns are considered for promotion:

```yaml
allowlist:
  # All YAML files under platform/
  - "platform/**/*.yaml"

  # All YAML files under system/
  - "system/**/*.yaml"

  # Specific app
  - "apps/myapp/**/*.yaml"

  # All YAML files at root
  - "*.yaml"
```

### Denylist Patterns

Denylist takes precedence over allowlist:

```yaml
denylist:
  # Any file with "secret" in the name
  - "**/*secret*"

  # Files in test directories
  - "**/test/**"

  # Specific files
  - "**/values-local.yaml"
```

### Protected Directories

These directories are never modified:

```yaml
protected_dirs:
  - custom     # Environment-specific customizations
  - env        # Environment configuration
  - local      # Local development overrides
  - test       # Test files
```

## Workflows

### Standard Promotion Flow

```bash
# 1. Make changes in staging
vim clusters/staging/platform/nginx/values.yaml

# 2. Commit changes
git add clusters/staging/
git commit -m "feat(nginx): update configuration"

# 3. Preview promotion
prl diff --source staging --dest production

# 4. Promote to production
prl promote --source staging --dest production

# 5. Commit promotion
git add clusters/production/
git commit -m "promote: nginx configuration to production"
git push
```

### Partial Promotion

Promote only specific components:

```bash
# Only platform components
prl promote --source staging --dest production platform

# Specific application
prl promote --source staging --dest production apps/keycloak
```

### Multi-Environment Promotion

```yaml
# promrail.yaml
repos:
  gitops:
    path: ~/gitops
    environments:
      dev: { path: clusters/dev }
      staging: { path: clusters/staging }
      production: { path: clusters/production }
```

```bash
# Dev to staging
prl promote --source dev --dest staging

# Staging to production
prl promote --source staging --dest production
```

## Examples

### Example 1: Basic GitOps Repository

```
gitops/
├── clusters/
│   ├── staging/
│   │   ├── platform/
│   │   │   └── nginx/
│   │   │       └── values.yaml
│   │   └── custom/
│   │       └── values.yaml  # Protected!
│   └── production/
│       ├── platform/
│       │   └── nginx/
│       │       └── values.yaml
│       └── custom/
│           └── values.yaml  # Protected!
└── promrail.yaml
```

```yaml
# promrail.yaml
version: 1
repos:
  gitops:
    path: .
    environments:
      staging: { path: clusters/staging }
      production: { path: clusters/production }
default_repo: gitops
allowlist:
  - "platform/**/*.yaml"
  - "system/**/*.yaml"
protected_dirs:
  - custom
```

### Example 2: Kustomize Overlays

```
gitops/
├── overlays/
│   ├── dev/
│   │   ├── kustomization.yaml
│   │   └── patches/
│   ├── staging/
│   │   ├── kustomization.yaml
│   │   └── patches/
│   └── production/
│       ├── kustomization.yaml
│       └── patches/
└── promrail.yaml
```

```yaml
# promrail.yaml
version: 1
repos:
  gitops:
    path: .
    environments:
      dev: { path: overlays/dev }
      staging: { path: overlays/staging }
      production: { path: overlays/production }
default_repo: gitops
allowlist:
  - "kustomization.yaml"
  - "patches/**/*.yaml"
protected_dirs:
  - local
```

## Troubleshooting

### "Git tree is not clean"

Set `require_clean_tree: false` in config, or commit your changes:

```yaml
git:
  require_clean_tree: false
```

### "No files matched allowlist patterns"

Check that your allowlist patterns are correct:

```bash
# Debug with verbose mode
prl -v diff --source staging --dest production
```

### "Environment not found"

Ensure environment names match your config:

```bash
# List environments
grep -A 10 "environments:" promrail.yaml
```

### Protected Files Are Being Promoted

Check for typos in `protected_dirs`:

```yaml
protected_dirs:
  - custom  # Not "Custom" or "CUSTOM"
```

### Denylist Not Working

Denylist patterns must match the full path:

```yaml
denylist:
  - "**/secrets*"    # Matches any file starting with "secrets"
  - "**/*secret*"    # Matches any file containing "secret"
```

## Version Management

Promrail can extract, compare, and apply Helm chart versions and container image tags across environments.

### `prl versions extract`

Extract versions from a repository path:

```bash
# Extract all versions
prl versions extract --path ~/gitops/staging

# Save to file
prl versions extract --path ~/gitops/staging -o versions.json

# Filter to specific components
prl versions extract --path ~/gitops/staging platform/nginx
```

The output is JSON with:
- `source_path`: Repository path
- `components`: Map of component path to versions
  - `helm_charts`: List of Helm chart versions from kustomization.yaml and Chart.yaml
  - `container_images`: List of container image tags from values.yaml

### `prl versions apply`

Apply versions from a file to a repository:

```bash
# Apply all versions
prl versions apply -f versions.json --path ~/gitops/production

# Dry run (preview changes)
prl versions apply -f versions.json --path ~/gitops/production --dry-run

# Filter to specific components
prl versions apply -f versions.json --path ~/gitops/production --component platform/nginx,system/redis

# Check for version downgrades
prl versions apply -f versions.json --path ~/gitops/production --check-conflicts

# Create a snapshot before applying
prl versions apply -f versions.json --path ~/gitops/production --snapshot
```

### `prl versions diff`

Compare versions between two repositories:

```bash
prl versions diff --source ~/gitops/staging --dest ~/gitops/production
```

Output shows version differences:
- Green: Version in destination
- Red: Version in source
- Yellow: Component name

## Snapshots

Snapshots record the state of a repository before applying changes, enabling rollback.

### Snapshot Storage

Snapshots are stored in `.promotion-snapshots.yaml` in the destination repository:

```yaml
snapshots:
  - id: snap_20260317_abc123
    created_at: "2026-03-17T10:00:00Z"
    source_path: ~/gitops/staging
    dest_path: ~/gitops/production
    status: Applied
    files_modified:
      - platform/nginx/kustomization.yaml
    version_changes:
      platform/nginx:
        - file: kustomization.yaml
          kind: HelmChart
          name: nginx
          before: "1.2.3"
          after: "1.3.0"
```

### `prl snapshot list`

List all snapshots:

```bash
prl snapshot list --path ~/gitops/production
```

### `prl snapshot show`

Show snapshot details:

```bash
prl snapshot show snap_20260317_abc123 --path ~/gitops/production
```

### `prl snapshot rollback`

Rollback to a snapshot:

```bash
prl snapshot rollback snap_20260317_abc123 --path ~/gitops/production
```

### `prl snapshot delete`

Delete a snapshot:

```bash
prl snapshot delete snap_20260317_abc123 --path ~/gitops/production
```

## Config Reference

View configuration documentation directly in the CLI:

### `prl config show`

Display all configuration options with descriptions, defaults, and examples:

```bash
prl config show
```

Output includes:
- Field names and types
- Descriptions from source code
- Default values
- Example values
- Environment variables

### `prl config example`

Generate a sample configuration file:

```bash
# Print to stdout
prl config example

# Save to file
prl config example -o promrail.yaml
```

### `prl config diff`

Compare configuration files between directories:

```bash
prl config diff ~/gitops/staging ~/gitops/production

# Filter to specific files
prl config diff ~/gitops/staging ~/gitops/production -f kustomization.yaml,values.yaml
```

## Workflows

### Version Promotion Workflow

```bash
# 1. Extract versions from staging
prl versions extract --path ~/gitops/staging -o staging-versions.json

# 2. Review the versions
cat staging-versions.json | jq

# 3. Compare with production
prl versions diff --source ~/gitops/staging --dest ~/gitops/production

# 4. Apply with conflict detection and snapshot
prl versions apply -f staging-versions.json --path ~/gitops/production \
  --check-conflicts --snapshot

# 5. If something goes wrong, rollback
prl snapshot list --path ~/gitops/production
prl snapshot rollback <snapshot-id> --path ~/gitops/production
```

### Cross-Repository Promotion

For promoting between separate repositories:

```bash
# Extract from source repo
prl versions extract --path ~/gitops-apps/staging -o versions.json

# Apply to destination repo
prl versions apply -f versions.json --path ~/gitops-infra/production
```

## Multi-Source Promotion

Promote from multiple staging sources to a single production environment.

### Configuration

Define promotion rules in `promrail.yaml`:

```yaml
rules:
  sources:
    staging-homelab:
      priority: 1
      description: "Homelab staging environment"
      include:
        - platform/*
        - system/monitoring/*
      exclude:
        - platform/homeassistant/*

    staging-work:
      priority: 2
      description: "Work staging environment"
      include:
        - apps/*
        - system/auth/*

  conflict_resolution:
    version_strategy: highest
    config_strategy: source_priority
    source_order:
      - staging-work
      - staging-homelab

  components:
    platform/postgres-operator:
      action: always
    platform/homeassistant:
      action: never
      notes: "Home-specific"
    system/auth/keycloak:
      action: review
      notes: "Check for env-specific configs"

  global:
    exclude:
      - "*/custom/*"
      - "*/env/*"
    version_rules:
      allow_downgrade: false
      allow_prerelease: false
```

### Workflow

```bash
# 1. Merge versions from multiple sources
prl versions merge \
  --source ~/gitops/staging-homelab \
  --source ~/gitops/staging-work \
  --explain \
  -o merged-versions.json

# 2. Review the merge output
# - Check removed components
# - Check warnings
# - Check items needing review

# 3. Apply merged versions
prl versions apply \
  -f merged-versions.json \
  --path ~/gitops/production \
  --check-conflicts \
  --snapshot

# 4. Review and commit
git diff
git add -A
git commit -m "promote: multi-source version updates"
```

### Automation Script

Use the provided script for non-interactive promotion:

```bash
# Create sources file
cat > sources.txt << EOF
~/gitops/staging-homelab
~/gitops/staging-work
EOF

# Run promotion
./scripts/promote-complex.sh \
  --sources sources.txt \
  --dest ~/gitops/production

# Dry run first
./scripts/promote-complex.sh \
  --sources sources.txt \
  --dest ~/gitops/production \
  --dry-run
```

### Opencode Integration

When using opencode AI assistant, it will:

1. Read `promrail.yaml` rules automatically
2. Apply `action: always` without question
3. Remove `action: never` components
4. Flag `action: review` items for your attention

See [AGENTS.md](../AGENTS.md) for guidelines.
