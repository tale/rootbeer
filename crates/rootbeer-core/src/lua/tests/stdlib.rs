//! Tests for the small Lua stdlib helpers in `lua/rootbeer/str.lua` and
//! `lua/rootbeer/tbl.lua`. These don't push ops — they're pure utilities —
//! so the tests poke values into Lua globals and read them back.

use crate::lua::test_support::vm_in;

/// Run a Lua snippet rooted at a tempdir and read back a global string.
fn eval_string(snippet: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let vm = vm_in(snippet, tmp.path());
    vm.lua.globals().get("result").unwrap()
}

#[test]
fn str_split_lines_preserves_empty_lines() {
    let result = eval_string(
        r#"
        local str = require("rootbeer.str")
        result = table.concat(str.split_lines("a\n\nb\n\nc"), "|")
        "#,
    );
    assert_eq!(result, "a||b||c");
}

#[test]
fn str_split_lines_strips_single_trailing_newline() {
    let result = eval_string(
        r#"
        local str = require("rootbeer.str")
        result = tostring(#str.split_lines("a\nb\n"))
        "#,
    );
    assert_eq!(result, "2");
}

#[test]
fn str_split_lines_keeps_trailing_blanks_when_explicit() {
    // "a\n\n" → ["a", ""]. The single trailing newline is normalized
    // away (so "a\n" -> ["a"]) but a deliberate trailing blank line
    // should survive.
    let result = eval_string(
        r#"
        local str = require("rootbeer.str")
        local parts = str.split_lines("a\n\n")
        result = parts[1] .. "|" .. parts[2] .. "|" .. tostring(#parts)
        "#,
    );
    assert_eq!(result, "a||2");
}

#[test]
fn str_indent_skips_empty_lines() {
    let result = eval_string(
        r#"
        local str = require("rootbeer.str")
        result = str.indent("a\n\nb", "  ")
        "#,
    );
    assert_eq!(result, "  a\n\n  b");
}

#[test]
fn tbl_sorted_keys_returns_ascending_order() {
    let result = eval_string(
        r#"
        local tbl = require("rootbeer.tbl")
        result = table.concat(tbl.sorted_keys({ z = 1, a = 2, m = 3 }), ",")
        "#,
    );
    assert_eq!(result, "a,m,z");
}

#[test]
fn tbl_sorted_pairs_yields_key_value_in_order() {
    let result = eval_string(
        r#"
        local tbl = require("rootbeer.tbl")
        local parts = {}
        for k, v in tbl.sorted_pairs({ b = 2, a = 1, c = 3 }) do
            parts[#parts + 1] = k .. "=" .. v
        end
        result = table.concat(parts, ",")
        "#,
    );
    assert_eq!(result, "a=1,b=2,c=3");
}

#[test]
fn tbl_sorted_pairs_on_empty_table_yields_nothing() {
    let result = eval_string(
        r#"
        local tbl = require("rootbeer.tbl")
        local count = 0
        for _ in tbl.sorted_pairs({}) do
            count = count + 1
        end
        result = tostring(count)
        "#,
    );
    assert_eq!(result, "0");
}
