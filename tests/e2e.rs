//! End-to-end tests for prl
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

fn has_ruamel_yaml() -> bool {
    Command::new("python")
        .args(["-c", "import ruamel.yaml"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
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
"#;
        fs::write(&self.config_path, config).expect("Failed to write config");
    }

    fn create_multi_source_config(&self) {
        let config = r#"
version: 1

repos:
  test:
    path: .
    environments:
      staging-a:
        path: staging-a
      staging-b:
        path: staging-b
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

rules:
  conflict_resolution:
    config_strategy: source_priority
    source_order:
      - staging-b
      - staging-a

git:
  require_clean_tree: false
"#;
        fs::write(&self.config_path, config).expect("Failed to write config");
        fs::create_dir_all(self.repo_path.join("staging-a")).expect("Failed to create staging-a");
        fs::create_dir_all(self.repo_path.join("staging-b")).expect("Failed to create staging-b");
    }

    fn create_multi_source_config_with_review_rule(&self, component: &str) {
        self.create_multi_source_config();
        let mut config = fs::read_to_string(&self.config_path).expect("Failed to read config");
        let insert = format!(
            "\n  components:\n    {}:\n      action: review\n      notes: \"Needs review during promotion\"\n",
            component
        );
        if let Some(pos) = config.find("\ngit:") {
            config.insert_str(pos, &insert);
        }
        fs::write(&self.config_path, config).expect("Failed to update config");
    }

    fn create_multi_source_config_with_preserve_rule(
        &self,
        component: &str,
        file: &str,
        paths: &[&str],
    ) {
        self.create_multi_source_config();
        let mut config = fs::read_to_string(&self.config_path).expect("Failed to read config");
        let joined_paths = paths
            .iter()
            .map(|path| format!("            - {}\n", path))
            .collect::<String>();
        let insert = format!(
            "\n  components:\n    {}:\n      action: always\n      notes: \"Preserve destination-specific paths\"\n      preserve:\n        - file: {}\n          paths:\n{}",
            component, file, joined_paths
        );
        if let Some(pos) = config.find("\ngit:") {
            config.insert_str(pos, &insert);
        }
        fs::write(&self.config_path, config).expect("Failed to update config");
    }

    fn create_realistic_gitops_config(&self) {
        let homelab_path = self.repo_path.join("homelab");
        fs::create_dir_all(&homelab_path).expect("Failed to create homelab dir");

        let config = format!(
            r#"version: 1

repos:
  gitops:
    path: .
    environments:
      grigri-cloud: {{path: grigri.cloud}}
      nbg1-c01: {{path: nbg1-c01}}

  homelab:
    path: {homelab_path}
    environments:
      default: {{path: .}}

default_repo: gitops
default_sources:
  - grigri-cloud
  - homelab
default_dest: nbg1-c01

protected_dirs:
  - custom
  - env

allowlist:
  - "**/*.yaml"
  - "**/*.yml"
  - "**/*.json"

denylist:
  - "**/*secret*"
  - "**/secrets/**"
  - "**/charts/**"
  - "**/values-images.yaml"

rules:
  sources:
    grigri-cloud:
      priority: 1
      include:
        - "apps/*"
        - "platform/*"

    homelab:
      priority: 2
      include:
        - "platform/*"

  conflict_resolution:
    version_strategy: source_priority
    config_strategy: source_priority
    source_order:
      - homelab
      - grigri-cloud

  components:
    apps/landing:
      action: never
      notes: "Local only - nbg1-c01 specific"
    platform/headscale:
      action: never
      notes: "Local only - nbg1-c01 specific"
    apps/home-assistant:
      action: never
      notes: "Homelab specific - not for nbg1-c01"

  global:
    exclude:
      - "*/custom/*"
      - "*/env/*"
      - "*/charts/*"
    promote_options:
      allow_duplicates: false
      only_existing: true
      no_delete: true

git:
  require_clean_tree: false
"#,
            homelab_path = homelab_path.display()
        );

        fs::write(&self.config_path, config).expect("Failed to write realistic config");
        fs::create_dir_all(self.repo_path.join("grigri.cloud"))
            .expect("Failed to create grigri.cloud");
        fs::create_dir_all(self.repo_path.join("nbg1-c01")).expect("Failed to create nbg1-c01");
    }

    fn create_current_gitops_like_config(&self) {
        let homelab_path = self.repo_path.join("homelab");
        fs::create_dir_all(&homelab_path).expect("Failed to create homelab dir");

        let config = format!(
            r#"version: 1

repos:
  gitops:
    path: .
    environments:
      grigri-cloud: {{path: grigri.cloud}}
      nbg1-c01: {{path: nbg1-c01}}

  homelab:
    path: {homelab_path}
    environments:
      default: {{path: .}}

default_repo: gitops
default_sources:
  - grigri-cloud
  - homelab
default_dest: nbg1-c01

protected_dirs:
  - custom
  - env

allowlist:
  - "**/*.yaml"
  - "**/*.yml"
  - "**/*.json"

denylist:
  - "**/secret.yaml"
  - "**/secret.yml"
  - "**/secrets.yaml"
  - "**/secrets.yml"
  - "**/secret.json"
  - "**/secrets.json"
  - "**/secrets/**"
  - "**/charts/**"
  - "**/values-images.yaml"
  - "platform/velero/templates/cross-backup/**"
  - "platform/velero/templates/cross-backup-bis/**"
  - "platform/vault/templates/vault.yaml"

rules:
  sources:
    grigri-cloud:
      priority: 1
      include:
        - "apps/*"
        - "platform/*"

    homelab:
      priority: 2
      include:
        - "platform/*"

  conflict_resolution:
    version_strategy: source_priority
    config_strategy: source_priority
    source_order:
      - homelab
      - grigri-cloud

  components:
    apps/landing:
      action: never
      notes: "Local only - nbg1-c01 specific"
    platform/headscale:
      action: never
      notes: "Local only - nbg1-c01 specific"
    apps/home-assistant:
      action: never
      notes: "Homelab specific - not for nbg1-c01"

  global:
    exclude:
      - "*/custom/*"
      - "*/env/*"
      - "*/charts/*"
    promote_options:
      allow_duplicates: false
      only_existing: true
      no_delete: true

git:
  require_clean_tree: false
"#,
            homelab_path = homelab_path.display()
        );

        fs::write(&self.config_path, config).expect("Failed to write current gitops-like config");
        fs::create_dir_all(self.repo_path.join("grigri.cloud"))
            .expect("Failed to create grigri.cloud");
        fs::create_dir_all(self.repo_path.join("nbg1-c01")).expect("Failed to create nbg1-c01");
    }

    fn write_env_file(&self, env_root: &str, relative_path: &str, content: &str) {
        let path = self.repo_path.join(env_root).join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dir");
        }
        fs::write(&path, content).expect("Failed to write env file");
    }

    fn read_env_file(&self, env_root: &str, relative_path: &str) -> Option<String> {
        let path = self.repo_path.join(env_root).join(relative_path);
        if path.exists() {
            Some(fs::read_to_string(&path).expect("Failed to read env file"))
        } else {
            None
        }
    }

    fn review_artifact_path(&self) -> Option<PathBuf> {
        let review_dir = self.repo_path.join(".promrail/review");
        if !review_dir.exists() {
            return None;
        }

        fs::read_dir(review_dir)
            .expect("Failed to read review dir")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("yaml"))
    }

    fn classify_review_artifact(&self, decision: &str, selected_source: &str) {
        let path = self
            .review_artifact_path()
            .expect("Missing review artifact");
        let content = fs::read_to_string(&path).expect("Failed to read artifact");
        let mut doc: serde_yaml::Value =
            serde_yaml::from_str(&content).expect("Failed to parse artifact");

        doc["status"] = serde_yaml::Value::String("classified".to_string());

        if let Some(items) = doc
            .get_mut("items")
            .and_then(|value| value.as_sequence_mut())
        {
            for item in items {
                item["decision"] = serde_yaml::Value::String(decision.to_string());
                item["selected_source"] = serde_yaml::Value::String(selected_source.to_string());
            }
        }

        fs::write(
            &path,
            serde_yaml::to_string(&doc).expect("Failed to serialize artifact"),
        )
        .expect("Failed to write artifact");
    }

    fn read_snapshot_file(&self, env_root: &str) -> Option<String> {
        let path = self
            .repo_path
            .join(env_root)
            .join(".promotion-snapshots.yaml");
        if path.exists() {
            Some(fs::read_to_string(path).expect("Failed to read snapshot file"))
        } else {
            None
        }
    }

    fn snapshot_ids(&self, env_root: &str) -> Vec<String> {
        let content = self
            .read_snapshot_file(env_root)
            .expect("Snapshot file should exist");
        let doc: serde_yaml::Value = serde_yaml::from_str(&content).expect("Invalid snapshot yaml");
        doc.get("snapshots")
            .and_then(|value| value.as_sequence())
            .into_iter()
            .flatten()
            .filter_map(|snapshot| snapshot.get("id").and_then(|id| id.as_str()))
            .map(ToString::to_string)
            .collect()
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

    fn run_prl(&self, args: &[&str]) -> (bool, String, String) {
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

    let (success, _stdout, stderr) = repo.run_prl(&["--repo", "nonexistent", "diff"]);

    assert!(!success);
    assert!(stderr.contains("RepoNotFound") || stderr.contains("not found in config"));
}

#[test]
fn test_completions_command_outputs_shell_script() {
    let repo = TestRepo::new();

    let (success, stdout, _stderr) = repo.run_prl(&["completions", "bash"]);

    assert!(success, "{}", stdout);
    assert!(stdout.contains("prl"), "{}", stdout);
    assert!(
        stdout.contains("complete") || stdout.contains("compgen"),
        "{}",
        stdout
    );
}

#[test]
fn test_diff_shows_new_file() {
    let repo = TestRepo::new();
    repo.create_config();

    repo.write_staging_file("platform/config.yaml", "key: value\n");
    repo.commit_all("Add staging config");

    let (success, stdout, _stderr) =
        repo.run_prl(&["diff", "--source", "staging", "--dest", "production"]);

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
        repo.run_prl(&["diff", "--source", "staging", "--dest", "production"]);

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
        repo.run_prl(&["diff", "--source", "staging", "--dest", "production"]);

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

    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

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

    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

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

    let (success, _stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--dry-run",
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
        repo.run_prl(&["diff", "--source", "staging", "--dest", "staging"]);

    assert!(!success);
    assert!(stderr.contains("same") || stderr.contains("SameEnvironment"));
}

#[test]
fn test_invalid_environment_error() {
    let repo = TestRepo::new();
    repo.create_config();

    let (success, _stdout, stderr) =
        repo.run_prl(&["diff", "--source", "nonexistent", "--dest", "production"]);

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
    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

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
    let (success, _stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--no-delete",
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

    let (success, stdout, _stderr) = repo.run_prl(&[
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

    let (success, _stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--dest-based",
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
        repo.run_prl(&["diff", "--source", "staging", "--dest", "production"]);

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

    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

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
        repo.run_prl(&["diff", "--source", "staging", "--dest", "production"]);
    assert!(success);
    assert!(
        stdout.contains("0 files to copy"),
        "custom/ should be protected: {}",
        stdout
    );

    // With --include-protected: custom/ should be promoted
    let (success, stdout, _stderr) = repo.run_prl(&[
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
    let (success, _stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--include-protected",
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

    let (success, stdout, _stderr) = repo.run_prl(&[
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
    let (success, stdout, _stderr) = repo.run_prl(&[
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
        repo.run_prl(&["diff", "--source", "staging", "--dest", "production"]);

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
    let (success, stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging",
        "--dest",
        "production",
        "--diff",
        "--dry-run",
    ]);

    assert!(success, "Should succeed. stdout: {}", stdout);
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

    let (success, _stdout, _stderr) = repo.run_prl(&["--log-level", "debug", "config", "show"]);
    assert!(success);

    let (success, _stdout, _stderr) = repo.run_prl(&["--log-level", "error", "config", "show"]);
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

    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

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

    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

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

    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

    assert!(success);
    assert!(repo.production_file_exists("platform/a.yaml"));
    assert!(repo.production_file_exists("platform/b.yaml"));
    assert!(repo.production_file_exists("platform/c.yaml"));
}

// =============================================================================
// AUTO REVIEW ARTIFACT TESTS
// =============================================================================

#[test]
fn test_multi_source_new_component_creates_review_artifact() {
    let repo = TestRepo::new();
    repo.create_multi_source_config();

    repo.write_env_file("staging-a", "apps/demo/config.yaml", "name: demo\n");
    repo.commit_all("Add new demo app");

    let (success, stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    assert!(success);
    assert!(
        stdout.contains("Review required before promotion."),
        "{}",
        stdout
    );
    assert!(repo.review_artifact_path().is_some());
    assert_eq!(
        repo.read_env_file("production", "apps/demo/config.yaml"),
        None
    );
}

#[test]
fn test_multi_source_classified_artifact_is_consumed_automatically() {
    let repo = TestRepo::new();
    repo.create_multi_source_config();

    repo.write_env_file("staging-a", "apps/demo/config.yaml", "name: demo\n");
    repo.commit_all("Add new demo app");

    let (_success, _stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    repo.classify_review_artifact("promote", "staging-a");

    let (success, stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    assert!(success, "{}", stdout);
    assert_eq!(
        repo.read_env_file("production", "apps/demo/config.yaml"),
        Some("name: demo\n".to_string())
    );

    let artifact_path = repo.review_artifact_path().expect("Missing artifact");
    let artifact = fs::read_to_string(artifact_path).expect("Failed to read artifact");
    assert!(artifact.contains("status: applied"), "{}", artifact);
}

#[test]
fn test_multi_source_stale_artifact_requires_fresh_review() {
    let repo = TestRepo::new();
    repo.create_multi_source_config();

    repo.write_env_file("staging-a", "apps/demo/config.yaml", "name: demo\n");
    repo.commit_all("Add new demo app");

    let (_success, _stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    repo.classify_review_artifact("promote", "staging-a");
    repo.write_env_file(
        "staging-a",
        "apps/demo/config.yaml",
        "name: demo\nversion: 2\n",
    );

    let (success, stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    assert!(success);
    assert!(
        stdout.contains("Review required before promotion."),
        "{}",
        stdout
    );
    assert_eq!(
        repo.read_env_file("production", "apps/demo/config.yaml"),
        None
    );

    let artifact_path = repo.review_artifact_path().expect("Missing artifact");
    let artifact = fs::read_to_string(artifact_path).expect("Failed to read artifact");
    assert!(artifact.contains("status: pending"), "{}", artifact);
}

#[test]
fn test_multi_source_review_rule_creates_artifact_for_non_version_file() {
    let repo = TestRepo::new();
    repo.create_multi_source_config_with_review_rule("platform/demo");

    repo.write_env_file("production", "platform/demo/config.yaml", "mode: prod\n");
    repo.write_env_file("staging-a", "platform/demo/config.yaml", "mode: source\n");
    repo.commit_all("Add review-ruled component");

    let (success, stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    assert!(success, "{}", stdout);
    assert!(
        stdout.contains("Review required before promotion."),
        "{}",
        stdout
    );
    assert!(repo.review_artifact_path().is_some());
    assert_eq!(
        repo.read_env_file("production", "platform/demo/config.yaml"),
        Some("mode: prod\n".to_string())
    );
}

#[test]
fn test_multi_source_review_rule_can_be_promoted_without_selected_source_for_single_candidate() {
    let repo = TestRepo::new();
    repo.create_multi_source_config_with_review_rule("platform/demo");

    repo.write_env_file("production", "platform/demo/config.yaml", "mode: prod\n");
    repo.write_env_file("staging-a", "platform/demo/config.yaml", "mode: source\n");
    repo.commit_all("Add review-ruled component");

    let (_success, _stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    let path = repo
        .review_artifact_path()
        .expect("Missing review artifact");
    let content = fs::read_to_string(&path).expect("Failed to read artifact");
    let mut doc: serde_yaml::Value =
        serde_yaml::from_str(&content).expect("Failed to parse artifact");
    doc["status"] = serde_yaml::Value::String("classified".to_string());
    if let Some(items) = doc
        .get_mut("items")
        .and_then(|value| value.as_sequence_mut())
    {
        for item in items {
            item["decision"] = serde_yaml::Value::String("promote".to_string());
            item["selected_source"] = serde_yaml::Value::Null;
        }
    }
    fs::write(
        &path,
        serde_yaml::to_string(&doc).expect("Failed to serialize artifact"),
    )
    .expect("Failed to write artifact");

    let (success, stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    assert!(success, "{}", stdout);
    assert_eq!(
        repo.read_env_file("production", "platform/demo/config.yaml"),
        Some("mode: source\n".to_string())
    );
}

#[test]
fn test_multi_source_preserve_rule_keeps_destination_env_values() {
    let repo = TestRepo::new();
    repo.create_multi_source_config_with_preserve_rule(
        "platform/demo",
        "config.yaml",
        &["spec.origin", "spec.redirectUrl"],
    );

    repo.write_env_file(
        "production",
        "platform/demo/config.yaml",
        "spec:\n  origin: https://prod.example.com\n  redirectUrl:\n    - https://prod.example.com/callback\n  common: old\n",
    );
    repo.write_env_file(
        "staging-a",
        "platform/demo/config.yaml",
        "spec:\n  origin: https://source.example.com\n  redirectUrl:\n    - https://source.example.com/callback\n  common: new\n",
    );
    repo.commit_all("Add preserve rule fixture");

    let (success, stdout, stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    assert!(success, "stdout: {}\nstderr: {}", stdout, stderr);
    assert!(
        !stdout.contains("Review required before promotion."),
        "{}",
        stdout
    );
    let content = repo
        .read_env_file("production", "platform/demo/config.yaml")
        .expect("missing promoted config");
    let doc: serde_yaml::Value = serde_yaml::from_str(&content).expect("invalid yaml");
    assert_eq!(doc["spec"]["origin"], "https://prod.example.com");
    assert_eq!(
        doc["spec"]["redirectUrl"][0],
        "https://prod.example.com/callback"
    );
    assert_eq!(doc["spec"]["common"], "new");
    if has_ruamel_yaml() {
        assert!(content.contains("  redirectUrl:\n    - https://prod.example.com/callback\n"));
    }
}

#[test]
fn test_multi_source_conflicting_values_is_version_merged() {
    let repo = TestRepo::new();
    repo.create_multi_source_config();

    repo.write_env_file(
        "production",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.0.0\n",
    );
    repo.write_env_file(
        "staging-a",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.1.0\n",
    );
    repo.write_env_file(
        "staging-b",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.2.0\n",
    );
    repo.commit_all("Add api values files");

    let (success, stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    assert!(success, "{}", stdout);
    assert!(
        stdout.contains("files preserved for structured version merge"),
        "{}",
        stdout
    );
    assert!(repo.review_artifact_path().is_none());
}

#[test]
fn test_multi_source_identical_files_are_not_copied() {
    let repo = TestRepo::new();
    repo.create_multi_source_config();

    let content = "key: same\n";
    repo.write_env_file("production", "platform/demo/config.yaml", content);
    repo.write_env_file("staging-a", "platform/demo/config.yaml", content);
    repo.commit_all("Add identical multi-source file");

    let (success, stdout, _stderr) = repo.run_prl(&[
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);

    assert!(success, "{}", stdout);
    assert!(stdout.contains("No changes to promote"), "{}", stdout);
    assert!(!stdout.contains("Copied:"), "{}", stdout);
}

#[test]
fn test_multi_source_consecutive_runs_create_unique_snapshot_ids() {
    let repo = TestRepo::new();
    repo.create_multi_source_config();

    repo.write_env_file(
        "production",
        "platform/api/kustomization.yaml",
        "resources:\n  - deployment.yaml\n",
    );
    repo.write_env_file(
        "staging-a",
        "platform/api/kustomization.yaml",
        "resources:\n  - deployment.yaml\n  - service.yaml\n",
    );
    repo.write_env_file(
        "staging-b",
        "platform/api/kustomization.yaml",
        "resources:\n  - deployment.yaml\n  - service.yaml\n",
    );
    repo.commit_all("Add kustomization files for snapshot id test");

    let (success, stdout, stderr) = repo.run_prl(&[
        "--force",
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);
    assert!(success, "stdout: {}\nstderr: {}", stdout, stderr);
    // kustomization.yaml is version-managed, so it uses structured updates
    assert!(
        stdout.contains("files preserved for structured version merge"),
        "First run should preserve file for structured update: {}",
        stdout
    );

    repo.write_env_file(
        "staging-a",
        "platform/api/kustomization.yaml",
        "resources:\n  - deployment.yaml\n  - service.yaml\n  - configmap.yaml\n",
    );
    repo.write_env_file(
        "staging-b",
        "platform/api/kustomization.yaml",
        "resources:\n  - deployment.yaml\n  - service.yaml\n  - configmap.yaml\n",
    );
    repo.commit_all("Update kustomization files");

    let (success, stdout, stderr) = repo.run_prl(&[
        "--force",
        "promote",
        "--source",
        "staging-a",
        "--source",
        "staging-b",
        "--dest",
        "production",
    ]);
    assert!(success, "stdout: {}\nstderr: {}", stdout, stderr);

    let ids = repo.snapshot_ids("production");
    assert!(
        ids.len() >= 2,
        "expected at least two snapshots, got {:?}",
        ids
    );
    let last_two = &ids[ids.len() - 2..];
    assert_ne!(last_two[0], last_two[1], "snapshot ids should be unique");
}

#[test]
fn test_realistic_gitops_workflow_version_merges_conflicting_values() {
    let repo = TestRepo::new();
    repo.create_realistic_gitops_config();

    repo.write_env_file(
        "nbg1-c01",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.0.0\ningress:\n  host: api.nbg1.example.com\n",
    );
    repo.write_env_file(
        "grigri.cloud",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.1.0\ningress:\n  host: api.grigri.example.com\n",
    );
    repo.write_env_file(
        "homelab",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.2.0\ningress:\n  host: api.home.example.com\n",
    );
    repo.commit_all("Add realistic api manifests");

    let (success, stdout, _stderr) = repo.run_prl(&["promote"]);

    assert!(success, "{}", stdout);
    assert!(
        stdout.contains("files preserved for structured version merge"),
        "{}",
        stdout
    );
    assert!(repo.review_artifact_path().is_none());
}

#[test]
fn test_realistic_gitops_workflow_skips_new_components_with_only_existing() {
    let repo = TestRepo::new();
    repo.create_realistic_gitops_config();

    repo.write_env_file(
        "grigri.cloud",
        "apps/new-service/config.yaml",
        "name: new-service\n",
    );
    repo.commit_all("Add new service only in source");

    let (success, stdout, _stderr) = repo.run_prl(&["promote"]);

    assert!(success, "{}", stdout);
    assert!(
        !stdout.contains("Review required before promotion."),
        "{}",
        stdout
    );
    assert!(stdout.contains("No changes to promote"), "{}", stdout);
    assert!(repo.review_artifact_path().is_none());
    assert_eq!(
        repo.read_env_file("nbg1-c01", "apps/new-service/config.yaml"),
        None
    );
}

#[test]
fn test_realistic_gitops_workflow_ignores_values_images_denylist() {
    let repo = TestRepo::new();
    repo.create_realistic_gitops_config();

    repo.write_env_file(
        "nbg1-c01",
        "apps/calypso/values-images.yaml",
        "image:\n  tag: 1.0.0\n",
    );
    repo.write_env_file(
        "grigri.cloud",
        "apps/calypso/values-images.yaml",
        "image:\n  tag: 2.0.0\n",
    );
    repo.commit_all("Add denied values-images files");

    let (success, stdout, _stderr) = repo.run_prl(&["promote"]);

    assert!(success, "{}", stdout);
    assert!(stdout.contains("No changes to promote"), "{}", stdout);
    assert_eq!(
        repo.read_env_file("nbg1-c01", "apps/calypso/values-images.yaml"),
        Some("image:\n  tag: 1.0.0\n".to_string())
    );
}

#[test]
fn test_realistic_gitops_workflow_creates_review_for_existing_conflicting_file() {
    let repo = TestRepo::new();
    repo.create_realistic_gitops_config();

    repo.write_env_file("nbg1-c01", "platform/api/config.yaml", "mode: prod\n");
    repo.write_env_file("grigri.cloud", "platform/api/config.yaml", "mode: cloud\n");
    repo.write_env_file("homelab", "platform/api/config.yaml", "mode: home\n");
    repo.commit_all("Add conflicting config manifests");

    let (success, stdout, _stderr) = repo.run_prl(&["promote"]);

    assert!(success, "{}", stdout);
    assert!(
        stdout.contains("Review required before promotion."),
        "{}",
        stdout
    );
    assert!(repo.review_artifact_path().is_some());
    assert_eq!(
        repo.read_env_file("nbg1-c01", "platform/api/config.yaml"),
        Some("mode: prod\n".to_string())
    );
}

#[test]
fn test_realistic_gitops_workflow_does_not_warn_for_source_only_version_components() {
    let repo = TestRepo::new();
    repo.create_realistic_gitops_config();

    repo.write_env_file(
        "nbg1-c01",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.0.0\n",
    );
    repo.write_env_file(
        "grigri.cloud",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.1.0\n",
    );
    repo.write_env_file(
        "homelab",
        "platform/git/values.yaml",
        "image:\n  repository: ghcr.io/demo/git\n  tag: 9.9.9\n",
    );
    repo.commit_all("Add version files including source-only component");

    let (success, _stdout, stderr) = repo.run_prl(&["promote"]);

    assert!(success, "{}", stderr);
    assert!(
        !stderr.contains("Component directory not found"),
        "unexpected warning: {}",
        stderr
    );
}

#[test]
fn test_realistic_gitops_workflow_no_delete_does_not_record_deleted_files_in_snapshot() {
    let repo = TestRepo::new();
    repo.create_realistic_gitops_config();

    repo.write_env_file("nbg1-c01", "apps/landing/config.yaml", "keep: true\n");
    repo.write_env_file(
        "nbg1-c01",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.0.0\n",
    );
    repo.write_env_file(
        "grigri.cloud",
        "platform/api/values.yaml",
        "image:\n  repository: ghcr.io/demo/api\n  tag: 1.1.0\n",
    );
    repo.commit_all("Add files for snapshot no-delete test");

    let (success, stdout, _stderr) = repo.run_prl(&["promote"]);

    assert!(success, "{}", stdout);
    let snapshot = repo
        .read_snapshot_file("nbg1-c01")
        .expect("Snapshot file should exist");
    assert!(
        !snapshot.contains("files_deleted:"),
        "snapshot should omit files_deleted when no_delete is active: {}",
        snapshot
    );
    assert!(
        !snapshot.contains("apps/landing/config.yaml"),
        "snapshot should not record deletions when no_delete is active: {}",
        snapshot
    );
}

#[test]
fn test_current_gitops_like_workflow_promotes_external_secrets_version() {
    let repo = TestRepo::new();
    repo.create_current_gitops_like_config();

    repo.write_env_file(
        "nbg1-c01",
        "platform/external-secrets/kustomization.yaml",
        "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\nhelmCharts:\n  - name: external-secrets\n    version: 2.1.0\n",
    );
    repo.write_env_file(
        "homelab",
        "platform/external-secrets/kustomization.yaml",
        "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\nhelmCharts:\n  - name: external-secrets\n    version: 2.4.0\n",
    );
    repo.commit_all("Add external-secrets version bump fixture");

    let (success, stdout, _stderr) = repo.run_prl(&["promote", "external-secrets"]);

    assert!(success, "{}", stdout);
    let updated = repo
        .read_env_file("nbg1-c01", "platform/external-secrets/kustomization.yaml")
        .expect("missing kustomization");
    assert!(updated.contains("version: 2.4.0"), "{}", updated);
}

#[test]
fn test_current_gitops_like_workflow_denylist_does_not_block_external_secrets_component() {
    let repo = TestRepo::new();
    repo.create_current_gitops_like_config();

    repo.write_env_file(
        "homelab",
        "platform/external-secrets/resources/clustersecretstore.yaml",
        "kind: ClusterSecretStore\nmetadata:\n  name: external-secrets\n",
    );
    repo.write_env_file(
        "nbg1-c01",
        "platform/external-secrets/resources/clustersecretstore.yaml",
        "kind: ClusterSecretStore\nmetadata:\n  name: old\n",
    );
    repo.commit_all("Add external-secrets denylist fixture");

    let (success, stdout, _stderr) = repo.run_prl(&["promote", "external-secrets"]);

    assert!(success, "{}", stdout);
    let updated = repo
        .read_env_file(
            "nbg1-c01",
            "platform/external-secrets/resources/clustersecretstore.yaml",
        )
        .expect("missing clustersecretstore");
    assert!(updated.contains("name: external-secrets"), "{}", updated);
}

#[test]
fn test_current_gitops_like_workflow_denylists_vault_template_heavy_file() {
    let repo = TestRepo::new();
    repo.create_current_gitops_like_config();

    repo.write_env_file(
        "homelab",
        "platform/vault/templates/vault.yaml",
        "namespace: {{ .Release.Namespace }}\nvalue: homelab\n",
    );
    repo.write_env_file(
        "nbg1-c01",
        "platform/vault/templates/vault.yaml",
        "namespace: {{ .Release.Namespace }}\nvalue: prod\n",
    );
    repo.commit_all("Add vault template denylist fixture");

    let (success, stdout, _stderr) = repo.run_prl(&["promote", "vault"]);

    assert!(success, "{}", stdout);
    assert!(stdout.contains("No changes to promote"), "{}", stdout);
    let content = repo
        .read_env_file("nbg1-c01", "platform/vault/templates/vault.yaml")
        .expect("missing vault template");
    assert!(content.contains("value: prod"), "{}", content);
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
        repo.run_prl(&["diff", "--source", "staging", "--dest", "production"]);

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

    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

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

    let (success, _stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

    assert!(success);
    // File should remain unchanged
    assert_eq!(
        repo.read_production_file("platform/config.yaml"),
        Some(content.to_string())
    );
}

// =============================================================================
// SINGLE-SOURCE VERSION-MANAGED FILE TESTS
// =============================================================================

#[test]
fn test_single_source_kustomization_yaml_uses_structured_version_update() {
    let repo = TestRepo::new();
    repo.create_config();

    // Destination has kustomization.yaml with version 1.0.0 and env-specific resources
    repo.write_production_file(
        "platform/monitoring/kustomization.yaml",
        r#"helmCharts:
  - name: kube-prometheus-stack
    version: 1.0.0
    repo: https://prometheus-community.github.io/helm-charts
resources:
  - resources/etcd-secrets-updater.yaml
  - resources/env-specific-config.yaml
"#,
    );

    // Source has kustomization.yaml with version 2.0.0 and different resources
    repo.write_staging_file(
        "platform/monitoring/kustomization.yaml",
        r#"helmCharts:
  - name: kube-prometheus-stack
    version: 2.0.0
    repo: https://prometheus-community.github.io/helm-charts
resources:
  - resources/homelab-specific.yaml
"#,
    );
    repo.commit_all("Add kustomization files");

    let (success, stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

    assert!(success, "{}", stdout);
    // Version-managed files use structured updates
    assert!(
        stdout.contains("Updated versions: platform/monitoring/kustomization.yaml"),
        "Expected structured update message in stdout: {}",
        stdout
    );

    // Version should be updated, but resources should be preserved from destination
    let result = repo
        .read_production_file("platform/monitoring/kustomization.yaml")
        .expect("File should exist");
    assert!(
        result.contains("version: 2.0.0"),
        "Version should be updated: {}",
        result
    );
    assert!(
        result.contains("resources/etcd-secrets-updater.yaml"),
        "Destination resources should be preserved: {}",
        result
    );
    assert!(
        !result.contains("resources/homelab-specific.yaml"),
        "Source resources should NOT be added: {}",
        result
    );
}

#[test]
fn test_single_source_chart_yaml_uses_structured_version_update() {
    let repo = TestRepo::new();
    repo.create_config();

    // Destination has Chart.yaml with version 1.0.0 and env-specific config
    repo.write_production_file(
        "apps/myapp/Chart.yaml",
        r#"apiVersion: v2
name: myapp
version: 1.0.0
dependencies:
  - name: redis
    version: 10.0.0
    repository: https://charts.bitnami.com/bitnami
"#,
    );

    // Source has Chart.yaml with updated redis version
    repo.write_staging_file(
        "apps/myapp/Chart.yaml",
        r#"apiVersion: v2
name: myapp
version: 1.0.0
dependencies:
  - name: redis
    version: 11.0.0
    repository: https://charts.bitnami.com/bitnami
"#,
    );
    repo.commit_all("Add Chart files");

    let (success, stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

    assert!(success, "{}", stdout);
    // Version-managed files use structured updates
    assert!(
        stdout.contains("Updated versions: apps/myapp/Chart.yaml"),
        "Expected structured update message in stdout: {}",
        stdout
    );

    let result = repo
        .read_production_file("apps/myapp/Chart.yaml")
        .expect("File should exist");
    assert!(
        result.contains("version: 11.0.0"),
        "Redis version should be updated: {}",
        result
    );
}

#[test]
fn test_version_handling_whole_file_override() {
    let repo = TestRepo::new();

    // Create config with version_handling override
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

allowlist:
  - "**/*.yaml"

rules:
  components:
    platform/monitoring:
      action: always
      version_handling: whole_file

git:
  require_clean_tree: false
"#;
    fs::write(&repo.config_path, config).expect("Failed to write config");

    // Destination has kustomization.yaml with version 1.0.0 and env-specific resources
    repo.write_production_file(
        "platform/monitoring/kustomization.yaml",
        r#"helmCharts:
  - name: kube-prometheus-stack
    version: 1.0.0
resources:
  - resources/etcd-secrets-updater.yaml
"#,
    );

    // Source has kustomization.yaml with version 2.0.0 and different resources
    repo.write_staging_file(
        "platform/monitoring/kustomization.yaml",
        r#"helmCharts:
  - name: kube-prometheus-stack
    version: 2.0.0
resources:
  - resources/homelab-specific.yaml
"#,
    );
    repo.commit_all("Add kustomization files");

    let (success, stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

    assert!(success, "{}", stdout);

    // With whole_file override, entire file should be copied
    let result = repo
        .read_production_file("platform/monitoring/kustomization.yaml")
        .expect("File should exist");
    assert!(
        result.contains("version: 2.0.0"),
        "Version should be updated: {}",
        result
    );
    assert!(
        result.contains("resources/homelab-specific.yaml"),
        "Source resources should be copied with whole_file: {}",
        result
    );
    assert!(
        !result.contains("resources/etcd-secrets-updater.yaml"),
        "Destination resources should be replaced with whole_file: {}",
        result
    );
}

#[test]
fn test_single_source_values_yaml_uses_structured_version_update() {
    let repo = TestRepo::new();
    repo.create_config();

    // Destination has values.yaml with image tag 1.0.0 and env-specific ingress
    repo.write_production_file(
        "platform/api/values.yaml",
        r#"image:
  repository: ghcr.io/demo/api
  tag: 1.0.0
ingress:
  host: api.prod.example.com
  annotations:
    external-dns.alpha.kubernetes.io/target: prod.example.com
"#,
    );

    // Source has values.yaml with image tag 2.0.0 and different ingress
    repo.write_staging_file(
        "platform/api/values.yaml",
        r#"image:
  repository: ghcr.io/demo/api
  tag: 2.0.0
ingress:
  host: api.staging.example.com
  annotations:
    external-dns.alpha.kubernetes.io/target: staging.example.com
"#,
    );
    repo.commit_all("Add values files");

    let (success, stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

    assert!(success, "{}", stdout);
    // Version-managed files use structured updates
    assert!(
        stdout.contains("Updated versions: platform/api/values.yaml"),
        "Expected structured update message in stdout: {}",
        stdout
    );

    let result = repo
        .read_production_file("platform/api/values.yaml")
        .expect("File should exist");
    assert!(
        result.contains("tag: 2.0.0"),
        "Image tag should be updated: {}",
        result
    );
    assert!(
        result.contains("api.prod.example.com"),
        "Destination ingress host should be preserved: {}",
        result
    );
    assert!(
        !result.contains("api.staging.example.com"),
        "Source ingress host should NOT be copied: {}",
        result
    );
}

#[test]
fn test_new_component_kustomization_is_whole_file_copied() {
    // New components (not existing in destination) should still be whole-file copied
    let repo = TestRepo::new();
    repo.create_config();

    // Source has a new component with kustomization.yaml
    repo.write_staging_file(
        "platform/new-service/kustomization.yaml",
        r#"helmCharts:
  - name: myapp
    version: 1.0.0
resources:
  - resources/deployment.yaml
"#,
    );
    repo.commit_all("Add new service");

    let (success, stdout, _stderr) =
        repo.run_prl(&["promote", "--source", "staging", "--dest", "production"]);

    assert!(success, "{}", stdout);

    // New component should be copied entirely (structured update only applies to existing files)
    let result = repo
        .read_production_file("platform/new-service/kustomization.yaml")
        .expect("File should exist");
    assert!(
        result.contains("version: 1.0.0"),
        "Version should be present: {}",
        result
    );
    assert!(
        result.contains("resources/deployment.yaml"),
        "Resources should be copied for new component: {}",
        result
    );
}

// =============================================================================
// ERROR PATH TESTS
// =============================================================================

#[test]
fn test_config_not_found_error() {
    let repo = TestRepo::new();
    // Don't create config

    let (success, _stdout, stderr) = repo.run_prl(&["diff"]);

    assert!(!success);
    assert!(stderr.contains("Config") || stderr.contains("not found"));
}
