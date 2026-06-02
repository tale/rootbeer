use std::path::PathBuf;

/// Where the bytes for a [`Op::WriteFile`] come from.
///
/// Most writes are `Bytes` — content produced during planning (Lua strings,
/// codec output, etc.). Secret-backed variants defer the fetch to apply time
/// so the value never lands in Lua memory or the plan log.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteSource {
    /// Bytes already produced during planning.
    Bytes(Vec<u8>),
    /// Fetched from 1Password via `op document get <reference>` at apply time.
    OpDocument { reference: String },
    // Future providers (e.g. `Rage { ciphertext, identity }`) add a variant
    // here and a matching arm in `executor::apply` + a Lua binding.
}

impl WriteSource {
    /// Convenience for callers producing text content (most codecs, scripts,
    /// `rb.file`). Equivalent to `Bytes(s.into().into_bytes())`.
    pub fn text(s: impl Into<String>) -> Self {
        Self::Bytes(s.into().into_bytes())
    }

    /// Returns the inline content as `&str` when the source is `Bytes` and
    /// the bytes are valid UTF-8. Returns `None` for deferred sources.
    /// Primarily used by tests and pretty-printing.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Bytes(b) => std::str::from_utf8(b).ok(),
            _ => None,
        }
    }

    /// Byte length if known at planning time (i.e. `Bytes` sources). Deferred
    /// sources return `None` because the size isn't known until apply.
    pub fn known_size(&self) -> Option<usize> {
        match self {
            Self::Bytes(b) => Some(b.len()),
            _ => None,
        }
    }

    /// Human-readable label for the upcoming fetch when the source is
    /// secret-backed. Returns `None` for inline `Bytes` (no fetch happens).
    /// Used by the CLI to print a `fetch …` preamble so users see why an
    /// apply is pausing on Touch ID / network. New providers add their
    /// branch here; the CLI never needs to know about them directly.
    pub fn fetch_label(&self) -> Option<String> {
        match self {
            Self::Bytes(_) => None,
            Self::OpDocument { reference } => Some(format!("op-document {reference}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    WriteFile {
        path: PathBuf,
        source: WriteSource,
    },
    Symlink {
        src: PathBuf,
        dst: PathBuf,
    },
    CopyFileIfMissing {
        src: PathBuf,
        dst: PathBuf,
    },
    Exec {
        cmd: String,
        args: Vec<String>,
        cwd: PathBuf,
    },
    Chmod {
        path: PathBuf,
        mode: u32,
    },
    SetRemoteUrl {
        dir: PathBuf,
        url: String,
    },
}
