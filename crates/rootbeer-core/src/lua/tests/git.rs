//! Tests for `lua/rootbeer/git.lua`.

use crate::lua::test_support::run;
use crate::plan::Op;

fn writes(ops: &[Op]) -> Vec<(String, String)> {
    ops.iter()
        .filter_map(|op| match op {
            Op::WriteFile { path, content } => Some((path.display().to_string(), content.clone())),
            _ => None,
        })
        .collect()
}

fn find<'a>(writes: &'a [(String, String)], suffix: &str) -> &'a str {
    &writes
        .iter()
        .find(|(p, _)| p.ends_with(suffix))
        .unwrap_or_else(|| panic!("no write for {suffix}; got: {writes:?}"))
        .1
}

#[test]
fn git_config_writes_user_section() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = "Alice", email = "alice@example.com" },
        })
        "#);
    let writes = writes(&ops);
    let cfg = find(&writes, "gitconfig");
    assert!(cfg.contains("[user]"));
    assert!(cfg.contains(r#"name = "Alice""#));
    assert!(cfg.contains(r#"email = "alice@example.com""#));
}

#[test]
fn git_config_signing_emits_commit_tag_gpg() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = "A", email = "a@b" },
            signing = { key = "ssh-ed25519 AAA" },
        })
        "#);
    let writes = writes(&ops);
    let cfg = find(&writes, "gitconfig");
    assert!(cfg.contains(r#"signingkey = "ssh-ed25519 AAA""#));
    assert!(cfg.contains("[gpg]"));
    assert!(cfg.contains(r#"format = "ssh""#));
    assert!(cfg.contains("[commit]"));
    assert!(cfg.contains("gpgSign = true"));
    assert!(cfg.contains("[tag]"));
}

#[test]
fn git_config_writes_ignores_file_alongside_config() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = "A", email = "a@b" },
            ignores = { ".DS_Store", "*.swp" },
        })
        "#);
    let writes = writes(&ops);

    let ignore = find(&writes, ".gitignore");
    assert!(ignore.contains(".DS_Store"));
    assert!(ignore.contains("*.swp"));

    let cfg = find(&writes, "gitconfig");
    assert!(cfg.contains("excludesfile = "));
    assert!(cfg.contains("/tmp/rb-test/.gitignore"));
}

#[test]
fn git_config_lfs_emits_filter_lfs_subsection() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = "A", email = "a@b" },
            lfs = true,
        })
        "#);
    let writes = writes(&ops);
    let cfg = find(&writes, "gitconfig");
    assert!(cfg.contains(r#"[filter "lfs"]"#));
    assert!(cfg.contains("clean ="));
    assert!(cfg.contains("smudge ="));
}
