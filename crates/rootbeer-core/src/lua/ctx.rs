use std::path::{Path, PathBuf};

use mlua::{AppDataRef, Error as LuaError, Lua, Result as LuaResult};

use super::vm::Run;
use crate::plan::Op;
use crate::Runtime;

pub(crate) struct Ctx<'a> {
    pub runtime: AppDataRef<'a, Runtime>,
    pub run: AppDataRef<'a, Run>,
}

impl<'a> Ctx<'a> {
    pub fn from(lua: &'a Lua) -> Self {
        Self {
            runtime: lua.app_data_ref().expect("Runtime not set"),
            run: lua.app_data_ref().expect("Run not set"),
        }
    }

    /// Resolve a user-supplied path: expand `~`, then anchor relatives to
    /// the script dir.
    pub fn resolve(&self, path: &str) -> PathBuf {
        let expanded = if let Some(rest) = path.strip_prefix('~') {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
            if rest.is_empty() {
                PathBuf::from(home)
            } else {
                PathBuf::from(home).join(rest.strip_prefix('/').unwrap_or(rest))
            }
        } else {
            PathBuf::from(path)
        };

        if expanded.is_relative() {
            self.runtime.script_dir.join(expanded)
        } else {
            expanded
        }
    }

    /// Anchor a path relative to the script dir without `~` expansion.
    /// Used for source files that must live within the user's config tree.
    pub fn source(&self, path: &str) -> PathBuf {
        self.runtime.script_dir.join(path)
    }

    pub fn canonicalize(&self, label: &str, original: &str, p: &Path) -> LuaResult<PathBuf> {
        p.canonicalize().map_err(|e| {
            LuaError::RuntimeError(format!(
                "{label} '{original}' (resolved to '{}'): {e}",
                p.display()
            ))
        })
    }

    pub fn push(&self, op: Op) {
        self.run.lock().push(op);
    }

    pub fn slurp(&self, path: &str) -> LuaResult<String> {
        let resolved = self.resolve(path);
        std::fs::read_to_string(&resolved).map_err(|e| {
            LuaError::RuntimeError(format!(
                "failed to read '{}' (resolved to '{}'): {e}",
                path,
                resolved.display()
            ))
        })
    }

    pub fn write(&self, path: &str, content: String) {
        self.push(Op::WriteFile {
            path: self.resolve(path),
            content,
        });
    }

    pub fn chmod(&self, path: &Path, mode: u32) {
        self.push(Op::Chmod {
            path: path.to_path_buf(),
            mode,
        });
    }
}
