//! End-to-end tests for promrail
//!
//! These tests verify the full promotion workflow against a temporary
//! git repository with staging and production environments.
//!
//! Tests are designed to replicate the behavior of the Python promote script
//! from /work/gitops/gitops-services/promote

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

struct TestRepo {
    repo_path: PathBuf,
    staging_path: PathBuf,
    production_path: PathBuf,
    config_path: PathBuf,
    _temp_dir: TempDir,
}

impl TestRepo {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let repo_path = temp_dir.path().to_path_buf();
        let staging_path = repo_path.join("staging");
        let production_path = repo_path.join("production");
        let config_path = repo_path.join("promrail.yaml");

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to init git repo");

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to config git");

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to config git");

        // Create environment directories
        fs::create_dir_all(&staging_path).expect("Failed to create staging dir");
        fs::create_dir_all(&production_path).expect("Failed to create production dir");

        Self {
            repo_path,
            staging_path,
            production_path,
            config_path,
            _temp_dir: temp_dir,
        }
    }

    fn create_config(&self) {
        let config = r#"
version: 1

repos:
  test:
    path: .
    environments:
      staging:
        path: staging
      production:
        path: production

default_repo: test

protected_dirs:
  - custom
  - env
  - local

allowlist:
  - "**/*.yaml"

denylist:
  - "**/secrets*"
  - "**/*secret*"

git:
  require_clean_tree: false

audit:
  enabled: false
"#;
        fs::write(&self.config_path, config).expect("Failed to write config");
    }

    fn write_staging_file(&self, relative_path: &str, content: &str) {
        let path = self.staging_path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dir");
        }
        fs::write(&path, content).expect("Failed to write staging file");
    }

    fn write_production_file(&self, relative_path: &str, content: &str) {
        let path = self.production_path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dir");
        }
        fs::write(&path, content).expect("Failed to write production file");
    }

    fn read_staging_file(&self, relative_path: &str) -> Option<String> {
        let path = self.staging_path.join(relative_path);
        if path.exists() {
            Some(fs::read_to_string(&path).expect("Failed to read file"))
        } else {
            None
        }
    }

    fn read_production_file(&self, relative_path: &str) -> Option<String> {
        let path = self.production_path.join(relative_path);
        if path.exists() {
            Some(fs::read_to_string(&path).expect("Failed to read file"))
        } else {
            None
        }
    }

    fn staging_file_exists(&self, relative_path: &str) -> bool {
        self.staging_path.join(relative_path).exists()
    }

    fn production_file_exists(&self, relative_path: &str) -> bool {
        self.production_path.join(relative_path).exists()
    }

    fn run_promrail(&self, args: &[&str]) -> (bool, String, String) {
        let binary = env!("CARGO_BIN_EXE_prl");

        let mut cmd = Command::new(binary);
        cmd.args(args)
            .current_dir(&self.repo_path)
            .env("PROMRAIL_CONFIG", &self.config_path);

        let output = cmd.output().expect("Failed to run prl");

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        (output.status.success(), stdout, stderr)
    }

    fn commit_all(&self, message: &str) {
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.repo_path)
            .output()
            .expect("Failed to git add");

        Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.repo_path)
            .output()
            .expect("Failed to git commit");
    }
}

// =============================================================================
// BASIC FUNCTIONALITY TESTS
// =============================================================================

#[test]
fn test_repo_not_found_error() {
    let repo = TestRepo::new();
    repo.create_config();

    let (success, _stdout, stderr) = repo.run_promrail(&["--repo", "nonexistent", "validate"]);

    assert!(!success);
    assert!(stderr.contains("RepoNotFound") || stderr.contains("not found in config"));
}

#[test]
fn test_diff_shows_new_file() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/config.yaml", "key: value\n");
    repo.commit_all("Add staging config");

    let (success, stdout, _stderr) =
        repo.run_promrail(&["diff", "--source", "staging", "--dest", "production"]);

    assert!(success);
    assert!(
        stdout.contains("1 files to copy") || stdout.contains("+ platform/config.yaml"),
        "Should show new file: {}",
        stdout
    );
}

#[test]
fn test_diff_shows_modified_file() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/config.yaml", "key: new-value\n");
    repo.write_production_file("platform/config.yaml", "key: old-value\n");
    repo.commit_all("Add configs");

    let (success, stdout, _stderr) =
        repo.run_promrail(&["diff", "--source", "staging", "--dest", "production"]);

    assert!(success);
    assert!(
        stdout.contains("1 files to copy") || stdout.contains("~ platform/config.yaml"),
        "Should show modified file: {}",
        stdout
    );
}

