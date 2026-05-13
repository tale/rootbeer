//! Tests for the `rb.profile` module (`crates/rootbeer-core/src/profile/`).

use std::fs;
use std::path::Path;

use crate::lua::test_support::{drain, vm_in_with_profile};
use crate::plan::Op;
use crate::profile::{self, NameError, ProfileError};

/// Run a snippet against a fresh VM, returning either the planned ops or
/// the structured `ProfileError` raised during exec.
fn run(source: &str, profile: Option<&str>) -> Result<Vec<Op>, ProfileError> {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_path_buf();
    let script_path = path.join("test.lua");
    let wrapped = format!("local rb = require(\"rootbeer\")\n{source}");
    fs::write(&script_path, &wrapped).unwrap();

    let runtime = crate::Runtime {
        script_dir: path.clone(),
        script_name: "test.lua".into(),
        lua_dir: std::path::PathBuf::from(env!("ROOTBEER_LUA_DIR")),
        profile: profile.map(str::to_owned),
    };
    let vm = crate::lua::Vm::new(runtime).unwrap();
    let chunk = format!("@{}", script_path.display());
    match vm.exec(&wrapped, &chunk) {
        Ok(()) => {
            std::mem::forget(tmp);
            Ok(drain(vm))
        }
        Err(e) => match profile::extract(&e) {
            Some(pe) => Err(pe),
            None => panic!("expected ProfileError, got: {e}"),
        },
    }
}

fn err(source: &str, profile: Option<&str>) -> ProfileError {
    run(source, profile).expect_err("expected ProfileError")
}

// ───────────────────────── module setup ──────────────────────────

#[test]
fn rb_profile_is_a_table() {
    let vm = vm_in_with_profile("assert(type(rb.profile) == 'table')", &tempdir(), None);
    drop(vm);
}

// ───────────────────────────── current ───────────────────────────

#[test]
fn current_is_nil_before_define() {
    let vm = vm_in_with_profile("result = rb.profile.current()", &tempdir(), None);
    let result: Option<String> = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, None);
}

#[test]
fn current_returns_cli_profile_before_define() {
    let vm = vm_in_with_profile(
        "result = rb.profile.current()",
        &tempdir(),
        Some("personal"),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "personal");
}

// ─────────────────────────── define shape ────────────────────────

#[test]
fn define_requires_strategy() {
    assert!(matches!(
        err(r#"rb.profile.define({ profiles = { work = {} } })"#, None),
        ProfileError::MissingField("strategy")
    ));
}

#[test]
fn define_requires_profiles() {
    assert!(matches!(
        err(r#"rb.profile.define({ strategy = "cli" })"#, Some("work")),
        ProfileError::MissingField("profiles")
    ));
}

#[test]
fn define_rejects_empty_profiles() {
    assert!(matches!(
        err(
            r#"rb.profile.define({ strategy = "cli", profiles = {} })"#,
            Some("work")
        ),
        ProfileError::EmptyProfiles
    ));
}

#[test]
fn define_rejects_unknown_strategy_string() {
    assert!(matches!(
        err(
            r#"rb.profile.define({ strategy = "magic", profiles = { work = {} } })"#,
            Some("work")
        ),
        ProfileError::InvalidStrategy(_)
    ));
}

#[test]
fn second_define_errors() {
    assert!(matches!(
        err(
            r#"
            rb.profile.define({ strategy = "cli", profiles = { work = {} } })
            rb.profile.define({ strategy = "cli", profiles = { other = {} } })
            "#,
            Some("work")
        ),
        ProfileError::AlreadyConfigured
    ));
}

// ─────────────────────────── name validation ─────────────────────

#[test]
fn rejects_hyphenated_name() {
    assert!(matches!(
        err(
            r#"rb.profile.define({ strategy = "cli", profiles = { ["my-profile"] = {} } })"#,
            None,
        ),
        ProfileError::InvalidName {
            reason: NameError::NotIdentifier,
            ..
        }
    ));
}

#[test]
fn rejects_leading_digit_name() {
    assert!(matches!(
        err(
            r#"rb.profile.define({ strategy = "cli", profiles = { ["1prod"] = {} } })"#,
            None,
        ),
        ProfileError::InvalidName {
            reason: NameError::NotIdentifier,
            ..
        }
    ));
}

#[test]
fn rejects_reserved_keyword_name() {
    assert!(matches!(
        err(
            r#"rb.profile.define({ strategy = "cli", profiles = { ["if"] = {} } })"#,
            None,
        ),
        ProfileError::InvalidName {
            reason: NameError::ReservedKeyword,
            ..
        }
    ));
}

#[test]
fn rejects_default_as_profile_name() {
    assert!(matches!(
        err(
            r#"rb.profile.define({ strategy = "cli", profiles = { default = {} } })"#,
            None,
        ),
        ProfileError::InvalidName {
            reason: NameError::ReservedKeyword,
            ..
        }
    ));
}

// ────────────────────── strategy = "cli" enforces flag ───────────

#[test]
fn cli_strategy_without_flag_errors() {
    assert!(matches!(
        err(
            r#"rb.profile.define({
                strategy = "cli",
                profiles = { work = {}, personal = {} },
            })"#,
            None,
        ),
        ProfileError::Required { active: None, .. }
    ));
}

