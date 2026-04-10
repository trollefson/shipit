use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn shipit(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("shipit").unwrap();
    // Clear token env vars so tests are not affected by the developer's environment
    cmd.env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .env_remove("GITLAB_TOKEN")
        .env_remove("GITLAB_PRIVATE_TOKEN")
        .args(["init", "--dir", dir.path().to_str().unwrap()]);
    cmd
}

// --- guide-only ---

#[test]
fn guide_only_creates_claude_md() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .arg("--guide-only")
        .assert()
        .success();

    assert!(dir.path().join("CLAUDE.md").exists());
}

#[test]
fn guide_only_does_not_create_config() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .arg("--guide-only")
        .assert()
        .success();

    assert!(!dir.path().join("shipit.toml").exists());
}

#[test]
fn guide_only_updates_existing_claude_md_section_in_place() {
    let dir = TempDir::new().unwrap();

    // Run once to create the initial CLAUDE.md
    shipit(&dir).arg("--guide-only").assert().success();

    let after_first = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert_eq!(after_first.matches("<!-- shipit:start -->").count(), 1);

    // Run again — section should be replaced, not duplicated
    shipit(&dir).arg("--guide-only").assert().success();

    let after_second = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert_eq!(
        after_second.matches("<!-- shipit:start -->").count(),
        1,
        "shipit section should not be duplicated on re-run"
    );
}

#[test]
fn guide_only_preserves_existing_claude_md_content() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("CLAUDE.md"),
        "# My Project\n\nSome existing content.\n",
    )
    .unwrap();

    shipit(&dir).arg("--guide-only").assert().success();

    let content = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(content.contains("# My Project"));
    assert!(content.contains("Some existing content."));
    assert!(content.contains("<!-- shipit:start -->"));
}

// --- happy path ---

#[test]
fn creates_config_with_explicit_domain_and_token() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .args(["--platform-domain", "github.com", "--platform-token", "ghp_test"])
        .assert()
        .success();

    assert!(dir.path().join("shipit.toml").exists());
}

#[test]
fn config_contains_provided_domain_and_token() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .args(["--platform-domain", "github.com", "--platform-token", "ghp_test"])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.path().join("shipit.toml")).unwrap();
    assert!(content.contains("github.com"), "domain should be in config");
    assert!(content.contains("ghp_test"), "token should be in config");
}

#[test]
fn creates_plans_directory() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .args(["--platform-domain", "github.com", "--platform-token", "ghp_test"])
        .assert()
        .success();

    assert!(dir.path().join(".shipit").join("plans").is_dir());
}

#[test]
fn creates_claude_md() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .args(["--platform-domain", "github.com", "--platform-token", "ghp_test"])
        .assert()
        .success();

    assert!(dir.path().join("CLAUDE.md").exists());
}

#[test]
fn creates_gitignore_with_shipit_entries() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .args(["--platform-domain", "github.com", "--platform-token", "ghp_test"])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(content.contains("shipit.toml"));
    assert!(content.contains(".shipit/"));
}

#[test]
fn appends_to_existing_gitignore() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join(".gitignore"), "node_modules/\n.env\n").unwrap();

    shipit(&dir)
        .args(["--platform-domain", "github.com", "--platform-token", "ghp_test"])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(content.contains("node_modules/"), "existing entries should be preserved");
    assert!(content.contains(".env"), "existing entries should be preserved");
    assert!(content.contains("shipit.toml"));
    assert!(content.contains(".shipit/"));
}

// --- config already exists ---

#[test]
fn does_not_overwrite_existing_config() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("shipit.toml");
    std::fs::write(&config_path, "# existing config\n").unwrap();

    shipit(&dir)
        .args(["--platform-domain", "github.com", "--platform-token", "ghp_test"])
        .assert()
        .success();

    let content = std::fs::read_to_string(&config_path).unwrap();
    assert_eq!(content, "# existing config\n", "existing config should not be overwritten");
}

// --- error cases ---

#[test]
fn known_domain_without_token_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .arg("--platform-domain")
        .arg("github.com")
        .assert()
        .failure()
        .stderr(contains("GITHUB_TOKEN").or(contains("GH_TOKEN")));
}

#[test]
fn gitlab_domain_without_token_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .arg("--platform-domain")
        .arg("gitlab.com")
        .assert()
        .failure()
        .stderr(contains("GITLAB_TOKEN").or(contains("GITLAB_PRIVATE_TOKEN")));
}