#[test]
fn test_diff_shows_no_changes_when_identical() {
    let repo = TestRepo::new();
    repo.create_config();

    let content = "key: value\n";
    repo.write_staging_file("platform/config.yaml", content);
    repo.write_production_file("platform/config.yaml", content);
    repo.commit_all("Add identical configs");

    let (success, stdout, _stderr) =
        repo.run_promrail(&["diff", "--source", "staging", "--dest", "production"]);

    assert!(success);
    assert!(
        stdout.contains("0 files to copy"),
        "Should show no changes: {}",
        stdout
    );
}

// =============================================================================
// PROMOTION TESTS
// =============================================================================

#[test]
fn test_promote_copies_new_file() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/config.yaml", "key: value\n");
    repo.commit_all("Add staging config");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    assert!(repo.production_file_exists("platform/config.yaml"));
    assert_eq!(
        repo.read_production_file("platform/config.yaml"),
        Some("key: value\n".to_string())
    );
}

#[test]
fn test_promote_updates_existing_file() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/config.yaml", "key: new-value\n");
    repo.write_production_file("platform/config.yaml", "key: old-value\n");
    repo.commit_all("Add configs");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    assert_eq!(
        repo.read_production_file("platform/config.yaml"),
        Some("key: new-value\n".to_string())
    );
}

#[test]
fn test_promote_dry_run_does_not_modify() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/config.yaml", "key: value\n");
    repo.commit_all("Add staging config");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--dry-run",
        "--yes",
    ]);

    assert!(success);
    assert!(
        !repo.production_file_exists("platform/config.yaml"),
        "Dry-run should not create files"
    );
}

// =============================================================================
// ERROR HANDLING TESTS
// =============================================================================

#[test]
fn test_same_environment_error() {
    let repo = TestRepo::new();
    repo.create_config();

    let (success, _stdout, stderr) =
        repo.run_promrail(&["diff", "--source", "staging", "--dest", "staging"]);

    assert!(!success);
    assert!(stderr.contains("same") || stderr.contains("SameEnvironment"));
}

#[test]
fn test_invalid_environment_error() {
    let repo = TestRepo::new();
    repo.create_config();

    let (success, _stdout, stderr) =
        repo.run_promrail(&["diff", "--source", "nonexistent", "--dest", "production"]);

    assert!(!success);
    assert!(stderr.contains("not found") || stderr.contains("EnvironmentNotFound"));
}

// =============================================================================
// DELETE BEHAVIOR TESTS
// =============================================================================

#[test]
fn test_delete_by_default_removes_extra_files() {
    let repo = TestRepo::new();
    repo.create_config();

    // Staging has no files, production has one
    repo.write_production_file("platform/old.yaml", "old: config\n");
    repo.commit_all("Add old config");

    // Delete is ON by default (no flag needed)
    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    assert!(
        !repo.production_file_exists("platform/old.yaml"),
        "Old file should be deleted by default"
    );
}

#[test]
fn test_no_delete_keeps_extra_files() {
    let repo = TestRepo::new();
    repo.create_config();

    // Staging has no files, production has one
    repo.write_production_file("platform/old.yaml", "old: config\n");
    repo.commit_all("Add old config");

    // With --no-delete flag, file should be kept
    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--no-delete",
        "--yes",
    ]);

    assert!(success);
    assert!(
        repo.production_file_exists("platform/old.yaml"),
        "Old file should NOT be deleted with --no-delete flag"
    );
}

// =============================================================================
// DEST-BASED BEHAVIOR TESTS
// =============================================================================

#[test]
fn test_dest_based_copy_only_to_existing_dirs() {
    let repo = TestRepo::new();
    repo.create_config();

    // Staging has platform/ and system/
    repo.write_staging_file("platform/config.yaml", "key: platform\n");
    repo.write_staging_file("system/config.yaml", "key: system\n");
    // Production only has platform/ (no system/)
    repo.write_production_file("platform/existing.yaml", "existing: true\n");
    repo.commit_all("Add configs");

    let (success, stdout, _stderr) = repo.run_promrail(&[
        "diff",
        "--source",
        "staging",
        "--dest",
        "production",
        "--dest-based",
    ]);

    assert!(success);
    // Should only copy platform (since system/ doesn't exist in production)
    assert!(
        stdout.contains("platform/config.yaml") || stdout.contains("1 files to copy"),
        "Should copy platform: {}",
        stdout
    );
    // system/ should not be copied because it doesn't exist in production
    assert!(
        !stdout.contains("system/config.yaml"),
        "system/ should be skipped (dest-based): {}",
        stdout
    );
}

