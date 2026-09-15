use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use rootbeer_core::package::{standalone, PackageRequest, RealizedPackage};

#[derive(clap::Args, Debug)]
pub struct RunArgs {
    /// Package name or name@version (aliases and explicit backends are accepted)
    package: String,

    /// Exported command to execute for packages with multiple commands
    #[arg(long)]
    bin: Option<String>,

    /// Open a macOS .app bundle inside the package instead of a command
    #[arg(long, conflicts_with = "bin")]
    app: Option<PathBuf>,

    /// Add another package's commands to this process's PATH
    #[arg(short = 'p', long = "package")]
    packages: Vec<String>,

    /// Use only a previously resolved request and cached package bytes
    #[arg(long, conflicts_with = "update")]
    offline: bool,

    /// Refresh the selected packages instead of reusing cached versions
    #[arg(long)]
    update: bool,

    /// Arguments passed unchanged to the package command, after --
    #[arg(last = true)]
    args: Vec<OsString>,
}

#[derive(clap::Args, Debug)]
pub struct UseArgs {
    /// Packages to install or replace, optionally pinned with @version
    #[arg(required = true)]
    packages: Vec<String>,

    /// Use only previously resolved requests and cached package bytes
    #[arg(long, conflicts_with = "update")]
    offline: bool,

    /// Refresh the selected packages instead of reusing cached versions
    #[arg(long)]
    update: bool,
}

#[derive(clap::Args, Debug)]
pub struct UnuseArgs {
    /// Installed package names to remove from your user profile
    #[arg(required = true)]
    packages: Vec<String>,
}

pub fn uninstall(args: UnuseArgs) {
    if let Err(error) = standalone::remove(&args.packages) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
    for name in args.packages {
        eprintln!("removed {name}");
    }
}

