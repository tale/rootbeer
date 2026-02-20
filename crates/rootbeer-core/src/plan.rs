use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    WriteFile { path: PathBuf, content: String },
    Symlink { src: PathBuf, dst: PathBuf },
}
