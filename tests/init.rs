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
fn does_not_create_claude_md_by_default() {
    let dir = TempDir::new().unwrap();
    shipit(&dir)
        .args(["--platform-domain", "github.com", "--platform-token", "ghp_test"])
        .assert()
        .success();

    assert!(!dir.path().join("CLAUDE.md").exists());
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

// --- --yes flag ---

#[test]
fn yes_uses_inferred_domain_and_env_token_without_prompting() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("origin", "https://github.com/owner/repo.git").unwrap();

    Command::cargo_bin("shipit")
        .unwrap()
        .env("GITHUB_TOKEN", "ghp_yes_test")
        .env_remove("GH_TOKEN")
        .env_remove("GITLAB_TOKEN")
        .env_remove("GITLAB_PRIVATE_TOKEN")
        .args(["init", "--dir", dir.path().to_str().unwrap(), "-y"])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.path().join("shipit.toml")).unwrap();
    assert!(content.contains("github.com"), "inferred domain should be written");
    assert!(content.contains("ghp_yes_test"), "env token should be written");
}

#[test]
fn yes_short_flag_works() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("origin", "https://github.com/owner/repo.git").unwrap();

    Command::cargo_bin("shipit")
        .unwrap()
        .env("GITHUB_TOKEN", "ghp_short_flag")
        .env_remove("GH_TOKEN")
        .env_remove("GITLAB_TOKEN")
        .env_remove("GITLAB_PRIVATE_TOKEN")
        .args(["init", "--dir", dir.path().to_str().unwrap(), "-y"])
        .assert()
        .success();

    assert!(dir.path().join("shipit.toml").exists());
}

#[test]
fn yes_with_no_remote_skips_domain_and_token() {
    let dir = TempDir::new().unwrap();
    git2::Repository::init(dir.path()).unwrap();

    Command::cargo_bin("shipit")
        .unwrap()
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .env_remove("GITLAB_TOKEN")
        .env_remove("GITLAB_PRIVATE_TOKEN")
        .args(["init", "--dir", dir.path().to_str().unwrap(), "-y"])
        .assert()
        .success();

    assert!(dir.path().join("shipit.toml").exists());
}

#[test]
fn yes_with_known_domain_and_missing_token_still_errors() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("origin", "https://github.com/owner/repo.git").unwrap();

    Command::cargo_bin("shipit")
        .unwrap()
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .env_remove("GITLAB_TOKEN")
        .env_remove("GITLAB_PRIVATE_TOKEN")
        .args(["init", "--dir", dir.path().to_str().unwrap(), "-y"])
        .assert()
        .failure();
}

#[test]
fn yes_explicit_flags_take_precedence_over_inferred() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("origin", "https://github.com/owner/repo.git").unwrap();

    Command::cargo_bin("shipit")
        .unwrap()
        .env("GITHUB_TOKEN", "ghp_env_token")
        .env_remove("GH_TOKEN")
        .env_remove("GITLAB_TOKEN")
        .env_remove("GITLAB_PRIVATE_TOKEN")
        .args([
            "init",
            "--dir", dir.path().to_str().unwrap(),
            "--platform-domain", "gitlab.com",
            "--platform-token", "glpat_explicit",
            "-y",
        ])
        .assert()
        .success();

    let content = std::fs::read_to_string(dir.path().join("shipit.toml")).unwrap();
    assert!(content.contains("gitlab.com"), "explicit domain should win over inferred");
    assert!(content.contains("glpat_explicit"), "explicit token should win over env");
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
