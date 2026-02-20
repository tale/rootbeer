use mlua::{Function, Lua, NavigateError, Require, Result, TextRequirer};
use std::{io, path::PathBuf};

/// A custom Require implementation that wraps TextRequirer and injects
/// a synthetic `@rootbeer` alias pointing to `<lua_dir>/rootbeer`.
/// This avoids needing a `.luaurc` file on disk.
pub(crate) struct RootbeerRequirer {
    inner: TextRequirer,
    config: Vec<u8>,
}

/// Takes the Lua directory and binds the `@rootbeer` alias to the directory.
impl RootbeerRequirer {
    pub fn new(lua_dir: PathBuf) -> Self {
        let root = lua_dir.join("rootbeer");
        let config = format!(r#"{{"aliases":{{"rootbeer":"{}"}}}}"#, root.display()).into_bytes();

        Self {
            inner: TextRequirer::new(),
            config,
        }
    }
}

/// Delegate TextRequirer methods
impl Require for RootbeerRequirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        self.inner.is_require_allowed(chunk_name)
    }

    fn reset(&mut self, chunk_name: &str) -> std::result::Result<(), NavigateError> {
        self.inner.reset(chunk_name)
    }

    fn jump_to_alias(&mut self, path: &str) -> std::result::Result<(), NavigateError> {
        self.inner.jump_to_alias(path)
    }

    fn to_parent(&mut self) -> std::result::Result<(), NavigateError> {
        self.inner.to_parent()
    }

    fn to_child(&mut self, name: &str) -> std::result::Result<(), NavigateError> {
        self.inner.to_child(name)
    }

    fn has_module(&self) -> bool {
        self.inner.has_module()
    }

    fn cache_key(&self) -> String {
        self.inner.cache_key()
    }

    fn has_config(&self) -> bool {
        true
    }

    fn config(&self) -> io::Result<Vec<u8>> {
        Ok(self.config.clone())
    }

    fn loader(&self, lua: &Lua) -> Result<Function> {
        self.inner.loader(lua)
    }
}
