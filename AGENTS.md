# Opencode Guidelines for Promrail

This file provides instructions for opencode (AI assistant) when assisting with promotions using promrail.

## Overview

Promrail is a GitOps promotion tool that can:
- Extract versions from kustomization.yaml, Chart.yaml, and values.yaml
- Merge versions from multiple sources
- Apply versions with conflict detection and snapshots
- Rollback to previous states

## When Assisting with Promotions

### 1. Read the Configuration

Always read `promrail.yaml` first to understand:
- Repository structure and environments
- Allowlist/denylist patterns
- Protected directories
- **Promotion rules** (if defined)

```bash
promrail config show
```

### 2. Check for Rules

If `rules:` section exists in promrail.yaml, use it to guide decisions:

```yaml
rules:
  sources:
    staging-homelab:
      priority: 1
      include: [platform/*, system/monitoring/*]
      exclude: [platform/homeassistant/*]
    staging-work:
      priority: 2
      include: [apps/*, system/auth/*]

  components:
    platform/postgres-operator:
      action: always
    platform/homeassistant:
      action: never
      notes: "Home-specific, not for work production"
    system/auth/keycloak:
      action: review
      notes: "Check for env-specific realm configs"
```

### 3. Categorize Changes

For each change in a promotion:

| Action | Behavior |
|--------|----------|
| `always` | Include without question |
| `never` | Remove from changeset |
| `review` | Flag for human, include if reasonable |

If component is not in rules:
- Check global exclusions (custom/, env/, local/)
- Use judgment based on patterns
- Default: include

### 4. Common Patterns

**Always promote:**
- Version upgrades (unless downgrade)
- Bug fix versions (patch releases)
- Security patches

**Never promote:**
- Files in `custom/`, `env/`, `local/`
- Components marked as environment-specific
- Pre-release versions to production

**Review needed:**
- Major version upgrades (potential breaking changes)
- Changes to secrets/credentials
- New components not seen before

## Multi-Source Promotion Workflow

When promoting from multiple sources to one destination:

### Step 1: Extract from All Sources

```bash
promrail versions extract --path ~/gitops/staging-homelab -o /tmp/homelab-versions.json
promrail versions extract --path ~/gitops/staging-work -o /tmp/work-versions.json
```

### Step 2: Merge with Rules

```bash
promrail versions merge \
  --source ~/gitops/staging-homelab \
  --source ~/gitops/staging-work \
  --explain \
  -o /tmp/merged-versions.json
```

### Step 3: Review the Merge

Check the output for:
- Components removed due to `action: never`
- Warnings about version conflicts
- Components flagged for review

### Step 4: Apply with Snapshot

```bash
promrail versions apply \
  -f /tmp/merged-versions.json \
  --path ~/gitops/production \
  --check-conflicts \
  --snapshot
```

### Step 5: User Reviews

The user should:
1. Run `git diff` to review changes
2. Commit if acceptable
3. Rollback if needed: `promrail snapshot rollback <id> --path ~/gitops/production`

## Decision Process

When opencode needs to make decisions about promotion:

```
For each component change:
  1. Check if component is in rules
  2. If action=never → remove, explain why
  3. If action=always → include
  4. If action=review → evaluate:
     - Is it environment-specific?
     - Are there breaking changes?
     - Does it match patterns of valid changes?
  5. If not in rules → use default behavior (include)
  6. Output summary for user review
```

## Output Format

When generating final recommendations, output:

```
## Promotion Summary

### Applied Changes (X)
- platform/postgres-operator: 1.15.0 → 1.15.1 (always promote)
- system/grafana: 10.2.0 → 10.3.0 (security update)

### Removed Changes (Y)
- platform/homeassistant: 2026.1.0 → 2026.2.0 (never: home-specific)
- apps/internal-tool: 1.0.0 → 1.1.0 (never: internal tool)

### Needs Review (Z)
- system/keycloak: 22.0.0 → 23.0.0 (major version, check realm configs)

### Recommended Actions
1. Review keycloak changes for env-specific configs
2. Verify postgres-operator upgrade is compatible
3. Run: `promrail versions apply -f merged.json --path production --snapshot`
```

## Troubleshooting

### Version Downgrade Detected

If `--check-conflicts` fails:
1. Check if downgrade is intentional
2. If intentional: run without `--check-conflicts`
3. If not: investigate source of old version

### Missing Components

If expected component is not in merged output:
1. Check `rules.sources.*.exclude` patterns
2. Check `rules.components.*.action`
3. Check `rules.global.exclude`

### Snapshot Rollback

If promotion caused issues:
```bash
promrail snapshot list --path ~/gitops/production
promrail snapshot rollback <id> --path ~/gitops/production
git diff  # Verify rollback
git checkout -- .  # If needed
```

## Example: Complex Promotion

User request: "Promote changes from staging-homelab and staging-work to production"

Opencode should:

1. **Check config**
   ```bash
   promrail config show
   ```

2. **Extract versions**
   ```bash
   promrail versions extract --path ~/gitops/staging-homelab -o /tmp/homelab.json
   promrail versions extract --path ~/gitops/staging-work -o /tmp/work.json
   ```

3. **Merge with explanation**
   ```bash
   promrail versions merge \
     -s ~/gitops/staging-homelab \
     -s ~/gitops/staging-work \
     --explain \
     -o /tmp/merged.json
   ```

4. **Review and explain** to user:
   - What's being promoted
   - What's being excluded and why
   - What needs attention

5. **Apply** after user confirms:
   ```bash
   promrail versions apply \
     -f /tmp/merged.json \
     --path ~/gitops/production \
     --snapshot
   ```

6. **Remind user** to review with `git diff` and commit
