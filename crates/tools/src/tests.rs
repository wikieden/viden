use super::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

mod capability_tests;
mod file_tests;
mod git_tests;
mod interceptor_tests;
mod lane_tests;
mod lsp_tests;
mod patch_conflict_tests;
mod patch_document_tests;
mod shell_tests;
mod web_tests;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "viden_tools_{name}_{}",
        viden_types::fresh_id("tmp")
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git_init(cwd: &PathBuf) {
    let init = Command::new("git")
        .arg("init")
        .arg("-b")
        .arg("main")
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(init.success());
}

fn git_config_identity(cwd: &PathBuf) {
    let email = Command::new("git")
        .args(["config", "user.email", "viden@example.com"])
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(email.success());
    let name = Command::new("git")
        .args(["config", "user.name", "Viden"])
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(name.success());
}

fn git_add(cwd: &PathBuf, path: &str) {
    let add = Command::new("git")
        .args(["add", path])
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(add.success());
}

fn git_commit(cwd: &PathBuf, message: &str) {
    let commit = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(commit.success());
}

fn git_repo_with_tracked_file(name: &str) -> PathBuf {
    let cwd = temp_dir(name);
    git_init(&cwd);
    git_config_identity(&cwd);
    fs::write(cwd.join("tracked.txt"), "first\n").unwrap();
    git_add(&cwd, "tracked.txt");
    git_commit(&cwd, "initial");
    cwd
}
