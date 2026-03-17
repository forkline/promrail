# GitHub Workflows

This document describes the CI/CD workflows and their setup requirements.

## Workflows Overview

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `rust.yml` | Push/PR to main, tags | Build, test, lint, release |
| `pre-commit.yml` | Push/PR to main | Run pre-commit hooks |
| `stale.yml` | Daily schedule | Close stale issues/PRs |
| `auto-tag.yaml` | Push to master | Auto-create git tags |
| `aur-publish.yml` | After Rust workflow | Publish to AUR |

## Secrets Required

### `aur-publish.yml`

Publish packages to Arch Linux User Repository (AUR).

| Secret | Description | How to Create |
|--------|-------------|---------------|
| `AUR_USERNAME` | Your AUR username | Register at https://aur.archlinux.org |
| `AUR_EMAIL` | Email for git commits | Any valid email |
| `AUR_SSH_PRIVATE_KEY` | SSH key for AUR | Generate with `ssh-keygen -t ed25519` |

**Setup:**
1. Create an AUR account at https://aur.archlinux.org/register
2. Generate SSH key: `ssh-keygen -t ed25519 -f ~/.ssh/aur -N ""`
3. Add public key to AUR: https://aur.archlinux.org/account (SSH Public Key field)
4. Add private key to GitHub: Repo → Settings → Secrets → New repository secret
   - Name: `AUR_SSH_PRIVATE_KEY`
   - Value: Contents of `~/.ssh/aur` (including `-----BEGIN` and `-----END` lines)

### `auto-tag.yaml`

Automatically create signed git tags from changelog versions.

| Secret | Description | How to Create |
|--------|-------------|---------------|
| `PAT` | Personal Access Token | GitHub Settings → Developer settings → Personal access tokens |
| `GPG_PRIVATE_KEY` | GPG key for signing tags | Generate with `gpg --full-generate-key` |

**Setup:**

1. **Create Personal Access Token (PAT):**
   - Go to GitHub → Settings → Developer settings → Personal access tokens → Tokens (classic)
   - Generate new token with scopes: `repo` (full control)
   - Add to repository secrets as `PAT`

2. **Create GPG Key:**
   ```bash
   # Generate GPG key
   gpg --full-generate-key
   # Choose: RSA and RSA, 4096 bits, key does not expire
   # Enter your name and email (must match GitHub email)

   # Export private key (for secret)
   gpg --armor --export-secret-keys YOUR_EMAIL@example.com > gpg-private-key.asc

   # Export public key (add to GitHub account)
   gpg --armor --export YOUR_EMAIL@example.com
   ```

3. **Add GPG key to GitHub:**
   - Go to GitHub → Settings → SSH and GPG keys → New GPG key
   - Paste the public key
   - Add private key to repository secrets as `GPG_PRIVATE_KEY`

### `rust.yml`

Build and release workflow. No secrets required - `GITHUB_TOKEN` is auto-provided.

| Secret | Description | Required |
|--------|-------------|----------|
| `GITHUB_TOKEN` | Auto-provided by GitHub | Auto |

### `pre-commit.yml`

No secrets required.

### `stale.yml`

Uses only `GITHUB_TOKEN` (auto-provided).

## Enabling/Disabling Workflows

### Disable AUR Publishing

If you don't need AUR packages:
1. Go to Actions → aur-publish
2. Click "Disable workflow"

Or remove the `aur-publish.yml` file.

### Disable Auto-Tagging

If you prefer manual tagging:
1. Go to Actions → Auto tag
2. Click "Disable workflow"

Or remove the `auto-tag.yaml` file.

## Workflow Dependencies

```
Push to main/master
    │
    ├──► rust.yml (test, lint, build)
    │
    ├──► pre-commit.yml (hooks)
    │
    └──► auto-tag.yaml (if version changed in CHANGELOG.md)
              │
              └──► Triggers rust.yml (release build on tag)
                        │
                        └──► aur-publish.yml (after release)
```

## Troubleshooting

### AUR Publish Fails with SSH Error

- Verify `AUR_SSH_PRIVATE_KEY` is the correct private key
- Check that the public key is added to your AUR account
- Ensure the key format includes the header and footer lines

### Auto-Tag Fails with Permission Error

- Verify `PAT` has `repo` scope
- Check that the token hasn't expired
- Ensure the token is for a user with write access to the repo

### Auto-Tag Fails with GPG Error

- Verify `GPG_PRIVATE_KEY` is the correct armored key
- Check that the GPG key's email matches your GitHub email
- Ensure the public key is added to your GitHub account

### Release Assets Not Created

- Check that the tag matches `v[0-9]*` pattern (e.g., `v1.0.0`)
- Verify CHANGELOG.md has an entry for the version
- Check rust.yml workflow logs for errors
