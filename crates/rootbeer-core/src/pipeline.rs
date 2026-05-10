use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::executor::{self, ApplyOptions, ExecutionHandler, ExecutionReport};
use crate::package::lockfile::{LockError, RootbeerLock};
use crate::package::{PackageIntent, PackageLockBuilder, PackageResolverInputs};
use crate::plan::Op;
use crate::{Error, Runtime};

#[derive(Debug, Default, Clone, Copy)]
pub enum Mode {
    #[default]
    Apply,
    DryRun,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PackageLockMode {
    #[default]
    Auto,
    Locked,
    Offline,
    Update,
}

impl PackageLockMode {
    pub fn offline(self) -> bool {
        matches!(self, Self::Offline)
    }
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Apply => write!(f, "apply"),
            Mode::DryRun => write!(f, "dry run"),
        }
    }
}

#[derive(Debug)]
pub struct Options {
    pub script_dir: PathBuf,
    pub script_name: String,
    pub lua_dir: PathBuf,
    pub profile: Option<String>,
    pub mode: Mode,
    pub force: bool,
    pub package_lock: PackageLockMode,
}

impl Options {
    pub fn from_script(script: &Path) -> io::Result<Self> {
        let script = script.canonicalize()?;
        let script_dir = script
            .parent()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "script has no parent directory",
                )
            })?
            .to_path_buf();

        let script_name = script
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "script has no file name"))?
            .to_string_lossy()
            .to_string();

        Ok(Self {
            lua_dir: PathBuf::from(env!("ROOTBEER_LUA_DIR")),
            script_dir,
            script_name,
            profile: None,
            mode: Mode::default(),
            force: false,
            package_lock: PackageLockMode::default(),
        })
    }
}

/// Entry point: configure the pipeline, then call `.plan()` to evaluate Lua.
pub struct Pipeline {
    opts: Options,
}

impl Pipeline {
    pub fn new(opts: Options) -> Self {
        Self { opts }
    }

    pub fn mode(&self) -> Mode {
        self.opts.mode
    }

    pub fn force(&self) -> bool {
        self.opts.force
    }

    /// Evaluate the Lua script and advance to the planned phase.
    pub fn plan(self) -> Result<PlannedPipeline, Error> {
        let runtime = Runtime {
            script_dir: self.opts.script_dir.clone(),
            script_name: self.opts.script_name.clone(),
            lua_dir: self.opts.lua_dir.clone(),
            profile: self.opts.profile.clone(),
        };

        let source = fs::read_to_string(runtime.script_dir.join(&runtime.script_name))?;
        let chunk_name = format!(
            "@{}",
            runtime.script_dir.join(&runtime.script_name).display()
        );
        let vm = crate::lua::Vm::new(runtime)?;
        if let Err(e) = vm.exec(&source, &chunk_name) {
            if let Some(pe) = crate::profile::extract(&e) {
                return Err(pe.into());
            }
            return Err(e.into());
        }

        let ops = vm.drain_ops();

        Ok(PlannedPipeline {
            opts: self.opts,
            ops,
        })
    }
}

/// A pipeline that has been planned — ops are collected, ready to execute.
pub struct PlannedPipeline {
    opts: Options,
    ops: Vec<Op>,
}

impl PlannedPipeline {
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    pub fn mode(&self) -> Mode {
        self.opts.mode
    }

    pub fn force(&self) -> bool {
        self.opts.force
    }

    /// Execute the planned operations, reporting progress to the handler.
    pub fn execute(self, handler: &mut impl ExecutionHandler) -> Result<ExecutionReport, Error> {
        let report = match self.opts.mode {
            Mode::Apply => {
                let ops = self.locked_ops_for_apply()?;
                executor::apply_with_options(
                    &ops,
                    self.opts.force,
                    handler,
                    ApplyOptions {
                        package_offline: self.opts.package_lock.offline(),
                    },
                )?
            }
            Mode::DryRun => executor::dry_run(&self.ops, handler),
        };

        Ok(report)
    }

    fn locked_ops_for_apply(&self) -> Result<Vec<Op>, Error> {
        if !RootbeerLock::has_package_ops(&self.ops) {
            return Ok(self.ops.clone());
        }

        let path = self.opts.script_dir.join("rootbeer.lock");
        let existing = if path.exists() {
            Some(RootbeerLock::read(&path)?)
        } else {
            None
        };

        if matches!(
            self.opts.package_lock,
            PackageLockMode::Locked | PackageLockMode::Offline
        ) {
            let Some(lock) = existing.as_ref() else {
                return Err(LockError::MissingLockfile { path }.into());
            };

            if !self.lock_matches_plan(lock)? {
                return Err(LockError::StaleLockfile { path }.into());
            }

            return lock.apply_to_ops(&self.ops).map_err(Into::into);
        }

        if !matches!(self.opts.package_lock, PackageLockMode::Update) {
            if let Some(lock) = existing.as_ref() {
                if self.lock_matches_plan(lock)? {
                    match lock.apply_to_ops(&self.ops) {
                        Ok(ops) => return Ok(ops),
                        Err(
                            LockError::MissingPackage { .. } | LockError::PackageChanged { .. },
                        ) => {}
                        Err(err) => return Err(err.into()),
                    }
                }
            }
        }

        let inputs = match self.opts.package_lock {
            PackageLockMode::Update if self.has_package_requests() => {
                PackageResolverInputs::resolve_current()?
            }
            PackageLockMode::Update => PackageResolverInputs::default(),
            PackageLockMode::Auto => existing
                .as_ref()
                .and_then(|lock| (!lock.inputs.is_empty()).then(|| lock.inputs.clone()))
                .map(Ok)
                .unwrap_or_else(|| {
                    if self.has_package_requests() {
                        PackageResolverInputs::resolve_current()
                    } else {
                        Ok(PackageResolverInputs::default())
                    }
                })?,
            PackageLockMode::Locked | PackageLockMode::Offline => unreachable!(),
        };

        let builder = PackageLockBuilder::current_system_with_inputs(inputs);
        let input = builder.lock_input_from_ops(&self.ops);
        let lock = builder.build(&input)?;
        lock.write(&path)?;
        Ok(lock.apply_to_ops(&self.ops)?)
    }

