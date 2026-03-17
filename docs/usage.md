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
promrail validate

# 3. Preview changes
promrail diff --source staging --dest production

# 4. Apply changes
promrail promote --source staging --dest production --yes
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
promrail diff --repo work --source dev --dest prod
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `PROMRAIL_CONFIG` | Path to configuration file |
| `PROMRAIL_REPO` | Default repository name |

## Commands

### `promrail validate`

Validates the configuration file and checks that:
- All repositories exist
- All environments exist
- Allowlist patterns are valid
- Git repository is clean (if configured)

```bash
promrail validate

# With verbose output
promrail -v validate
```

### `promrail diff`

Shows what would change without applying:

```bash
# Diff all files
promrail diff --source staging --dest production

# Filter by path
promrail diff --source staging --dest production platform/nginx

# Multiple filters
promrail diff --source staging --dest production platform system

# Show what would be deleted
promrail diff --source staging --dest production --delete
```

Output symbols:
- `+` (green) - File will be added
- `~` (yellow) - File will be modified
- `-` (red) - File will be deleted (with `--delete`)

### `promrail promote`

Copies files from source to destination. **Delete is enabled by default** (files in destination that don't exist in source will be removed):

```bash
# Preview and confirm (deletes extra files by default)
promrail promote --source staging --dest production

# Skip confirmation
promrail promote --source staging --dest production --yes

# Dry run (no changes)
promrail promote --source staging --dest production --dry-run --yes

# Keep extra files (don't delete)
promrail promote --source staging --dest production --no-delete --yes

# Dest-based (only copy/delete in directories that exist in both environments)
promrail promote --source staging --dest production --dest-based --yes

# Show file content changes during promotion
promrail promote --source staging --dest production --diff --yes

# Include protected directories
promrail promote --source staging --dest production --include-protected --yes
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
promrail promote --source staging --dest production --dest-based --yes
```

This prevents:
1. Creating new directories in partial environments
2. Deleting files from directories that don't exist in source

### `--diff` Flag

Show file content changes during promotion:

```bash
promrail promote --source staging --dest production --diff --yes
```

Output includes unified diff with colored lines:
- Green (`+`): Added lines
- Red (`-`): Removed lines

### `--include-protected` Flag

Override protected directory exclusion at runtime:

```bash
# Normally custom/ is excluded, but with this flag it will be promoted
promrail promote --source staging --dest production --include-protected --yes
```

### `--log-level` Option

Control verbosity of output:

```bash
# Show debug messages
promrail --log-level debug diff --source staging --dest production

# Only show errors
promrail --log-level error promote --source staging --dest production --yes
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
promrail diff --source staging --dest production

# 4. Promote to production
promrail promote --source staging --dest production --yes

# 5. Commit promotion
git add clusters/production/
git commit -m "promote: nginx configuration to production"
git push
```

### Partial Promotion

Promote only specific components:

```bash
# Only platform components
promrail promote --source staging --dest production platform --yes

# Specific application
promrail promote --source staging --dest production apps/keycloak --yes
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
promrail promote --source dev --dest staging --yes

# Staging to production
promrail promote --source staging --dest production --yes
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
promrail -v diff --source staging --dest production
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