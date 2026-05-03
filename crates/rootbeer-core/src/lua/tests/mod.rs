//! Unit tests for the Lua bindings and the high-level Lua modules
//! shipped in `lua/rootbeer/`. Each submodule covers one concern.
//!
//! The pattern is always the same: run a snippet via
//! [`super::test_support::run`] and assert on the resulting `Vec<Op>`.

mod brew;
mod fs;
mod git;
mod profile;
mod vm;
mod writer;
mod zsh;
