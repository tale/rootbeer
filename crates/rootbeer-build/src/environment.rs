use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use rootbeer_package::{BuildEnvironmentInput, BuildEnvironmentLock, ResolveContext};
use rootbeer_store::{hash_bytes, hash_file};
use serde::{Deserialize, Serialize};

/// Explicit tools, SDK/toolchain directories, and variables to pin before building.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildEnvironment {
    pub tools: BTreeMap<String, PathBuf>,
    #[serde(default)]
    pub inputs: BTreeMap<String, PathBuf>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

impl BuildEnvironment {
    /// Hashes declared inputs; paths must be absolute and directory links must stay inside their root.
    pub fn pin(&self) -> Result<BuildEnvironmentLock, String> {
        for required in ["sh", "cc", "make", "patch"] {
            if !self.tools.contains_key(required) {
                return Err(format!("build environment requires tool {required}"));
            }
        }
        for name in self.variables.keys() {
            if !valid_name(name) || reserved(name) {
                return Err(format!("build environment cannot set variable {name}"));
            }
            if self.variables[name].contains('\0') {
                return Err(format!("build variable {name} contains a null byte"));
            }
        }
        let collect = |paths: &BTreeMap<String, PathBuf>, is_tool| {
            paths
                .iter()
                .map(|(name, path)| {
                    if !valid_name(name) {
                        return Err(format!("invalid build input name {name}"));
                    }
                    let input = pin_input(path, is_tool)?;
                    Ok((name.clone(), input))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()
        };
        Ok(BuildEnvironmentLock {
            schema: 1,
            system: ResolveContext::current().system,
            tools: collect(&self.tools, true)?,
            inputs: collect(&self.inputs, false)?,
            variables: self.variables.clone(),
        })
    }
}

pub(crate) struct Environment {
    pub lock: BuildEnvironmentLock,
    pub is_host: bool,
}

impl Environment {
    pub fn resolve(lock: Option<&BuildEnvironmentLock>) -> Result<Self, String> {
        if let Some(lock) = lock {
            verify_environment(lock)?;
            return Ok(Self {
                lock: lock.clone(),
                is_host: false,
            });
        }
        let tools = ["sh", "cc", "c++", "make", "patch", "ar", "ranlib", "uname"]
            .into_iter()
            .map(|name| {
                let path = ["/usr/bin", "/bin"]
                    .iter()
                    .map(|root| Path::new(root).join(name))
                    .find(|path| path.is_file())
                    .ok_or_else(|| format!("missing host tool {name}"))?;
                Ok((name.into(), path))
            })
            .collect::<Result<_, String>>()?;
        let lock = BuildEnvironment {
            tools,
            ..Default::default()
        }
        .pin()?;
        Ok(Self {
            lock,
            is_host: true,
        })
    }

    pub fn identity(&self, context: &str) -> Result<String, String> {
        let bytes = serde_json::to_vec(&(context, self.is_host, &self.lock))
            .map_err(|error| error.to_string())?;
        Ok(hash_bytes(&bytes))
    }

    pub fn stage(&self, directory: &Path) -> Result<(), String> {
        fs::create_dir(directory).map_err(|error| error.to_string())?;
        for (name, input) in &self.lock.tools {
            symlink(&input.path, directory.join(name)).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn variables<'a>(
        &'a self,
        tools: &Path,
        host_tools: &Path,
        workspace: &Path,
    ) -> BTreeMap<&'a str, String> {
        let mut variables = self
            .lock
            .variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        let fallback = if self.is_host {
            ":/usr/bin:/bin:/usr/sbin:/sbin"
        } else {
            ""
        };
        variables.extend([
            (
                "PATH",
                format!("{}:{}{fallback}", tools.display(), host_tools.display()),
            ),
            ("HOME", workspace.to_string_lossy().into_owned()),
            ("TMPDIR", workspace.to_string_lossy().into_owned()),
            ("LC_ALL", "C".into()),
            ("TZ", "UTC".into()),
            ("SOURCE_DATE_EPOCH", "1".into()),
            ("ZERO_AR_DATE", "1".into()),
            (
                "CONFIG_SHELL",
                self.lock.tools["sh"].path.to_string_lossy().into_owned(),
            ),
            (
                "CC",
                self.lock.tools["cc"].path.to_string_lossy().into_owned(),
            ),
        ]);
        if let Some(compiler) = self.lock.tools.get("c++") {
            variables.insert("CXX", compiler.path.to_string_lossy().into_owned());
        }
        variables
    }
}

/// Rejects stale pins before a cached result can be used or a new result published.
pub fn verify_environment(lock: &BuildEnvironmentLock) -> Result<(), String> {
    if lock.schema != 1 || lock.system != ResolveContext::current().system {
        return Err("unsupported build environment schema or host system".into());
    }
    let specification = BuildEnvironment {
        tools: lock
            .tools
            .iter()
            .map(|(name, input)| (name.clone(), input.path.clone()))
            .collect(),
        inputs: lock
            .inputs
            .iter()
            .map(|(name, input)| (name.clone(), input.path.clone()))
            .collect(),
        variables: lock.variables.clone(),
    };
    if specification.pin()? != *lock {
        return Err(
            "build environment inputs changed; review and regenerate the environment lock".into(),
        );
    }
    Ok(())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_+.-".contains(&byte))
}

fn reserved(name: &str) -> bool {
    matches!(
        name,
        "PATH"
            | "HOME"
            | "TMPDIR"
            | "LC_ALL"
            | "TZ"
            | "SOURCE_DATE_EPOCH"
            | "ZERO_AR_DATE"
            | "CONFIG_SHELL"
            | "CC"
            | "CXX"
            | "PKG_CONFIG_PATH"
            | "PKG_CONFIG_LIBDIR"
            | "PKG_CONFIG_SYSROOT_DIR"
    )
}

fn pin_input(path: &Path, is_tool: bool) -> Result<BuildEnvironmentInput, String> {
    if !path.is_absolute() {
        return Err(format!("build input must be absolute: {}", path.display()));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = fs::metadata(&canonical).map_err(|error| error.to_string())?;
    let digest = if is_tool {
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!("build tool is not executable: {}", path.display()));
        }
        hash_file(&canonical).map_err(|error| error.to_string())?
    } else {
        if !metadata.is_dir() {
            return Err(format!(
                "build input must be a directory: {}",
                path.display()
            ));
        }
        hash_directory(&canonical, &canonical)?
    };
    Ok(BuildEnvironmentInput {
        path: path.to_path_buf(),
        sha256: digest,
    })
}

fn hash_directory(root: &Path, directory: &Path) -> Result<String, String> {
    let mut entries = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = entry
            .path()
            .symlink_metadata()
            .map_err(|error| error.to_string())?;
        let kind = metadata.file_type();
        let (kind, digest) = if kind.is_symlink() {
            let resolved = entry
                .path()
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !resolved.starts_with(root) {
                return Err(format!(
                    "build input link escapes its directory: {}",
                    entry.path().display()
                ));
            }
            let target = fs::read_link(entry.path()).map_err(|error| error.to_string())?;
            (
                "link",
                hash_bytes(&serde_json::to_vec(&target).map_err(|error| error.to_string())?),
            )
        } else if kind.is_dir() {
            ("directory", hash_directory(root, &entry.path())?)
        } else if kind.is_file() {
            (
                "file",
                hash_file(entry.path()).map_err(|error| error.to_string())?,
            )
        } else {
            return Err(format!(
                "unsupported build input file: {}",
                entry.path().display()
            ));
        };
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "build input names must be UTF-8")?;
        entries.insert(name, (kind, metadata.permissions().mode() & 0o777, digest));
    }
    Ok(hash_bytes(
        &serde_json::to_vec(&entries).map_err(|error| error.to_string())?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specification() -> BuildEnvironment {
        let host = Environment::resolve(None).unwrap();
        BuildEnvironment {
            tools: host
                .lock
                .tools
                .into_iter()
                .map(|(name, input)| (name, input.path))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn tools_sdk_contents_and_variables_participate_in_identity() {
        let directory = tempfile::tempdir().unwrap();
        let tool = directory.path().join("cc");
        fs::write(&tool, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        let sdk = directory.path().join("sdk");
        fs::create_dir(&sdk).unwrap();
        fs::write(sdk.join("header.h"), "first").unwrap();
        let mut spec = specification();
        spec.tools.insert("cc".into(), tool.clone());
        spec.inputs.insert("sdk".into(), sdk.clone());
        let original = spec.pin().unwrap();
        let identity = Environment::resolve(Some(&original))
            .unwrap()
            .identity("image")
            .unwrap();
        fs::write(&tool, "#!/bin/sh\nexit 1\n").unwrap();
        assert!(verify_environment(&original)
            .unwrap_err()
            .contains("changed"));
        let changed_tool = spec.pin().unwrap();
        assert_ne!(
            identity,
            Environment::resolve(Some(&changed_tool))
                .unwrap()
                .identity("image")
                .unwrap()
        );
        fs::write(sdk.join("header.h"), "second").unwrap();
        assert!(verify_environment(&changed_tool)
            .unwrap_err()
            .contains("changed"));
        let changed_sdk = spec.pin().unwrap();
        fs::create_dir(sdk.join(".rootbeer")).unwrap();
        fs::write(sdk.join(".rootbeer/config"), "tracked").unwrap();
        assert!(verify_environment(&changed_sdk)
            .unwrap_err()
            .contains("changed"));
        let changed_sdk = spec.pin().unwrap();
        spec.variables.insert("CFLAGS".into(), "-O2".into());
        let changed_flags = spec.pin().unwrap();
        assert_ne!(
            Environment::resolve(Some(&changed_sdk))
                .unwrap()
                .identity("image")
                .unwrap(),
            Environment::resolve(Some(&changed_flags))
                .unwrap()
                .identity("image")
                .unwrap()
        );
    }

    #[test]
    fn pinned_path_excludes_undeclared_host_tools_and_preserves_flags() {
        let directory = tempfile::tempdir().unwrap();
        let mut spec = specification();
        spec.variables.insert("CFLAGS".into(), "-O2".into());
        let lock = spec.pin().unwrap();
        let environment = Environment::resolve(Some(&lock)).unwrap();
        let tools = directory.path().join("tools");
        environment.stage(&tools).unwrap();
        let variables = environment.variables(&tools, &tools, directory.path());
        crate::run(
            &[
                "sh".into(),
                "-c".into(),
                "test \"$CFLAGS\" = -O2 && ! command -v env && test -z \"${USER+x}\"".into(),
            ],
            directory.path(),
            &variables,
            &directory.path().join("log"),
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(variables["CC"], lock.tools["cc"].path.to_string_lossy());
    }

    #[test]
    fn rejects_environment_overrides_and_untracked_symlink_targets() {
        let directory = tempfile::tempdir().unwrap();
        let sdk = directory.path().join("sdk");
        fs::create_dir(&sdk).unwrap();
        fs::write(directory.path().join("external.h"), "external").unwrap();
        symlink("../external.h", sdk.join("header.h")).unwrap();
        let mut spec = specification();
        spec.inputs.insert("sdk".into(), sdk);
        assert!(spec.pin().unwrap_err().contains("escapes"));
        spec.inputs.clear();
        spec.variables.insert("PATH".into(), "/usr/bin".into());
        assert!(spec.pin().unwrap_err().contains("cannot set"));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod archive_tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn apple_archives_ignore_member_timestamps_in_build_environments() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(root.join("member.c"), "int member(void) { return 42; }\n").unwrap();
        let environment = Environment::resolve(None).unwrap();
        let variables = environment.variables(root, root, root);
        let run = |args: &[&str]| {
            crate::run(
                &args.iter().map(|arg| (*arg).into()).collect::<Vec<_>>(),
                root,
                &variables,
                &root.join("archive.log"),
                Duration::from_secs(30),
            )
            .unwrap();
        };
        run(&["/usr/bin/cc", "-c", "member.c", "-o", "member.o"]);
        let object = fs::File::open(root.join("member.o")).unwrap();
        object
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(3600))
            .unwrap();
        run(&["/usr/bin/ar", "crs", "first.a", "member.o"]);
        object
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(7200))
            .unwrap();
        run(&["/usr/bin/ar", "crs", "second.a", "member.o"]);
        assert_eq!(
            fs::read(root.join("first.a")).unwrap(),
            fs::read(root.join("second.a")).unwrap()
        );
    }
}
