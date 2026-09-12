use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::executor::{self, ApplyOptions, ExecutionHandler, ExecutionReport};
use crate::package::lockfile::{LockError, RootbeerLock};
use crate::package::{
    PackageIndexPin, PackageIntent, PackageLockBuilder, PackageResolverInputs, ResolverInput,
};
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

        let package_index = vm
            .lua
            .app_data_ref::<PackageIndexPin>()
            .map(|pin| pin.clone());
        let ops = vm.drain_ops();

        Ok(PlannedPipeline {
            opts: self.opts,
            package_index,
            ops,
        })
    }
}

/// A pipeline that has been planned — ops are collected, ready to execute.
pub struct PlannedPipeline {
    package_index: Option<PackageIndexPin>,
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
                let ops = self.locked_ops_for_apply(&mut |notice| handler.on_output(notice))?;
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

    fn locked_ops_for_apply(&self, notice: &mut dyn FnMut(&str)) -> Result<Vec<Op>, Error> {
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

        let should_refresh = self.opts.package_lock == PackageLockMode::Update;
        let mut inputs = if should_refresh {
            PackageResolverInputs::default()
        } else {
            existing
                .as_ref()
                .map(|lock| lock.inputs.clone())
                .unwrap_or_default()
        };
        if let Some(pin) = &self.package_index {
            inputs.resolvers.insert(
                "rootbeer".into(),
                ResolverInput::PublishedIndex(pin.clone()),
            );
        } else if self.has_canonical_requests() {
            let selection = crate::package::official::select_default(should_refresh)
                .map_err(io::Error::other)?;
            if let Some(message) = &selection.notice {
                notice(message);
            }
            inputs.resolvers.insert("rootbeer".into(), selection.input);
        } else {
            inputs.resolvers.remove("rootbeer");
        }
        let needs_aqua = self.ops.iter().any(|op| {
            matches!(op,
                Op::Package { intent: PackageIntent::Request(request) }
                    if request.resolver.as_deref() == Some("aqua")
            )
        });
        if needs_aqua && (should_refresh || inputs.aqua_registry().is_none()) {
            let current = PackageResolverInputs::resolve_current()?;
            inputs
                .resolvers
                .insert("aqua".into(), current.resolvers["aqua"].clone());
        }
        let builder = PackageLockBuilder::current_system_with_inputs(inputs);
        let input = builder.lock_input_from_ops(&self.ops);
        let lock = builder.build(&input)?;
        lock.write(&path)?;
        Ok(lock.apply_to_ops(&self.ops)?)
    }

    fn lock_matches_plan(&self, lock: &RootbeerLock) -> Result<bool, Error> {
        if lock.inputs.explicit_package_index() != self.package_index.as_ref() {
            return Ok(false);
        }
        let Some(expected) = lock.input_fingerprint.as_deref() else {
            return Ok(true);
        };

        let builder = PackageLockBuilder::current_system_with_inputs(lock.inputs.clone());
        let input = builder.lock_input_from_ops(&self.ops);
        let actual = builder.fingerprint_input(&input)?;
        Ok(actual == expected)
    }

