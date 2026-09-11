//! Tests for `lua/rootbeer/git.lua`.

use crate::lua::test_support::run;
use crate::plan::Op;

fn writes(ops: &[Op]) -> Vec<(String, String)> {
    ops.iter()
        .filter_map(|op| match op {
            Op::WriteFile { path, source } => source
                .as_str()
                .map(|c| (path.display().to_string(), c.to_string())),
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

#[test]
fn git_config_extra_merges_into_shortcut_sections() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = "A", email = "a@b" },
            editor = "nvim",
            signing = { key = "ssh-ed25519 AAA" },
            ignores = { ".DS_Store" },
            extra = {
                core = { fsmonitor = true },
                tag = { sort = "-version:refname" },
                gpg = { ssh = { allowedSignersFile = "~/.ssh/allowed_signers" } },
            },
        })
        "#);
    let writes = writes(&ops);
    let cfg = find(&writes, "gitconfig");

    // extra.core must not drop what `editor` and `ignores` wrote.
    assert!(cfg.contains(r#"editor = "nvim""#));
    assert!(cfg.contains("excludesfile = "));
    assert!(cfg.contains("fsmonitor = true"));

    // extra.tag must not drop the signing shortcut's gpgSign.
    assert!(cfg.contains("gpgSign = true"));
    assert!(cfg.contains(r#"sort = "-version:refname""#));

    // extra.gpg adds a subsection without dropping the scalar format.
    assert!(cfg.contains(r#"format = "ssh""#));
    assert!(cfg.contains(r#"[gpg "ssh"]"#));
}

#[test]
fn git_config_extra_wins_on_conflict() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = "A", email = "a@b" },
            pull_rebase = true,
            extra = { pull = { rebase = false } },
        })
        "#);
    let writes = writes(&ops);
    let cfg = find(&writes, "gitconfig");
    assert!(cfg.contains("rebase = false"));
    assert!(!cfg.contains("rebase = true"));
}

#[test]
fn git_config_quotes_are_escaped() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = [[A "Quoted" B]], email = "a@b" },
            extra = { alias = { win = [[!f() { echo C:\temp; }; f]] } },
        })
        "#);
    let writes = writes(&ops);
    let cfg = find(&writes, "gitconfig");
    assert!(cfg.contains(r#"name = "A \"Quoted\" B""#));
    assert!(cfg.contains(r#"C:\\temp"#));
}

#[test]
fn git_config_allowed_signers_writes_file_and_wires_config() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = "A", email = "a@b" },
            signing = {
                key = "ssh-ed25519 AAA",
                allowed_signers = { "a@b", "a@work" },
                allowed_signers_path = "/tmp/rb-test/allowed_signers",
            },
        })
        "#);
    let writes = writes(&ops);

    let signers = find(&writes, "allowed_signers");
    assert_eq!(signers, "a@b ssh-ed25519 AAA\na@work ssh-ed25519 AAA\n");

    let cfg = find(&writes, "gitconfig");
    assert!(cfg.contains(r#"[gpg "ssh"]"#));
    assert!(cfg.contains(r#"allowedSignersFile = "/tmp/rb-test/allowed_signers""#));
}

#[test]
fn git_config_allowed_signers_skipped_for_non_ssh_format() {
    let ops = run(r#"
        local git = require("rootbeer.git")
        git.config({
            path = "/tmp/rb-test/gitconfig",
            user = { name = "A", email = "a@b" },
            signing = {
                key = "ABCD1234",
                format = "openpgp",
                allowed_signers = { "a@b" },
            },
        })
        "#);
    let writes = writes(&ops);
    assert!(!writes.iter().any(|(p, _)| p.ends_with("allowed_signers")));

    let cfg = find(&writes, "gitconfig");
    assert!(cfg.contains(r#"format = "openpgp""#));
    assert!(!cfg.contains("allowedSignersFile"));
}
