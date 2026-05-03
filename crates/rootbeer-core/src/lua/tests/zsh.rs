//! Tests for `lua/rootbeer/zsh.lua`.

use crate::lua::test_support::run;
use crate::plan::Op;

/// Helper: collect all WriteFile ops as `(path-suffix, content)` pairs.
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
fn zsh_config_writes_bootstrap_and_zshrc() {
    let ops = run(r#"
        local zsh = require("rootbeer.zsh")
        zsh.config({
            keybind_mode = "emacs",
            options = { "CORRECT", "EXTENDED_GLOB" },
            env = { EDITOR = "nvim" },
            aliases = { g = "git" },
        })
        "#);

    let writes = writes(&ops);
    // Bootstrap, env, zshrc — at minimum.
    let bootstrap = find(&writes, ".zshenv");
    assert!(bootstrap.contains("ZDOTDIR=~/.config/zsh"));
    assert!(bootstrap.contains(". $ZDOTDIR/.zshenv"));

    let zshrc = find(&writes, ".zshrc");
    assert!(zshrc.contains("set -o emacs"));
    assert!(zshrc.contains("setopt CORRECT"));
    assert!(zshrc.contains("setopt EXTENDED_GLOB"));
    assert!(zshrc.contains("alias g='git'"));
}

#[test]
fn zsh_config_respects_custom_dir() {
    let ops = run(r#"
        local zsh = require("rootbeer.zsh")
        zsh.config({ dir = "/etc/zsh", env = { LANG = "C" } })
        "#);
    let writes = writes(&ops);
    let bootstrap = find(&writes, ".zshenv");
    assert!(bootstrap.contains("ZDOTDIR=/etc/zsh"));
    // env file lives under custom dir
    assert!(
        writes.iter().any(|(p, _)| p == "/etc/zsh/.zshenv"),
        "expected /etc/zsh/.zshenv, got {writes:?}"
    );
}

#[test]
fn zsh_history_block_includes_share_dedup_append_defaults() {
    let ops = run(r#"
        local zsh = require("rootbeer.zsh")
        zsh.config({ history = {} })
        "#);
    let writes = writes(&ops);
    let zshrc = find(&writes, ".zshrc");
    assert!(zshrc.contains("setopt SHARE_HISTORY"));
    assert!(zshrc.contains("setopt HIST_IGNORE_DUPS"));
    assert!(zshrc.contains("HISTFILE=$ZDOTDIR/.zsh_history"));
    assert!(zshrc.contains("HISTSIZE=10000"));
}

#[test]
fn zsh_evals_and_sources_are_emitted() {
    let ops = run(r#"
        local zsh = require("rootbeer.zsh")
        zsh.config({
            evals = { "mise activate zsh" },
            sources = { "~/.config/zsh/local.zsh" },
        })
        "#);
    let writes = writes(&ops);
    let zshrc = find(&writes, ".zshrc");
    assert!(zshrc.contains(r#"eval "$(mise activate zsh)""#));
    assert!(zshrc.contains("source ~/.config/zsh/local.zsh"));
}
