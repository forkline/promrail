# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [unreleased]

### Added
- Multi-repository support with explicit environment configuration
- Allowlist-based file selection with glob patterns
- Protected directories (custom, env, local) that are never modified during promotion
- Git-native operations using git2 crate for repository handling and status checks
- Commands: `diff`, `promote`, `validate`
- Audit logging to `.promotion-log.yaml`
- Colored diff output with safety defaults (dry-run by default, `--yes` required for promotion)
- Configuration file support (`promrail.yaml`)
- E2E tests for core promotion scenarios