#[test]
fn test_dest_based_delete_only_in_source_dirs() {
    let repo = TestRepo::new();
    repo.create_config();

    // Staging only has platform/
    repo.write_staging_file("platform/config.yaml", "key: platform\n");
    // Production has platform/ and system/ (system/ is extra)
    repo.write_production_file("platform/config.yaml", "key: platform\n");
    repo.write_production_file("system/old.yaml", "old: config\n");
    repo.commit_all("Add configs");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--dest-based",
        "--yes",
    ]);

    assert!(success);
    // system/old.yaml should NOT be deleted because system/ doesn't exist in staging
    assert!(
        repo.production_file_exists("system/old.yaml"),
        "system/old.yaml should not be deleted (dest-based)"
    );
}

// =============================================================================
// PROTECTED DIRECTORIES TESTS
// =============================================================================

#[test]
fn test_promote_respects_protected_dirs() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("custom/values.yaml", "custom: staging\n");
    repo.write_production_file("custom/values.yaml", "custom: production\n");
    repo.commit_all("Add custom configs");

    let (success, stdout, _stderr) =
        repo.run_promrail(&["diff", "--source", "staging", "--dest", "production"]);

    assert!(success);
    assert!(
        stdout.contains("0 files to copy"),
        "custom/ should be protected: {}",
        stdout
    );
}

#[test]
fn test_delete_respects_protected_dirs() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_production_file("custom/important.yaml", "important: data\n");
    repo.commit_all("Add important custom config");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    assert!(
        repo.production_file_exists("custom/important.yaml"),
        "Custom dir should not be deleted"
    );
}

#[test]
fn test_include_protected_allows_custom() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("custom/values.yaml", "custom: staging\n");
    repo.write_production_file("custom/values.yaml", "custom: production\n");
    repo.commit_all("Add custom configs");

    // Without --include-protected: custom/ should be skipped
    let (success, stdout, _stderr) =
        repo.run_promrail(&["diff", "--source", "staging", "--dest", "production"]);
    assert!(success);
    assert!(
        stdout.contains("0 files to copy"),
        "custom/ should be protected: {}",
        stdout
    );

    // With --include-protected: custom/ should be promoted
    let (success, stdout, _stderr) = repo.run_promrail(&[
        "diff",
        "--source",
        "staging",
        "--dest",
        "production",
        "--include-protected",
    ]);
    assert!(success);
    assert!(
        stdout.contains("1 files to copy") || stdout.contains("custom/values.yaml"),
        "custom/ should be included with flag: {}",
        stdout
    );

    // Promote with --include-protected
    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--include-protected",
        "--yes",
    ]);
    assert!(success);
    assert_eq!(
        repo.read_production_file("custom/values.yaml"),
        Some("custom: staging\n".to_string())
    );
}

// =============================================================================
// FILTER TESTS
// =============================================================================

#[test]
fn test_promote_with_filter() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/config.yaml", "key: platform\n");
    repo.write_staging_file("system/config.yaml", "key: system\n");
    repo.commit_all("Add configs");

    let (success, stdout, _stderr) = repo.run_promrail(&[
        "diff",
        "--source",
        "staging",
        "--dest",
        "production",
        "platform",
    ]);

    assert!(success);
    assert!(
        stdout.contains("1 files to copy") || stdout.contains("platform/config.yaml"),
        "Should only show platform: {}",
        stdout
    );
    assert!(!stdout.contains("system"), "system should be filtered out");
}

#[test]
fn test_regex_filter_support() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/app-a/config.yaml", "a: value\n");
    repo.write_staging_file("platform/app-b/config.yaml", "b: value\n");
    repo.write_staging_file("system/config.yaml", "system: value\n");
    repo.commit_all("Add configs");

    // Filter with regex: only app-a or app-b
    let (success, stdout, _stderr) = repo.run_promrail(&[
        "diff",
        "--source",
        "staging",
        "--dest",
        "production",
        "platform/app-[ab]/",
    ]);

    assert!(success);
    assert!(
        !stdout.contains("system/config.yaml"),
        "system/ should be filtered out: {}",
        stdout
    );
}

// =============================================================================
// DENYLIST TESTS
// =============================================================================

#[test]
fn test_promote_respects_denylist() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("secrets/api.yaml", "password: secret\n");
    repo.commit_all("Add secrets");

    let (success, stdout, _stderr) =
        repo.run_promrail(&["diff", "--source", "staging", "--dest", "production"]);

    assert!(success);
    assert!(
        stdout.contains("0 files to copy"),
        "Secrets should be excluded: {}",
        stdout
    );
    assert!(!repo.production_file_exists("secrets/api.yaml"));
}

