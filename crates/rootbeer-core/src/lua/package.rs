use std::collections::BTreeMap;
use std::path::PathBuf;

use mlua::{Error as LuaError, Lua, Result as LuaResult, Table, Value};

use super::ctx::Ctx;
use super::module::Module;
use super::vm::{profile_bin_path, PackageBins};
use crate::package::{
    lockfile::RootbeerLock, profile as package_profile, ArchiveFormat, LockedInstall,
    LockedPackage, LockedSource, PackageCatalog, PackageIntent, PackageRequest, Provides,
    ResolveContext,
};
use crate::plan::{Op, WriteSource};

pub(crate) struct Package;

impl Module for Package {
    const NAME: &'static str = "";

    fn build(lua: &Lua, t: &Table) -> LuaResult<()> {
        t.set(
            "package",
            lua.create_function(|lua, (spec, opts): (Value, Option<Table>)| {
                let cx = Ctx::from(lua);
                if opts.is_some() && !matches!(&spec, Value::String(_)) {
                    return Err(LuaError::RuntimeError(
                        "package options require a request string".to_string(),
                    ));
                }
                let intent = match spec {
                    Value::Table(spec) => {
                        let package = parse_package(&cx, spec)?;
                        register_package_bins(lua, &package);
                        PackageIntent::locked(package)
                    }
                    Value::String(spec) => {
                        let mut request = PackageRequest::parse(spec.to_str()?.as_ref());
                        if let Some(opts) = opts {
                            request.asset = optional(&opts, "asset")?;
                            if let Some(bins) = optional::<Table>(&opts, "bins")? {
                                request.bins = parse_provides(bins)?.bins;
                            }
                            if request.resolver.as_deref() != Some("github") {
                                return Err(LuaError::RuntimeError(
                                    "package options currently require the github resolver"
                                        .to_string(),
                                ));
                            }
                        }
                        for bin in request.bins.keys() {
                            lua.app_data_ref::<PackageBins>()
                                .expect("PackageBins not set")
                                .insert(bin.clone(), profile_bin_path(bin));
                        }
                        if !register_locked_request_bins(lua, &cx, &request) {
                            register_catalog_request_bins(lua, &request)?;
                        }
                        PackageIntent::request(request)
                    }
                    other => {
                        return Err(LuaError::RuntimeError(format!(
                            "rb.package expected a package table or request string, got {}",
                            other.type_name()
                        )));
                    }
                };

                cx.push(Op::Package { intent });
                Ok(())
            })?,
        )?;

        t.set(
            "which",
            lua.create_function(|lua, bin: String| {
                let bins = lua
                    .app_data_ref::<PackageBins>()
                    .expect("PackageBins not set");

                if let Some(path) = bins.get(&bin) {
                    return Ok(Some(path.to_string_lossy().to_string()));
                }

                let path = profile_bin_path(&bin);
                Ok(path.exists().then(|| path.to_string_lossy().to_string()))
            })?,
        )?;

        t.set(
            "env_export",
            lua.create_function(|lua, shell: Option<String>| {
                let shell = shell.as_deref().unwrap_or("sh");
                if !matches!(shell, "sh" | "bash" | "zsh") {
                    return Err(LuaError::RuntimeError(format!(
                        "unsupported env export shell `{shell}`"
                    )));
                }

                let path = package_profile::env_path();
                Ctx::from(lua).push(Op::WriteFile {
                    path: path.clone(),
                    source: WriteSource::text(package_profile::env_contents()),
                });

                Ok(path.to_string_lossy().to_string())
            })?,
        )?;

        Ok(())
    }
}

fn register_package_bins(lua: &Lua, package: &LockedPackage) {
    let bins = lua
        .app_data_ref::<PackageBins>()
        .expect("PackageBins not set");

    for bin in package.provides.bins.keys() {
        bins.insert(bin.clone(), profile_bin_path(bin));
    }
}

fn register_locked_request_bins(lua: &Lua, cx: &Ctx<'_>, request: &PackageRequest) -> bool {
    let lock_path = cx.runtime.script_dir.join("rootbeer.lock");
    let Ok(lock) = RootbeerLock::read(lock_path) else {
        return false;
    };

    let Ok(package) = lock.package_for_request(request, &ResolveContext::current()) else {
        return false;
    };

    register_package_bins(lua, package);
    true
}

