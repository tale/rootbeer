use std::path::PathBuf;

use crate::package::{LockedPackage, PackageIntent};

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    WriteFile {
        path: PathBuf,
        content: String,
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
    Package {
        intent: PackageIntent,
    },
    RealizePackage {
        package: LockedPackage,
    },
}