#[test]
fn cli_flag_does_not_override_strategy() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = function() return "personal" end,
            profiles = { work = {}, personal = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        Some("work"),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "personal");
}

#[test]
fn cli_strategy_uses_flag() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = "cli",
            profiles = { work = {}, personal = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        Some("work"),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "work");
}

#[test]
fn cli_strategy_validates_flag_against_schema() {
    match err(
        r#"rb.profile.define({
            strategy = "cli",
            profiles = { work = {}, personal = {} },
        })"#,
        Some("wrok"),
    ) {
        ProfileError::Required { active, .. } => assert_eq!(active.as_deref(), Some("wrok")),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn ignored_cli_flag_is_not_validated_against_schema() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = function() return "personal" end,
            profiles = { work = {}, personal = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        Some("wrok"),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "personal");
}

// ────────────────────────── strategy = "hostname" ────────────────

#[test]
fn hostname_strategy_no_match_errors() {
    match err(
        r#"rb.profile.define({
            strategy = "hostname",
            profiles = {
                work = { "definitely-not-this-host" },
                personal = { "also-not-this-host" },
            },
        })"#,
        None,
    ) {
        ProfileError::NoMatch { strategy, .. } => assert_eq!(strategy, "hostname"),
        other => panic!("unexpected: {other:?}"),
    }
}

// ───────────────────────────── strategy = "user" ─────────────────

#[test]
fn user_strategy_no_match_errors() {
    match err(
        r#"rb.profile.define({
            strategy = "user",
            profiles = {
                work = { "nobody" },
                personal = { "alsonobody" },
            },
        })"#,
        None,
    ) {
        ProfileError::NoMatch { strategy, .. } => assert_eq!(strategy, "user"),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn user_strategy_matches_profile_strings() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = "user",
            profiles = { current = { rb.host.user }, other = { "nobody" } },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "current");
}

// ────────────────────────── strategy = "command" ─────────────────

#[test]
fn command_strategy_matches_executable_on_path() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = "command",
            profiles = { work = { "sh" }, personal = { "definitely-not-a-real-binary-xyz" } },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "work");
}

#[test]
fn command_strategy_no_match_errors() {
    match err(
        r#"rb.profile.define({
            strategy = "command",
            profiles = {
                work     = { "definitely-not-a-real-binary-xyz" },
                personal = { "another-fake-binary-abc" },
            },
        })"#,
        None,
    ) {
        ProfileError::NoMatch { strategy, .. } => assert_eq!(strategy, "command"),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn command_strategy_accepts_absolute_path() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = "command",
            profiles = { unix = { "/bin/sh" }, other = { "/no/such/binary-xyz" } },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "unix");
}

// ───────────────── empty matcher list = fallback profile ─────────

#[test]
fn empty_matcher_profile_is_fallback_for_command_strategy() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = "command",
            profiles = { work = { "definitely-not-a-real-binary-xyz" }, personal = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "personal");
}

#[test]
fn empty_matcher_profile_is_fallback_for_hostname_strategy() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = "hostname",
            profiles = { work = { "definitely-not-this-host" }, personal = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "personal");
}

#[test]
fn multiple_empty_matchers_disable_fallback() {
    // With cli strategy and two empty profiles, omitting `--profile` must
    // still error rather than ambiguously picking one as fallback.
    assert!(matches!(
        err(
            r#"rb.profile.define({
                strategy = "cli",
                profiles = { work = {}, personal = {} },
            })"#,
            None,
        ),
        ProfileError::Required { active: None, .. }
    ));
}

// ─────────────────────── function strategy + ctx ─────────────────

#[test]
fn function_strategy_resolves() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = function() return "server" end,
            profiles = { server = {}, dev = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "server");
}

#[test]
fn function_strategy_returning_nil_yields_nil_active() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = function() return nil end,
            profiles = { server = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: Option<String> = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, None);
}

#[test]
fn function_strategy_returning_nil_does_not_fall_back_to_cli() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = function() return nil end,
            profiles = { server = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        Some("server"),
    );
    let result: Option<String> = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, None);
}

#[test]
fn function_strategy_validates_returned_name() {
    match err(
        r#"rb.profile.define({
            strategy = function() return "wrok" end,
            profiles = { work = {}, personal = {} },
        })"#,
        None,
    ) {
        ProfileError::Required { active, .. } => assert_eq!(active.as_deref(), Some("wrok")),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn ctx_match_helpers_compose_into_a_chain() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = function(ctx)
                return ctx.hostname()
                    or ctx.user()
                    or "fallback"
            end,
            profiles = {
                work = { "x" },
                personal = { "y" },
                fallback = {},
            },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "fallback");
}

