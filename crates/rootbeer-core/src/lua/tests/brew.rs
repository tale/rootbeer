//! Tests for `lua/rootbeer/brew.lua`.

use crate::lua::test_support::run;
use crate::plan::Op;

#[test]
fn brew_config_writes_brewfile_and_runs_bundle() {
    let ops = run(r#"
        local brew = require("rootbeer.brew")
        brew.config({
            path = "/tmp/rb-test/Brewfile",
            formulae = { "ripgrep", "fd" },
            casks = { "ghostty" },
            mas = { { name = "Xcode", id = 497799835 } },
        })
        "#);

    assert_eq!(ops.len(), 2, "expected write + exec, got {ops:?}");

    let Op::WriteFile { path, content } = &ops[0] else {
        panic!("expected WriteFile, got {:?}", ops[0]);
    };
    assert!(path.ends_with("Brewfile"));
    assert!(content.contains(r#"brew "ripgrep""#));
    assert!(content.contains(r#"brew "fd""#));
    assert!(content.contains(r#"cask "ghostty""#));
    assert!(content.contains(r#"mas "Xcode", id: 497799835"#));
    assert!(content.ends_with('\n'));

    let Op::Exec { cmd, args, .. } = &ops[1] else {
        panic!("expected Exec, got {:?}", ops[1]);
    };
    assert_eq!(cmd, "brew");
    assert_eq!(
        args,
        &vec!["bundle".to_string(), "--file=/tmp/rb-test/Brewfile".into()]
    );
}

#[test]
fn brew_config_with_only_formulae_omits_other_sections() {
    let ops = run(r#"
        local brew = require("rootbeer.brew")
        brew.config({
            path = "/tmp/rb-test/Brewfile",
            formulae = { "git" },
        })
        "#);
    let Op::WriteFile { content, .. } = &ops[0] else {
        panic!("expected WriteFile");
    };
    assert!(content.contains(r#"brew "git""#));
    assert!(!content.contains("cask"));
    assert!(!content.contains("mas "));
}
