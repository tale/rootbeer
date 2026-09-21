use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::time::Duration;

use crate::{BuildOptions, LockedPackage, PackageRealizer, RealizedPackage};

pub(crate) fn check_package(
    package: &LockedPackage,
    realized: &RealizedPackage,
    realizer: &PackageRealizer,
    checks: &[Vec<String>],
    root: &Path,
    build_options: &BuildOptions,
) -> Result<(), String> {
    let profile = root.join("profile");
    fs::create_dir(&profile).map_err(|e| e.to_string())?;
    for (name, path) in &realized.bins {
        symlink(path, profile.join(name)).map_err(|e| e.to_string())?;
    }
    let check_workspace = tempfile::tempdir_in(root).map_err(|error| error.to_string())?;
    let environment = BTreeMap::from([
        (
            "HOME",
            check_workspace.path().to_string_lossy().into_owned(),
        ),
        (
            "PATH",
            if build_options.is_isolated {
                profile.display().to_string()
            } else {
                format!("{}:/usr/bin:/bin", profile.display())
            },
        ),
        ("LC_ALL", "C".into()),
        (
            "TMPDIR",
            check_workspace.path().to_string_lossy().into_owned(),
        ),
    ]);
    let mut runtime_roots = Vec::new();
    for dependency in rootbeer_package::runtime::closure(package)? {
        runtime_roots.push(
            realizer
                .realize(dependency)
                .map_err(|e| e.to_string())?
                .store_entry
                .path,
        );
    }
    let sandbox = if build_options.is_isolated {
        Some(rootbeer_build::Sandbox::new(
            build_options
                .environment
                .as_ref()
                .ok_or("missing build environment")?,
            [profile.clone(), realized.store_entry.path.clone()]
                .into_iter()
                .chain(runtime_roots),
            check_workspace.path(),
        )?)
    } else {
        None
    };
    for check in checks {
        let mut command = check.clone();
        command[0] = profile.join(&command[0]).to_string_lossy().into_owned();
        rootbeer_build::run_with_execution(
            &command,
            check_workspace.path(),
            &environment,
            &root.join("checks.log"),
            Duration::from_secs(30),
            sandbox.as_ref(),
            &build_options.execution,
        )?;
    }
    for (name, app) in &realized.apps {
        let plist = app.join("Contents/Info.plist");
        let output = std::process::Command::new("/usr/bin/plutil")
            .args(["-extract", "CFBundleExecutable", "raw", "-o", "-"])
            .arg(&plist)
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "{name}: missing application executable in Info.plist"
            ));
        }
        let executable = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
        let executable = executable.trim();
        if executable.is_empty() || executable.contains(['/', '\\']) {
            return Err(format!("{name}: invalid application executable"));
        }
        let executable = app
            .join("Contents/MacOS")
            .join(executable)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        if !executable.starts_with(app)
            || !executable.is_file()
            || executable
                .metadata()
                .map_err(|error| error.to_string())?
                .permissions()
                .mode()
                & 0o111
                == 0
        {
            return Err(format!(
                "{name}: application executable is missing, unsafe, or not executable"
            ));
        }
        rootbeer_build::run_with_execution(
            &[
                "/usr/bin/codesign".into(),
                "--verify".into(),
                "--deep".into(),
                "--strict".into(),
                app.to_string_lossy().into_owned(),
            ],
            check_workspace.path(),
            &environment,
            &root.join("checks.log"),
            Duration::from_secs(60),
            sandbox.as_ref(),
            &build_options.execution,
        )?;
    }
    if let Some(environment) = &build_options.environment {
        rootbeer_build::verify_environment(environment)?;
    }
    Ok(())
}