#[test]
fn ctx_can_use_cli_strategy_explicitly() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = function(ctx) return ctx.cli() or "personal" end,
            profiles = { work = {}, personal = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        Some("work"),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "work");
}

#[test]
fn ctx_cli_validates_flag_when_used() {
    match err(
        r#"rb.profile.define({
            strategy = function(ctx) return ctx.cli() or "personal" end,
            profiles = { work = {}, personal = {} },
        })"#,
        Some("wrok"),
    ) {
        ProfileError::Required { active, .. } => assert_eq!(active.as_deref(), Some("wrok")),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn ctx_match_checks_arbitrary_strings() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({
            strategy = function(ctx) return ctx.match("token") end,
            profiles = { work = { "token" }, personal = {} },
        })
        result = rb.profile.current()
        "#,
        &tempdir(),
        None,
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "work");
}

// ───────────────────────────── select / when ─────────────────────

#[test]
fn select_returns_value_for_active_profile() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({ strategy = "cli", profiles = { work = {}, personal = {} } })
        result = rb.profile.select({ default = "fallback", work = "yes" })
        "#,
        &tempdir(),
        Some("work"),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "yes");
}

#[test]
fn select_falls_back_to_default() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({ strategy = "cli", profiles = { work = {}, personal = {} } })
        result = rb.profile.select({ default = "fallback", work = "yes" })
        "#,
        &tempdir(),
        Some("personal"),
    );
    let result: String = vm.lua.globals().get("result").unwrap();
    assert_eq!(result, "fallback");
}

#[test]
fn select_requires_define_first() {
    assert!(matches!(
        err(r#"rb.profile.select({ default = "x" })"#, None),
        ProfileError::NotConfigured
    ));
}

#[test]
fn select_validates_keys_against_schema() {
    match err(
        r#"
        rb.profile.define({ strategy = "cli", profiles = { work = {}, personal = {} } })
        rb.profile.select({ work = 1, wrok = 2 })
        "#,
        Some("work"),
    ) {
        ProfileError::Required { active, .. } => assert_eq!(active.as_deref(), Some("wrok")),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn when_runs_for_matching_profile() {
    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({ strategy = "cli", profiles = { work = {}, personal = {} } })
        ran = false
        rb.profile.when("work", function() ran = true end)
        "#,
        &tempdir(),
        Some("work"),
    );
    let ran: bool = vm.lua.globals().get("ran").unwrap();
    assert!(ran);
}

#[test]
fn when_validates_name_against_schema() {
    match err(
        r#"
        rb.profile.define({ strategy = "cli", profiles = { work = {}, personal = {} } })
        rb.profile.when("wrok", function() end)
        "#,
        Some("work"),
    ) {
        ProfileError::Required { active, .. } => assert_eq!(active.as_deref(), Some("wrok")),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn when_requires_define_first() {
    assert!(matches!(
        err(r#"rb.profile.when("any", function() end)"#, None),
        ProfileError::NotConfigured
    ));
}

// ──────────────────────────────── config ─────────────────────────

fn setup_host_files(dir: &Path) {
    fs::create_dir_all(dir.join("hosts")).unwrap();
    fs::write(
        dir.join("hosts/work.lua"),
        "local rb = require(\"rootbeer\")\nrb.file(\"/tmp/rb-test/work.txt\", \"work\\n\")\n",
    )
    .unwrap();
    fs::write(
        dir.join("hosts/personal.lua"),
        "local rb = require(\"rootbeer\")\nrb.file(\"/tmp/rb-test/personal.txt\", \"personal\\n\")\n",
    )
    .unwrap();
}

#[test]
fn config_dispatches_to_active_profile() {
    let tmp = tempfile::tempdir().unwrap();
    setup_host_files(tmp.path());

    let vm = vm_in_with_profile(
        r#"
        rb.profile.define({ strategy = "cli", profiles = { work = {}, personal = {} } })
        rb.profile.config({
            work     = "hosts/work.lua",
            personal = "hosts/personal.lua",
        })
        "#,
        tmp.path(),
        Some("work"),
    );
    let ops = drain(vm);
    assert_eq!(ops.len(), 1);
    assert!(matches!(
        &ops[0],
        Op::WriteFile { path, .. } if path.ends_with("work.txt")
    ));
}

#[test]
fn config_requires_define_first() {
    assert!(matches!(
        err(
            r#"rb.profile.config({ work = "hosts/work.lua" })"#,
            Some("work")
        ),
        ProfileError::NotConfigured
    ));
}

fn tempdir() -> std::path::PathBuf {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_path_buf();
    std::mem::forget(tmp);
    path
}
