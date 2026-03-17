# ADR-001: Promrail Architecture

## Status

Accepted

## Context

We need a Rust-based GitOps promotion tool that:
- Promotes configuration files between environments (e.g., staging → production)
- Supports multiple git repositories with different structures
- Uses git-native features for diff, audit, and safety checks
- Excludes certain directories (custom, env, local) from promotion
- Allows flexible file selection via allowlist/denylist patterns

## Decision

### 1. Project Structure

```
promrail/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── cli.rs               # clap argument parsing
│   ├── config.rs            # YAML config loading
│   ├── error.rs             # Error types
│   ├── commands/            # Command implementations
│   ├── git/                 # Git operations
│   ├── files/               # File selection
│   └── audit/               # Promotion logging
└── promrail.yaml            # Configuration file
```

### 2. Multi-Repository Support

Configuration explicitly defines repositories and their environments:

```yaml
repos:
  gitops:
    path: ~/gitops
    environments:
      staging: { path: clusters/staging }
      production: { path: clusters/production }
```

Rationale: Different repositories have different structures. Explicit config prevents ambiguity.

### 3. File Selection Strategy

- **Allowlist-only**: Only explicitly allowed patterns are candidates for promotion
- **Denylist**: Additional exclusions (secrets, test files)
- **Protected dirs**: Immutable directories (custom, env, local) that are never touched

Rationale: Fail-safe approach. Better to miss a file than accidentally promote secrets.

### 4. Git-Native Operations

Uses `git2` crate for:
- Repository discovery and validation
- Tree cleanliness checks (`require_clean_tree`)
- Diff output (colored, unified format)
- Tracked file listing (only consider git-tracked files)
- Optional git notes for audit trail

Rationale: Git is the source of truth. Leverage its machinery instead of reimplementing.

### 5. Delete Behavior

Mirrors Python `promote` script:
- `--delete`: Enable deletion of files in dest that don't exist in source
- `--dest-based`: Only delete if parent directory exists in source
- Default: delete disabled (safe by default)

Rationale: Compatibility with existing workflow, configurable for different use cases.

### 6. Command Structure

```
promrail diff --source <env> --dest <env> [filter...]
promrail promote --source <env> --dest <env> [options]
promrail validate
```

Rationale: Clear separation between preview (diff) and action (promote). Validate for config checking.

### 7. Safety Defaults

- `diff` is the default safe operation
- `promote` requires `--yes` or interactive confirmation
- `--dry-run` available for additional safety
- Clean git tree required before promotion

Rationale: Prevent accidental changes. Mirror Python tool's safety features.

### 8. Audit Trail

Writes `.promotion-log.yaml` with:
- Timestamp, source/dest, user
- List of promoted, skipped, and protected files
- Git reference at time of promotion

Rationale: Traceability for compliance and debugging.

## Alternatives Considered

### 1. Single Repository Only
Rejected: User requires multi-repo support for different GitOps structures.

### 2. Auto-Discovery of Environments
Rejected: Explicit config prevents ambiguity and supports varied repo structures.

### 3. Full Mirror (always delete)
Rejected: Too destructive. Configurable delete matches Python tool behavior.

### 4. Go Implementation
Rejected: Rust provides better type safety and git2 bindings.

## Consequences

- Users must create `promrail.yaml` config before use
- Git repository required (no standalone file operations)
- Learning curve for glob patterns in allowlist/denylist
- Compatible with existing Python `promote` workflow

## Implementation Phases

1. Core: CLI, config, error types, git repository handling
2. File Selection: allowlist/denylist with globset
3. Commands: validate, diff, promote
4. Git Features: native diff, notes
5. Audit: promotion log

## References

- git2 crate: https://docs.rs/git2/
