use std::collections::BTreeMap;
use std::path::PathBuf;

use mlua::{Error as LuaError, Lua, Result as LuaResult, Table, Value};

use super::ctx::Ctx;
use super::module::Module;
use super::vm::{profile_bin_path, PackageBins};
use crate::package::{
    default_resolver_stack, ArchiveFormat, LockedInstall, LockedPackage, LockedSource,
    PackageRequest, Provides, ResolveContext,
};
use crate::plan::Op;

pub(crate) struct Package;

impl Module for Package {
    const NAME: &'static str = "";

    fn build(lua: &Lua, t: &Table) -> LuaResult<()> {
        t.set(
            "package",
            lua.create_function(|lua, spec: Value| {
                let cx = Ctx::from(lua);
                let package = match spec {
                    Value::Table(spec) => parse_package(&cx, spec)?,
                    Value::String(spec) => resolve_package_request(spec.to_str()?.as_ref())?,
                    other => {
                        return Err(LuaError::RuntimeError(format!(
                            "rb.package expected a package table or request string, got {}",
                            other.type_name()
                        )));
                    }
                };

                register_package_bins(lua, &package);
                cx.push(Op::RealizePackage { package });
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

        Ok(())
    }
}

fn resolve_package_request(input: &str) -> LuaResult<LockedPackage> {
    let request = PackageRequest::parse(input);
    default_resolver_stack()
        .resolve(&request, &ResolveContext::current())
        .map_err(|e| LuaError::RuntimeError(e.to_string()))
}

fn register_package_bins(lua: &Lua, package: &LockedPackage) {
    let bins = lua
        .app_data_ref::<PackageBins>()
        .expect("PackageBins not set");

    for bin in package.provides.bins.keys() {
        bins.insert(bin.clone(), profile_bin_path(bin));
    }
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

    if optional::<bool>(&install, "directory")?.unwrap_or(false) {
        return Ok(LockedInstall::Directory { strip_prefix });
    }

    if let Some(archive) = optional::<String>(&install, "archive")? {
        let format = match archive.as_str() {
            "tar.gz" | "tgz" => ArchiveFormat::TarGz,
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
        "package install requires `directory = true` or `archive = \"tar.gz\"`".to_string(),
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
