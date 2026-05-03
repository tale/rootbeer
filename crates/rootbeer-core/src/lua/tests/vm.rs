//! Tests for the require translator in `lua::vm`.
//!
//! The translator wraps Luau's native require so users can write standard
//! dot-separated paths like `require("rootbeer.git")` or
//! `require("./helper")`. It must also keep `@`-prefixed and `../`-prefixed
//! paths working for callers that already use Luau-native syntax.

use std::fs;

use crate::lua::test_support::vm_in;

#[test]
fn require_translates_rootbeer_dot_path() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = vm_in(
        r#"
        local git = require("rootbeer.git")
        assert(type(git) == "table", "rootbeer.git did not return a table")
        assert(type(git.config) == "function", "git.config missing")
        "#,
        tmp.path(),
    );
    drop(lua);
}

#[test]
fn require_translates_at_rootbeer_path() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = vm_in(
        r#"
        local zsh = require("@rootbeer/zsh")
        assert(type(zsh) == "table", "@rootbeer/zsh did not return a table")
        "#,
        tmp.path(),
    );
    drop(lua);
}

#[test]
fn require_translates_dot_slash_to_source_alias() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("helper.lua"), "return { value = 42 }\n").unwrap();

    let lua = vm_in(
        r#"
        local h = require("./helper")
        assert(h.value == 42, "expected 42, got " .. tostring(h.value))
        "#,
        tmp.path(),
    );
    drop(lua);
}

#[test]
fn require_translates_bare_name_to_source_alias() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("util.lua"), "return { ok = true }\n").unwrap();

    let lua = vm_in(
        r#"
        local u = require("util")
        assert(u.ok, "util module not loaded")
        "#,
        tmp.path(),
    );
    drop(lua);
}

#[test]
fn require_rootbeer_returns_global_api() {
    let tmp = tempfile::tempdir().unwrap();
    let lua = vm_in(
        r#"
        local mod = require("rootbeer")
        assert(type(mod.file) == "function", "rootbeer.file missing")
        assert(type(mod.json) == "table", "rootbeer.json missing")
        "#,
        tmp.path(),
    );
    drop(lua);
}