pub fn run(args: RunArgs) {
    if let Err(error) = execute(args) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn execute(args: RunArgs) -> Result<(), String> {
    if args.app.is_some() && !cfg!(target_os = "macos") {
        return Err("--app is supported only on macOS".into());
    }
    let request = PackageRequest::parse(&args.package);
    let mut requests = vec![request.clone()];
    requests.extend(
        args.packages
            .iter()
            .map(|package| PackageRequest::parse(package)),
    );
    let environment = standalone::prepare(&requests, false, args.offline, args.update)?;
    let package = &environment.packages[0];
    let path = command_path(&environment.bin_dir)?;
    if let Some(app) = args.app {
        let app = select_app(&package.store_entry.path, &app)?;
        let error = app_command(&app, &args.args).env("PATH", path).exec();
        return Err(format!("cannot open {}: {error}", app.display()));
    }
    let bin = environment
        .bin_dir
        .join(select_bin(package, &request, args.bin.as_deref())?);
    let error = Command::new(&bin).args(args.args).env("PATH", path).exec();
    Err(format!("cannot run {}: {error}", bin.display()))
}

pub fn install(args: UseArgs) {
    let requests = args
        .packages
        .iter()
        .map(|package| PackageRequest::parse(package))
        .collect::<Vec<_>>();
    match standalone::prepare(&requests, true, args.offline, args.update) {
        Ok(environment) => {
            for package in environment.packages {
                eprintln!(
                    "installed {}@{}",
                    package.package.name, package.package.version
                );
            }
            eprintln!("Run eval \"$(rb env)\" to add installed commands to this shell.");
        }
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}

fn select_app(store_entry: &Path, app: &Path) -> Result<PathBuf, String> {
    if app.as_os_str().is_empty()
        || app
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || app.extension().is_none_or(|extension| extension != "app")
    {
        return Err("--app must name a relative .app directory without '.' or '..'".into());
    }
    let root = store_entry
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let path = root
        .join(app)
        .canonicalize()
        .map_err(|error| format!("cannot find app {}: {error}", app.display()))?;
    if !path.starts_with(&root) {
        return Err("--app must remain inside the package store entry".into());
    }
    if !path.is_dir() || path.extension().is_none_or(|extension| extension != "app") {
        return Err("--app must name an existing .app directory".into());
    }
    Ok(path)
}

fn app_command(app: &Path, args: &[OsString]) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg(app);
    if !args.is_empty() {
        command.arg("--args").args(args);
    }
    command
}

fn command_path(bin_dir: &Path) -> Result<OsString, String> {
    let existing = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(
        std::iter::once(bin_dir.to_path_buf()).chain(std::env::split_paths(&existing)),
    )
    .map_err(|e| e.to_string())
}

fn select_bin<'a>(
    package: &'a RealizedPackage,
    request: &PackageRequest,
    bin: Option<&str>,
) -> Result<&'a Path, String> {
    if let Some(bin) = bin {
        return package
            .bins
            .get_key_value(bin)
            .map(|(name, _)| Path::new(name))
            .ok_or_else(|| {
                format!(
                    "{} does not export '{bin}'; commands: {}",
                    package.package.name,
                    package.bins.keys().cloned().collect::<Vec<_>>().join(", ")
                )
            });
    }
    if let Some((name, _)) = package
        .bins
        .get_key_value(&request.name)
        .or_else(|| package.bins.get_key_value(&package.package.name))
    {
        return Ok(Path::new(name));
    }
    if package.bins.len() == 1 {
        return Ok(Path::new(package.bins.keys().next().unwrap()));
    }
    Err(format!(
        "choose a command with --bin; {} exports: {}",
        package.package.name,
        package.bins.keys().cloned().collect::<Vec<_>>().join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use clap::{error::ErrorKind, Parser};
    use rootbeer_core::package::{LockedInstall, LockedPackage, LockedSource, Provides};
    use rootbeer_core::store::StoreEntry;

    use crate::{Cli, Commands};

    fn package(name: &str, commands: &[&str]) -> RealizedPackage {
        let bins = commands
            .iter()
            .map(|command| (command.to_string(), PathBuf::from(command)))
            .collect::<BTreeMap<_, _>>();
        RealizedPackage {
            apps: Default::default(),
            package: LockedPackage {
                name: name.into(),
                version: "1.0.0".into(),
                source: LockedSource::File {
                    path: "archive".into(),
                    sha256: "0".repeat(64),
                },
                install: LockedInstall::Binary {
                    path: "tool".into(),
                },
                provides: Provides {
                    apps: Default::default(),
                    bins: bins.clone(),
                },
                runtime_dependencies: Default::default(),
                output_sha256: None,
            },
            store_entry: StoreEntry {
                path: "/store/tool".into(),
                name: name.into(),
                version: "1.0.0".into(),
                output_sha256: "0".repeat(64),
            },
            bins,
        }
    }

    #[test]
    fn run_parses_binary_extra_packages_and_preserves_command_flags() {
        let cli = Cli::try_parse_from([
            "rb",
            "run",
            "--bin",
            "python3",
            "python@3.13",
            "-p",
            "jq",
            "--package",
            "curl",
            "--offline",
            "--",
            "-c",
            "print('hello world')",
            "--update",
            "",
        ])
        .unwrap();
        let Commands::Run(args) = cli.command else {
            panic!("expected run command");
        };

        assert_eq!(args.package, "python@3.13");
        assert_eq!(args.bin.as_deref(), Some("python3"));
        assert_eq!(args.packages, ["jq", "curl"]);
        assert!(args.offline);
        assert!(!args.update);
        assert_eq!(
            args.args,
            ["-c", "print('hello world')", "--update", ""].map(OsString::from)
        );
    }

    #[test]
    fn run_preserves_non_unicode_command_arguments() {
        use std::os::unix::ffi::OsStringExt;

        let argument = OsString::from_vec(vec![b'f', 0xff]);
        let cli = Cli::try_parse_from([
            OsString::from("rb"),
            "run".into(),
            "jq".into(),
            "--".into(),
            argument.clone(),
        ])
        .unwrap();
        let Commands::Run(args) = cli.command else {
            panic!("expected run command");
        };

        assert_eq!(args.args, [argument]);
    }

    #[test]
    fn use_accepts_multiple_packages_and_update() {
        let cli = Cli::try_parse_from(["rb", "use", "jq@1.7", "ripgrep", "--update"]).unwrap();
        let Commands::Use(args) = cli.command else {
            panic!("expected use command");
        };

        assert_eq!(args.packages, ["jq@1.7", "ripgrep"]);
        assert!(args.update);
        assert!(!args.offline);
    }

    #[test]
    fn standalone_commands_require_packages_and_reject_conflicting_flags() {
        for command in ["run", "use"] {
            let missing = Cli::try_parse_from(["rb", command]).unwrap_err();
            assert_eq!(missing.kind(), ErrorKind::MissingRequiredArgument);

            let conflict =
                Cli::try_parse_from(["rb", command, "jq", "--offline", "--update"]).unwrap_err();
            assert_eq!(conflict.kind(), ErrorKind::ArgumentConflict);
        }
        let unseparated = Cli::try_parse_from(["rb", "run", "jq", "--version"]).unwrap_err();
        assert_eq!(unseparated.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn binary_selection_prefers_explicit_then_alias_then_package_name() {
        let package = package("python", &["python", "python3", "pip"]);
        let alias = PackageRequest::parse("python3@3.13");

        assert_eq!(
            select_bin(&package, &alias, Some("pip")).unwrap(),
            Path::new("pip")
        );
        assert_eq!(
            select_bin(&package, &alias, None).unwrap(),
            Path::new("python3")
        );
        assert_eq!(
            select_bin(&package, &PackageRequest::parse("py@3.13"), None).unwrap(),
            Path::new("python")
        );
    }

    #[test]
    fn binary_selection_uses_a_single_export_when_names_differ() {
        let package = package("ripgrep", &["rg"]);
        let request = PackageRequest::parse("ripgrep");

        assert_eq!(
            select_bin(&package, &request, None).unwrap(),
            Path::new("rg")
        );
        let error = select_bin(&package, &request, Some("missing")).unwrap_err();
        assert!(error.contains("does not export 'missing'"));
        assert!(error.contains("commands: rg"));
    }

    #[test]
    fn ambiguous_binary_selection_lists_commands_and_requires_override() {
        let package = package("tools", &["second", "first"]);
        let request = PackageRequest::parse("tools");

        let error = select_bin(&package, &request, None).unwrap_err();
        assert!(error.contains("choose a command with --bin"));
        assert!(error.contains("exports: first, second"));
        assert_eq!(
            select_bin(&package, &request, Some("second")).unwrap(),
            Path::new("second")
        );
    }

    #[test]
    fn app_arguments_parse_and_conflict_with_binary_selection() {
        let cli = Cli::try_parse_from([
            "rb",
            "run",
            "bobrwm",
            "--app",
            "Bobrwm.app",
            "--",
            "--config",
            "config with spaces.zon",
        ])
        .unwrap();
        let Commands::Run(args) = cli.command else {
            panic!("expected run command")
        };
        assert_eq!(args.app, Some(PathBuf::from("Bobrwm.app")));
        assert_eq!(
            args.args,
            [
                OsString::from("--config"),
                OsString::from("config with spaces.zon")
            ]
        );
        let error = Cli::try_parse_from([
            "rb",
            "run",
            "bobrwm",
            "--app",
            "Bobrwm.app",
            "--bin",
            "bobrwm",
        ])
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn app_selection_requires_a_contained_relative_bundle_directory() {
        use std::os::unix::fs::symlink;

        let temporary = tempfile::tempdir().unwrap();
        let store = temporary.path().join("store");
        let bundle = store.join("nested/Bobrwm.app");
        std::fs::create_dir_all(&bundle).unwrap();
        assert_eq!(
            select_app(&store, Path::new("nested/Bobrwm.app")).unwrap(),
            bundle.canonicalize().unwrap()
        );
        for path in [
            "",
            ".",
            "../Bobrwm.app",
            "nested/../Bobrwm.app",
            "/Applications/Bobrwm.app",
            "nested",
            "Missing.app",
        ] {
            assert!(select_app(&store, Path::new(path)).is_err(), "{path}");
        }
        std::fs::write(store.join("File.app"), "not a directory").unwrap();
        assert!(select_app(&store, Path::new("File.app")).is_err());
        symlink(&bundle, store.join("Alias.app")).unwrap();
        assert_eq!(
            select_app(&store, Path::new("Alias.app")).unwrap(),
            bundle.canonicalize().unwrap()
        );
        let outside = temporary.path().join("Outside.app");
        std::fs::create_dir(&outside).unwrap();
        symlink(outside, store.join("Escape.app")).unwrap();
        assert!(select_app(&store, Path::new("Escape.app"))
            .unwrap_err()
            .contains("inside the package"));
    }

    #[test]
    fn app_launch_uses_the_exact_bundle_and_forwards_arguments_only_when_present() {
        let bundle = Path::new("/store/package/My App.app");
        let no_args = app_command(bundle, &[]);
        assert_eq!(no_args.get_program(), "/usr/bin/open");
        assert_eq!(no_args.get_args().collect::<Vec<_>>(), [bundle.as_os_str()]);
        let args = [
            OsString::from("--config"),
            OsString::from("a path.zon"),
            OsString::new(),
        ];
        let command = app_command(bundle, &args);
        assert_eq!(
            command.get_args().map(OsString::from).collect::<Vec<_>>(),
            [
                bundle.as_os_str().to_owned(),
                "--args".into(),
                "--config".into(),
                "a path.zon".into(),
                "".into(),
            ]
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn apps_are_rejected_before_package_resolution_on_other_platforms() {
        let cli =
            Cli::try_parse_from(["rb", "run", "missing-package", "--app", "Missing.app"]).unwrap();
        let Commands::Run(args) = cli.command else {
            panic!("expected run command")
        };
        assert_eq!(
            execute(args).unwrap_err(),
            "--app is supported only on macOS"
        );
    }
}