// =============================================================================
// DIFF OUTPUT TESTS
// =============================================================================

#[test]
fn test_diff_flag_shows_file_content() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/config.yaml", "key: new-value\nother: data\n");
    repo.write_production_file("platform/config.yaml", "key: old-value\n");
    repo.commit_all("Add configs");

    // With --diff flag during promote, should show content changes
    let (success, stdout, stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--diff",
        "--dry-run",
        "--yes",
    ]);

    assert!(success, "Should succeed. stderr: {}", stderr);
    // Should show diff output (lines with + or -)
    assert!(
        stdout.contains("+") || stdout.contains("-") || stdout.contains("key:"),
        "Should show diff content: {}",
        stdout
    );
}

// =============================================================================
// LOG LEVEL TESTS
// =============================================================================

#[test]
fn test_log_level_option() {
    let repo = TestRepo::new();
    repo.create_config();

    let (success, _stdout, _stderr) = repo.run_promrail(&["--log-level", "debug", "validate"]);
    assert!(success);

    let (success, _stdout, _stderr) = repo.run_promrail(&["--log-level", "error", "validate"]);
    assert!(success);
}

// =============================================================================
// NESTED DIRECTORY TESTS
// =============================================================================

#[test]
fn test_promote_nested_directories() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/redis-operator/config.yaml", "redis: true\n");
    repo.write_staging_file("platform/redis-operator/values.yaml", "replicas: 3\n");
    repo.commit_all("Add nested configs");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    assert!(repo.production_file_exists("platform/redis-operator/config.yaml"));
    assert!(repo.production_file_exists("platform/redis-operator/values.yaml"));
}

#[test]
fn test_promote_creates_missing_directories() {
    let repo = TestRepo::new();
    repo.create_config();

    // Staging has deeply nested file, production doesn't have the directory
    repo.write_staging_file("platform/redis-operator/config.yaml", "redis: true\n");
    repo.commit_all("Add config");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    assert!(repo.production_file_exists("platform/redis-operator/config.yaml"));
}

// =============================================================================
// MULTIPLE FILE TESTS
// =============================================================================

#[test]
fn test_promote_multiple_files() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/a.yaml", "a: 1\n");
    repo.write_staging_file("platform/b.yaml", "b: 2\n");
    repo.write_staging_file("platform/c.yaml", "c: 3\n");
    repo.commit_all("Add multiple configs");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    assert!(repo.production_file_exists("platform/a.yaml"));
    assert!(repo.production_file_exists("platform/b.yaml"));
    assert!(repo.production_file_exists("platform/c.yaml"));
}

// =============================================================================
// EDGE CASES
// =============================================================================

#[test]
fn test_empty_source_directory() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_production_file("platform/config.yaml", "key: value\n");
    repo.commit_all("Add production config");

    let (success, stdout, _stderr) =
        repo.run_promrail(&["diff", "--source", "staging", "--dest", "production"]);

    assert!(success);
    assert!(stdout.contains("0 files to copy"));
}

#[test]
fn test_file_content_with_special_characters() {
    let repo = TestRepo::new();
    repo.create_config();

    let content = "key: \"value with 'quotes' and \\\"escapes\\\"\"\nspecial: \u{1F600}\n";
    repo.write_staging_file("platform/config.yaml", content);
    repo.commit_all("Add special content");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    assert_eq!(
        repo.read_production_file("platform/config.yaml"),
        Some(content.to_string())
    );
}

#[test]
fn test_overwrite_same_content_no_change() {
    let repo = TestRepo::new();
    repo.create_config();

    let content = "key: value\n";
    repo.write_staging_file("platform/config.yaml", content);
    repo.write_production_file("platform/config.yaml", content);
    repo.commit_all("Add same content");

    let (success, _stdout, _stderr) = repo.run_promrail(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--yes",
    ]);

    assert!(success);
    // File should remain unchanged
    assert_eq!(
        repo.read_production_file("platform/config.yaml"),
        Some(content.to_string())
    );
}

// =============================================================================
// ERROR PATH TESTS
// =============================================================================

#[test]
fn test_config_not_found_error() {
    let repo = TestRepo::new();
    // Don't create config

    let (success, _stdout, stderr) = repo.run_promrail(&["validate"]);

    assert!(!success);
    assert!(stderr.contains("Config") || stderr.contains("not found"));
}