    fn has_canonical_requests(&self) -> bool {
        self.ops.iter().any(|op| {
            matches!(op,
                Op::Package { intent: PackageIntent::Request(request) }
                    if request.resolver.as_deref().is_none_or(|name| name == "rootbeer")
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
            package_index: None,
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
            package_index: None,
            opts: opts(tmp.path().to_path_buf(), Mode::Apply),
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(package()),
            }],
        };

        let ops = planned.locked_ops_for_apply(&mut |_| {}).unwrap();

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
            package_index: None,
            opts: opts(tmp.path().to_path_buf(), Mode::Apply),
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(planned_package),
            }],
        };

        let err = planned.locked_ops_for_apply(&mut |_| {}).unwrap_err();

        assert!(matches!(err, Error::LockBuild(_)));
    }

    #[test]
    fn locked_mode_requires_existing_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let mut opts = opts(tmp.path().to_path_buf(), Mode::Apply);
        opts.package_lock = PackageLockMode::Locked;
        let planned = PlannedPipeline {
            package_index: None,
            opts,
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(package()),
            }],
        };

        let err = planned.locked_ops_for_apply(&mut |_| {}).unwrap_err();

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
            package_index: None,
            opts,
            ops: vec![Op::Package {
                intent: PackageIntent::Locked(package()),
            }],
        };

        let err = planned.locked_ops_for_apply(&mut |_| {}).unwrap_err();

        assert!(matches!(err, Error::Lock(LockError::StaleLockfile { .. })));
    }
    fn plan_index_script(directory: &Path, source: &str) -> PlannedPipeline {
        let script = directory.join("init.lua");
        fs::write(&script, source).unwrap();
        let mut options = Options::from_script(&script).unwrap();
        options.package_lock = PackageLockMode::Offline;
        Pipeline::new(options).plan().unwrap()
    }

    #[test]
    fn index_planning_is_offline_and_pin_changes_invalidate_locks() {
        let root = tempfile::tempdir().unwrap();
        let source = format!("local rb = require('rootbeer')\nrb.package_index({{url='https://unavailable.invalid/index.json', sha256='{}'}})\nrb.package('new-tool')", "a".repeat(64));
        let planned = plan_index_script(root.path(), &source);
        assert!(planned.has_canonical_requests());
        let mut inputs = PackageResolverInputs::default();
        inputs.resolvers.insert(
            "rootbeer".into(),
            ResolverInput::PublishedIndex(planned.package_index.clone().unwrap()),
        );
        let builder = PackageLockBuilder::current_system_with_inputs(inputs.clone());
        let fingerprint = builder
            .fingerprint_input(&builder.lock_input_from_ops(&planned.ops))
            .unwrap();
        let mut lock = RootbeerLock::from_packages(std::iter::empty::<LockedPackage>())
            .unwrap()
            .with_input_fingerprint(fingerprint);
        lock.inputs = inputs;
        assert!(planned.lock_matches_plan(&lock).unwrap());
        lock.write(root.path().join("rootbeer.lock")).unwrap();
        let changed = plan_index_script(
            root.path(),
            &source.replace(&"a".repeat(64), &"b".repeat(64)),
        );
        assert!(!changed.lock_matches_plan(&lock).unwrap());
        assert!(matches!(
            changed.locked_ops_for_apply(&mut |_| {}).unwrap_err(),
            Error::Lock(LockError::StaleLockfile { .. })
        ));
        let removed = plan_index_script(
            root.path(),
            "local rb = require('rootbeer'); rb.package('new-tool')",
        );
        assert!(!removed.lock_matches_plan(&lock).unwrap());
        assert!(removed.has_canonical_requests());
        let mixed = plan_index_script(root.path(), &(source + "\nrb.package('aqua:owner/repo@1')"));
        assert!(mixed.has_canonical_requests());
    }

    #[test]
    fn index_declarations_require_valid_pins_and_precede_packages() {
        let root = tempfile::tempdir().unwrap();
        let pin = format!(
            "rb.package_index({{url='https://example.org/index.json', sha256='{}'}})",
            "a".repeat(64)
        );
        for source in [
            format!("{pin}; {pin}"),
            format!("rb.package('age'); {pin}"),
            pin.replace(&"a".repeat(64), "invalid"),
            pin.replace("https://", "http://"),
        ] {
            let script = root.path().join("init.lua");
            fs::write(&script, format!("local rb = require('rootbeer'); {source}")).unwrap();
            assert!(Pipeline::new(Options::from_script(&script).unwrap())
                .plan()
                .is_err());
        }
    }
    #[test]
    fn official_locks_replay_without_an_override_or_catalog_fetch() {
        use crate::package::lockfile::PackageLockEntry;
        use crate::package::{
            PackageRequest, PackageResolution, PublishedIndexProof, ResolutionProof, ResolveContext,
        };
        let root = tempfile::tempdir().unwrap();
        let mut planned = plan_index_script(
            root.path(),
            "local rb = require('rootbeer'); rb.package('demo')",
        );
        let pin = PackageIndexPin {
            url: "https://unavailable.invalid/index.json".into(),
            sha256: "a".repeat(64),
        };
        let context = ResolveContext::current();
        let entry = PackageLockEntry::resolved(
            &PackageRequest::parse("demo"),
            &context,
            PackageResolution::new(
                package(),
                ResolutionProof::PublishedIndex(PublishedIndexProof {
                    index: pin.clone(),
                    catalog_sha256: "b".repeat(64),
                    revision: 1,
                    system: context.system.clone(),
                    receipt_sha256: "c".repeat(64),
                }),
            ),
        )
        .unwrap();
        let mut lock = RootbeerLock::from_package_entries([entry]).unwrap();
        lock.inputs
            .resolvers
            .insert("rootbeer".into(), ResolverInput::OfficialIndex(pin));
        let builder = PackageLockBuilder::current_system_with_inputs(lock.inputs.clone());
        lock.input_fingerprint = Some(
            builder
                .fingerprint_input(&builder.lock_input_from_ops(&planned.ops))
                .unwrap(),
        );
        lock.write(root.path().join("rootbeer.lock")).unwrap();
        for mode in [
            PackageLockMode::Auto,
            PackageLockMode::Locked,
            PackageLockMode::Offline,
        ] {
            planned.opts.package_lock = mode;
            let ops = planned
                .locked_ops_for_apply(&mut |_| panic!("matching lock attempted catalog selection"))
                .unwrap();
            assert!(matches!(ops.as_slice(), [Op::RealizePackage { .. }]));
        }
    }
}