fn register_catalog_request_bins(lua: &Lua, request: &PackageRequest) -> LuaResult<()> {
    if request
        .resolver
        .as_deref()
        .is_some_and(|resolver| resolver != "rootbeer")
    {
        return Ok(());
    }
    let catalog = PackageCatalog::embedded().map_err(LuaError::RuntimeError)?;
    let Some(package) = catalog.find(&request.name) else {
        return Ok(());
    };
    let version = request.version.as_ref().unwrap_or(&package.default_version);
    let Some(recipe) = package.versions.get(version) else {
        return Ok(());
    };
    if !recipe.systems.contains(&ResolveContext::current().system) {
        return Ok(());
    }
    for bin in &recipe.bins {
        lua.app_data_ref::<PackageBins>()
            .expect("PackageBins not set")
            .insert(bin.clone(), profile_bin_path(bin));
    }
    Ok(())
}

fn parse_package(cx: &Ctx<'_>, spec: Table) -> LuaResult<LockedPackage> {
    let name: String = required(&spec, "name")?;
    let version: String = required(&spec, "version")?;
    let source = parse_source(cx, required(&spec, "source")?)?;
    let install = parse_install(required(&spec, "install")?)?;
    let provides = parse_provides(required(&spec, "bins")?)?;

    Ok(LockedPackage {
        name,
        version,
        source,
        install,
        provides,
        output_sha256: None,
    })
}

fn parse_source(cx: &Ctx<'_>, source: Table) -> LuaResult<LockedSource> {
    let sha256: String = required(&source, "sha256")?;

    if let Some(path) = optional::<String>(&source, "path")? {
        return Ok(LockedSource::Path {
            path: cx.resolve(&path),
            sha256,
        });
    }

    if let Some(file) = optional::<String>(&source, "file")? {
        return Ok(LockedSource::File {
            path: cx.resolve(&file),
            sha256,
        });
    }

    if let Some(url) = optional::<String>(&source, "url")? {
        return Ok(LockedSource::Url { url, sha256 });
    }

    Err(LuaError::RuntimeError(
        "package source requires one of `path`, `file`, or `url`".to_string(),
    ))
}

fn parse_install(install: Table) -> LuaResult<LockedInstall> {
    let strip_prefix = optional::<String>(&install, "strip_prefix")?.map(PathBuf::from);

    if let Some(path) = optional::<String>(&install, "binary")? {
        if strip_prefix.is_some() {
            return Err(LuaError::RuntimeError(
                "binary install cannot use strip_prefix".to_string(),
            ));
        }
        return Ok(LockedInstall::Binary {
            path: PathBuf::from(path),
        });
    }

    if optional::<bool>(&install, "directory")?.unwrap_or(false) {
        return Ok(LockedInstall::Directory { strip_prefix });
    }

    if let Some(archive) = optional::<String>(&install, "archive")? {
        let format = match archive.as_str() {
            "tar.gz" | "tgz" => ArchiveFormat::TarGz,
            "tar.xz" | "txz" => ArchiveFormat::TarXz,
            "zip" => ArchiveFormat::Zip,
            other => {
                return Err(LuaError::RuntimeError(format!(
                    "unsupported package archive format `{other}`"
                )));
            }
        };

        return Ok(LockedInstall::Archive {
            format,
            strip_prefix,
        });
    }

    Err(LuaError::RuntimeError(
        "package install requires `directory`, `archive`, or `binary`".to_string(),
    ))
}

fn parse_provides(bins: Table) -> LuaResult<Provides> {
    let mut out = BTreeMap::new();
    for pair in bins.pairs::<String, String>() {
        let (name, path) = pair?;
        out.insert(name, PathBuf::from(path));
    }

    Ok(Provides { bins: out })
}

fn required<T>(table: &Table, field: &str) -> LuaResult<T>
where
    T: mlua::FromLua,
{
    table.get(field).map_err(|e| {
        LuaError::RuntimeError(format!(
            "package field `{field}` is required or invalid: {e}"
        ))
    })
}

fn optional<T>(table: &Table, field: &str) -> LuaResult<Option<T>>
where
    T: mlua::FromLua,
{
    table.get(field)
}
