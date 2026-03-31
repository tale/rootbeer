use std::io::{self, Read};

const BASE_URL: &str = "https://rootbeer.tale.me/nightly";

pub fn run() {
    let platform = detect_platform();
    let url = format!("{BASE_URL}/rb-{platform}.zip");
    let version_url = format!("{BASE_URL}/version.txt");

    let current_exe = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("error: could not determine current executable path: {e}");
        std::process::exit(1);
    });

    let install_dir = current_exe.parent().unwrap_or_else(|| {
        eprintln!("error: could not determine install directory");
        std::process::exit(1);
    });

    let old_version = env!("RB_BUILD_TIMESTAMP");
    let new_version = fetch_text(&version_url);

    if let Some(ref new) = new_version {
        if new.trim() == old_version {
            println!("rootbeer is already up to date ({old_version})");
            return;
        }
    }

    println!("downloading rootbeer nightly for {platform}...");

    let zip_bytes = fetch_bytes(&url).unwrap_or_else(|e| {
        eprintln!("error: download failed: {e}");
        eprintln!("hint: check your internet connection or try again later");
        std::process::exit(1);
    });

    let binary = extract_rb_from_zip(&zip_bytes).unwrap_or_else(|e| {
        eprintln!("error: failed to extract archive: {e}");
        std::process::exit(1);
    });

    let dest = install_dir.join("rb");

    // Write to a temp file in the same directory, then rename atomically
    let tmp_dest = install_dir.join(".rb.update.tmp");
    std::fs::write(&tmp_dest, &binary).unwrap_or_else(|e| {
        let _ = std::fs::remove_file(&tmp_dest);
        eprintln!("error: failed to write binary: {e}");
        std::process::exit(1);
    });

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&tmp_dest, perms).unwrap_or_else(|e| {
            let _ = std::fs::remove_file(&tmp_dest);
            eprintln!("error: failed to set permissions: {e}");
            std::process::exit(1);
        });
    }

    std::fs::rename(&tmp_dest, &dest).unwrap_or_else(|e| {
        let _ = std::fs::remove_file(&tmp_dest);
        eprintln!("error: failed to replace binary at {}: {e}", dest.display());
        std::process::exit(1);
    });

    match new_version {
        Some(new) => println!("rootbeer updated: {old_version} -> {}", new.trim()),
        None => println!("rootbeer updated from {old_version}"),
    }
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let response = ureq::get(url).call()?;
    let buf = response.into_body().read_to_vec()?;
    Ok(buf)
}

fn fetch_text(url: &str) -> Option<String> {
    let response = ureq::get(url).call().ok()?;
    response.into_body().read_to_string().ok()
}

fn extract_rb_from_zip(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let cursor = io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;

    let mut file = archive
        .by_name("rb")
        .map_err(|e| format!("archive does not contain 'rb': {e}"))?;

    let mut buf = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

fn detect_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        (os, arch) => {
            eprintln!("error: unsupported platform: {os}-{arch}");
            std::process::exit(1);
        }
    }
}
