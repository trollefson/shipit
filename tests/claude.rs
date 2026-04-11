use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn shipit(home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("shipit").unwrap();
    cmd.env("HOME", home.path());
    cmd.arg("claude");
    cmd
}

fn skill_path(fake_home: &TempDir) -> std::path::PathBuf {
    fake_home
        .path()
        .join(".claude")
        .join("skills")
        .join("shipit")
        .join("SKILL.md")
}

#[test]
fn claude_creates_skill_md() {
    let home = TempDir::new().unwrap();
    shipit(&home).assert().success();

    assert!(skill_path(&home).exists());
}

#[test]
fn claude_creates_intermediate_directories() {
    let home = TempDir::new().unwrap();
    shipit(&home).assert().success();

    assert!(home.path().join(".claude").join("skills").join("shipit").is_dir());
}

#[test]
fn claude_skill_content_contains_frontmatter() {
    let home = TempDir::new().unwrap();
    shipit(&home).assert().success();

    let content = std::fs::read_to_string(skill_path(&home)).unwrap();
    assert!(content.contains("name: shipit"));
    assert!(content.contains("disable-model-invocation: true"));
    assert!(content.contains("allowed-tools:"));
}

#[test]
fn claude_skill_content_contains_agent_guide() {
    let home = TempDir::new().unwrap();
    shipit(&home).assert().success();

    let content = std::fs::read_to_string(skill_path(&home)).unwrap();
    assert!(content.contains("shipit b2b plan"));
    assert!(content.contains("shipit b2t plan"));
    assert!(content.contains("plan_file"));
}

#[test]
fn claude_is_idempotent() {
    let home = TempDir::new().unwrap();

    shipit(&home).assert().success();
    shipit(&home).assert().success();

    let content = std::fs::read_to_string(skill_path(&home)).unwrap();
    assert_eq!(content.matches("name: shipit").count(), 1);
}

#[test]
fn claude_prints_path_to_stdout() {
    let home = TempDir::new().unwrap();
    shipit(&home)
        .assert()
        .success()
        .stdout(contains("SKILL.md"));
}

#[test]
fn claude_does_not_create_config() {
    let home = TempDir::new().unwrap();
    shipit(&home).assert().success();

    assert!(!home.path().join("shipit.toml").exists());
}

#[test]
fn claude_without_home_exits_nonzero() {
    Command::cargo_bin("shipit")
        .unwrap()
        .env_remove("HOME")
        .arg("claude")
        .assert()
        .failure()
        .stderr(contains("HOME"));
}
