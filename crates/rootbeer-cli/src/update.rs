use std::process::Command;

const BASE_URL: &str = "https://rootbeer.tale.me/nightly";

pub fn run() {
    let platform = detect_platform();
    let artifact = format!("rb-{platform}.zip");
    let url = format!("{BASE_URL}/{artifact}");

    let current_exe = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("error: could not determine current executable path: {e}");
        std::process::exit(1);
    });

    let install_dir = current_exe.parent().unwrap_or_else(|| {
        eprintln!("error: could not determine install directory");
        std::process::exit(1);
    });

    let tmpdir = std::env::temp_dir().join(format!("rb-update-{}", std::process::id()));
    std::fs::create_dir_all(&tmpdir).unwrap_or_else(|e| {
        eprintln!("error: failed to create temp directory: {e}");
        std::process::exit(1);
    });

    let zip_path = tmpdir.join("rb.zip");

    println!("downloading rootbeer nightly for {platform}...");

    let status = Command::new("curl")
        .args(["-fsSL", &url, "-o", &zip_path.to_string_lossy()])
        .status()
        .unwrap_or_else(|e| {
            cleanup(&tmpdir);
            eprintln!("error: failed to run curl: {e}");
            std::process::exit(1);
        });

    if !status.success() {
        cleanup(&tmpdir);
        eprintln!("error: download failed");
        eprintln!("hint: check your internet connection or try again later");
        std::process::exit(1);
    }

    let status = Command::new("unzip")
        .args([
            "-q",
            "-o",
            &zip_path.to_string_lossy(),
            "-d",
            &tmpdir.to_string_lossy(),
        ])
        .status()
        .unwrap_or_else(|e| {
            cleanup(&tmpdir);
            eprintln!("error: failed to run unzip: {e}");
            std::process::exit(1);
        });

    if !status.success() {
        cleanup(&tmpdir);
        eprintln!("error: failed to unzip archive");
        std::process::exit(1);
    }

    let new_binary = tmpdir.join("rb");
    let dest = install_dir.join("rb");

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&new_binary, perms).unwrap_or_else(|e| {
            cleanup(&tmpdir);
            eprintln!("error: failed to set permissions: {e}");
            std::process::exit(1);
        });
    }

    std::fs::rename(&new_binary, &dest)
        .or_else(|_| {
            // rename fails across filesystems, fall back to copy
            std::fs::copy(&new_binary, &dest).map(|_| ())
        })
        .unwrap_or_else(|e| {
            cleanup(&tmpdir);
            eprintln!("error: failed to replace binary at {}: {e}", dest.display());
            std::process::exit(1);
        });

    cleanup(&tmpdir);
    println!("rootbeer updated to latest nightly");
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn detect_platform() -> String {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        eprintln!("error: unsupported OS");
        std::process::exit(1);
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        eprintln!("error: unsupported architecture");
        std::process::exit(1);
    };

    format!("{os}-{arch}")
}
