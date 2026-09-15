use super::{Binary, Format, Violation};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

pub(super) fn check(roots: &BTreeMap<PathBuf, PathBuf>, binaries: &[Binary]) -> Vec<Violation> {
    let mut audit = Checker {
        roots,
        binaries,
        violations: Vec::new(),
        reached: BTreeSet::new(),
    };
    for binary in binaries.iter().filter(|binary| binary.is_executable) {
        audit.visit(binary, Some(&binary.path), &[], &mut BTreeSet::new());
    }
    for binary in binaries {
        if !audit.reached.contains(&(&binary.path, binary.architecture)) {
            audit.visit(binary, None, &[], &mut BTreeSet::new());
        }
    }
    audit.violations.sort_by(|a, b| {
        (&a.path, a.architecture, &a.reference, &a.reason).cmp(&(
            &b.path,
            b.architecture,
            &b.reference,
            &b.reason,
        ))
    });
    audit.violations.dedup_by(|a, b| {
        a.path == b.path
            && a.architecture == b.architecture
            && a.reference == b.reference
            && a.reason == b.reason
    });
    audit.violations
}

struct Checker<'a> {
    roots: &'a BTreeMap<PathBuf, PathBuf>,
    binaries: &'a [Binary],
    violations: Vec<Violation>,
    reached: BTreeSet<(&'a PathBuf, u32)>,
}

impl<'a> Checker<'a> {
    fn reject(&mut self, binary: &Binary, reference: &str, reason: &str) {
        self.violations.push(Violation {
            path: binary.path.clone(),
            architecture: binary.architecture,
            reference: reference.into(),
            reason: reason.into(),
        });
    }

    fn resolve(&self, candidate: &Path) -> Option<PathBuf> {
        for (directory, root) in self.roots {
            let Ok(relative) = candidate.strip_prefix(directory) else {
                continue;
            };
            let path = root.join(relative).canonicalize().ok()?;
            let relative = path.strip_prefix(root).ok()?;
            return Some(directory.join(relative));
        }
        None
    }