    fn lock_matches_plan(&self, lock: &RootbeerLock) -> Result<bool, Error> {
        let Some(expected) = lock.input_fingerprint.as_deref() else {
            return Ok(true);
        };

        let builder = PackageLockBuilder::current_system_with_inputs(lock.inputs.clone());
        let input = builder.lock_input_from_ops(&self.ops);
        let actual = builder.fingerprint_input(&input)?;
        Ok(actual == expected)
    }

    fn has_package_requests(&self) -> bool {
        self.ops.iter().any(|op| {
            matches!(
                op,
                Op::Package {
                    intent: PackageIntent::Request(_)
                }
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::executor::OpResult;
    use crate::package::{LockedInstall, LockedPackage, LockedSource, PackageIntent, Provides};

    use super::*;

    #[derive(Default)]
    struct Recorder;

    impl ExecutionHandler for Recorder {
        fn on_start(&mut self, _op: &Op) {}
        fn on_output(&mut self, _line: &str) {}
        fn on_result(&mut self, _result: &OpResult) {}
    }

    fn opts(script_dir: PathBuf, mode: Mode) -> Options {
        Options {
            script_dir,
            script_name: "init.lua".to_string(),
            lua_dir: PathBuf::from("lua"),
            profile: None,
            mode,
            force: false,
            package_lock: PackageLockMode::Auto,
        }
    }

    fn package() -> LockedPackage {
        LockedPackage {
            name: "demo".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Path {
                path: PathBuf::from("demo"),
                sha256: "source".to_string(),
            },
            install: LockedInstall::Directory { strip_prefix: None },
            provides: Provides {
                bins: BTreeMap::new(),
            },
            output_sha256: None,
        }
    }

    fn package_with_source_path(path: PathBuf) -> LockedPackage {
        let mut package = package();
        package.source = LockedSource::Path {
            path,
            sha256: "source".to_string(),
        };
        package
    }

    #[test]
    fn apply_attempts_to_build_lock_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let planned = PlannedPipeline {
            opts: opts(tmp.path().to_path_buf(), Mode::Apply),
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(package()),
            }],
        };

        let err = planned.execute(&mut Recorder).unwrap_err();

        assert!(matches!(err, Error::LockBuild(_)));
    }

    #[test]
    fn apply_uses_locked_package_facts() {
        let tmp = tempfile::tempdir().unwrap();
        let mut locked = package();
        locked.output_sha256 = Some("output".to_string());
        RootbeerLock::from_packages([locked])
            .unwrap()
            .write(tmp.path().join("rootbeer.lock"))
            .unwrap();
        let planned = PlannedPipeline {
            opts: opts(tmp.path().to_path_buf(), Mode::Apply),
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(package()),
            }],
        };

        let ops = planned.locked_ops_for_apply().unwrap();

        let [Op::RealizePackage { package }] = ops.as_slice() else {
            panic!("expected package op");
        };
        assert_eq!(package.output_sha256.as_deref(), Some("output"));
    }

    #[test]
    fn apply_rebuilds_lock_when_input_fingerprint_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let planned_package = package_with_source_path(tmp.path().join("missing-source"));
        let mut locked = planned_package.clone();
        locked.output_sha256 = Some("output".to_string());
        RootbeerLock::from_packages([locked])
            .unwrap()
            .with_input_fingerprint("stale")
            .write(tmp.path().join("rootbeer.lock"))
            .unwrap();
        let planned = PlannedPipeline {
            opts: opts(tmp.path().to_path_buf(), Mode::Apply),
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(planned_package),
            }],
        };

        let err = planned.locked_ops_for_apply().unwrap_err();

        assert!(matches!(err, Error::LockBuild(_)));
    }

    #[test]
    fn locked_mode_requires_existing_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let mut opts = opts(tmp.path().to_path_buf(), Mode::Apply);
        opts.package_lock = PackageLockMode::Locked;
        let planned = PlannedPipeline {
            opts,
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(package()),
            }],
        };

        let err = planned.locked_ops_for_apply().unwrap_err();

        assert!(matches!(
            err,
            Error::Lock(LockError::MissingLockfile { .. })
        ));
    }

    #[test]
    fn locked_mode_rejects_stale_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let mut locked = package();
        locked.output_sha256 = Some("output".to_string());
        RootbeerLock::from_packages([locked])
            .unwrap()
            .with_input_fingerprint("stale")
            .write(tmp.path().join("rootbeer.lock"))
            .unwrap();
        let mut opts = opts(tmp.path().to_path_buf(), Mode::Apply);
        opts.package_lock = PackageLockMode::Locked;
        let planned = PlannedPipeline {
            opts,
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(package()),
            }],
        };

        let err = planned.locked_ops_for_apply().unwrap_err();

        assert!(matches!(err, Error::Lock(LockError::StaleLockfile { .. })));
    }
}