    fn visit(
        &mut self,
        binary: &'a Binary,
        executable: Option<&Path>,
        inherited: &[PathBuf],
        active: &mut BTreeSet<(&'a PathBuf, u32)>,
    ) {
        let key = (&binary.path, binary.architecture);
        self.reached.insert(key);
        if active.len() > 128 {
            self.reject(
                binary,
                "dependency chain",
                "runtime dependency depth exceeds 128",
            );
            return;
        }
        if !active.insert(key) {
            return;
        }
        if let Some(interpreter) = &binary.interpreter {
            let allowed = match binary.format {
                Format::MachO => interpreter == "/usr/lib/dyld",
                Format::Elf => matches!(
                    interpreter.as_str(),
                    "/lib64/ld-linux-x86-64.so.2"
                        | "/lib/ld-linux-aarch64.so.1"
                        | "/lib/ld-linux.so.2"
                        | "/lib/ld-linux-armhf.so.3"
                        | "/lib/ld-musl-x86_64.so.1"
                        | "/lib/ld-musl-aarch64.so.1"
                ),
            };
            if !allowed {
                self.reject(
                    binary,
                    interpreter,
                    "interpreter is outside the OS-runtime baseline",
                );
            }
        }
        if let Some(identity) = &binary.identity {
            let allowed = match binary.format {
                Format::Elf => !identity.contains('/') && !identity.is_empty(),
                Format::MachO => ["@rpath/", "@loader_path/"].iter().any(|prefix| {
                    identity
                        .strip_prefix(prefix)
                        .is_some_and(|path| normalize(Path::new(path)).is_ok())
                }),
            };
            if !allowed {
                self.reject(binary, identity, "library identity is not relocatable");
            }
        }
        let mut rpaths = Vec::new();
        let mut runpaths = Vec::new();
        for (paths, resolved) in [
            (&binary.rpaths, &mut rpaths),
            (&binary.runpaths, &mut runpaths),
        ] {
            for path in paths {
                if system_path(binary.format, path) {
                    continue;
                }
                match expand(path, binary, executable) {
                    Ok(path) => resolved.push(path),
                    Err(reason) => self.reject(binary, path, reason),
                }
            }
        }
        let mut inherited_next = rpaths.clone();
        inherited_next.extend_from_slice(inherited);
        let search = if binary.format == Format::Elf && !binary.runpaths.is_empty() {
            &runpaths
        } else {
            &inherited_next
        };
        for library in &binary.libraries {
            if system_library(binary.format, library) {
                continue;
            }
            let candidates = match binary.format {
                Format::Elf if !library.contains('/') => Ok(search
                    .iter()
                    .map(|path| path.join(library))
                    .collect::<Vec<_>>()),
                Format::MachO if library.starts_with("@rpath/") => {
                    let suffix = &library[7..];
                    Ok(search.iter().map(|path| path.join(suffix)).collect())
                }
                _ => expand(library, binary, executable).map(|path| vec![path]),
            };
            let candidates = match candidates {
                Ok(candidates) => candidates,
                Err(reason) => {
                    self.reject(binary, library, reason);
                    continue;
                }
            };
            let mut target = None;
            for candidate in candidates {
                let Ok(candidate) = normalize(&candidate) else {
                    continue;
                };
                let Some(relative) = self.resolve(&candidate) else {
                    continue;
                };
                target = self.binaries.iter().find(|other| {
                    other.path == relative
                        && other.format == binary.format
                        && other.architecture == binary.architecture
                        && other.bits == binary.bits
                        && other.little_endian == binary.little_endian
                        && !other.is_executable
                });
                if target.is_some() {
                    break;
                }
            }
            match target {
                Some(target) => self.visit(target, executable, if binary.format == Format::Elf && !binary.runpaths.is_empty() { inherited } else { &inherited_next }, active),
                None => self.reject(binary, library, "no compatible bundled library at the loader's search paths; external runtime dependencies are not supported yet"),
            }
        }
        active.remove(&key);
    }
}

fn expand(
    reference: &str,
    binary: &Binary,
    executable: Option<&Path>,
) -> Result<PathBuf, &'static str> {
    let (base, suffix) = match binary.format {
        Format::Elf => {
            let suffix = reference
                .strip_prefix("$ORIGIN")
                .or_else(|| reference.strip_prefix("${ORIGIN}"))
                .ok_or(
                    "reference must be relative to $ORIGIN, not a host or working-directory path",
                )?;
            (binary.path.parent().unwrap(), suffix)
        }
        Format::MachO => {
            if let Some(suffix) = reference.strip_prefix("@loader_path") {
                (binary.path.parent().unwrap(), suffix)
            } else if let Some(suffix) = reference.strip_prefix("@executable_path") {
                (
                    executable
                        .ok_or("@executable_path requires an executable dependency context")?
                        .parent()
                        .unwrap(),
                    suffix,
                )
            } else {
                return Err("reference must use @loader_path, @executable_path, or @rpath rather than a host path");
            }
        }
    };
    if !suffix.is_empty() && !suffix.starts_with('/') {
        return Err("invalid loader-relative token");
    }
    if suffix.contains('$') || suffix.contains('@') {
        return Err("unsupported loader token");
    }
    normalize(&base.join(suffix.trim_start_matches('/')))
}

fn normalize(path: &Path) -> Result<PathBuf, &'static str> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => result.push(value),
            Component::CurDir => {}
            Component::ParentDir if result.pop() => {}
            _ => return Err("reference escapes the package"),
        }
    }
    Ok(result)
}

fn system_path(format: Format, path: &str) -> bool {
    if Path::new(path)
        .components()
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    match format {
        Format::MachO => {
            path == "/usr/lib" || Path::new(path).starts_with("/System/Library/Frameworks")
        }
        Format::Elf => matches!(
            path,
            "/lib"
                | "/lib64"
                | "/usr/lib"
                | "/usr/lib64"
                | "/lib/x86_64-linux-gnu"
                | "/usr/lib/x86_64-linux-gnu"
                | "/lib/aarch64-linux-gnu"
                | "/usr/lib/aarch64-linux-gnu"
        ),
    }
}

fn system_library(format: Format, library: &str) -> bool {
    match format {
        Format::MachO => {
            !Path::new(library)
                .components()
                .any(|part| matches!(part, Component::ParentDir))
                && (Path::new(library).starts_with("/usr/lib")
                    || Path::new(library).starts_with("/System/Library/Frameworks"))
        }
        Format::Elf => matches!(
            library,
            "libc.so.6"
                | "libm.so.6"
                | "libdl.so.2"
                | "libpthread.so.0"
                | "librt.so.1"
                | "libgcc_s.so.1"
                | "libstdc++.so.6"
                | "libc.musl-x86_64.so.1"
                | "libc.musl-aarch64.so.1"
                | "ld-linux-x86-64.so.2"
                | "ld-linux-aarch64.so.1"
        ),
    }
}
