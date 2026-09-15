use std::collections::BTreeMap;
use std::path::PathBuf;

use mlua::{Error as LuaError, Lua, Result as LuaResult, Table, Value};

use super::ctx::Ctx;
use super::module::Module;
use super::vm::{profile_bin_path, PackageBins};
use crate::package::{
    lockfile::RootbeerLock, profile as package_profile, ArchiveFormat, LockedInstall,
    LockedPackage, LockedSource, PackageIndexPin, PackageIntent, PackageRequest, Provides,
    ResolveContext,
};
use crate::plan::{Op, WriteSource};

pub(crate) struct Package;

impl Module for Package {
    const NAME: &'static str = "";

    fn build(lua: &Lua, t: &Table) -> LuaResult<()> {
        t.set(
            "package_index",
            lua.create_function(|lua, spec: Table| {
                let cx = Ctx::from(lua);
                if lua.app_data_ref::<PackageIndexPin>().is_some()
                    || cx
                        .run
                        .lock()
                        .iter()
                        .any(|op| matches!(op, Op::Package { .. }))
                {
                    return Err(LuaError::RuntimeError(
                        "declare package_index once, before packages".into(),
                    ));
                }
                let pin = PackageIndexPin {
                    url: required(&spec, "url")?,
                    sha256: required(&spec, "sha256")?,
                };
                pin.validate().map_err(LuaError::RuntimeError)?;
                drop(cx);
                lua.set_app_data(pin);
                Ok(())
            })?,
        )?;
        t.set(
            "package",
            lua.create_function(|lua, (spec, opts): (Value, Option<Table>)| {
                let parsed = parse_intent(lua, spec, opts)?;
                append_packages(lua, vec![parsed]);
                Ok(())
            })?,
        )?;
        t.set(
            "packages",
            lua.create_function(|lua, specs: Table| {
                let entries = batch_entries(specs)?;
                let parsed = entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, spec)| {
                        parse_intent(lua, spec, None).map_err(|error| {
                            LuaError::RuntimeError(format!(
                                "rb.packages entry {}: {error}",
                                index + 1
                            ))
                        })
                    })
                    .collect::<LuaResult<Vec<_>>>()?;
                append_packages(lua, parsed);
                Ok(())
            })?,
        )?;
        t.set(
            "bin_dir",
            lua.create_function(|_, ()| {
                Ok(package_profile::bin_dir().to_string_lossy().into_owned())
            })?,
        )?;
        t.set(
            "bin_path",
            lua.create_function(|_, value: Value| {
                let name = String::parse(value)?;
                validate_command(&name)?;
                Ok(profile_bin_path(&name).to_string_lossy().into_owned())
            })?,
        )?;

        t.set(
            "which",
            lua.create_function(|lua, value: Value| {
                let bin = String::parse(value)?;
                validate_command(&bin)?;
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

fn append_packages(lua: &Lua, parsed: Vec<(PackageIntent, Vec<String>)>) {
    let cx = Ctx::from(lua);
    let bins = lua
        .app_data_ref::<PackageBins>()
        .expect("PackageBins not set");
    for (intent, names) in parsed {
        for name in names {
            bins.insert(name.clone(), profile_bin_path(&name));
        }
        cx.push(Op::Package { intent });
    }
}

fn parse_intent(
    lua: &Lua,
    spec: Value,
    opts: Option<Table>,
) -> LuaResult<(PackageIntent, Vec<String>)> {
    if opts.is_some() && !matches!(&spec, Value::String(_)) {
        return Err(LuaError::RuntimeError(
            "package options require a request string".into(),
        ));
    }
    let cx = Ctx::from(lua);
    let (request, opts, is_structured) = match spec {
        Value::String(value) => (value.to_str()?.to_string(), opts, false),
        Value::Table(table) if !matches!(table.raw_get::<Value>("request")?, Value::Nil) => {
            fields(&table, &["request", "asset", "bins"])?;
            let request = required(&table, "request")?;
            (request, Some(table), true)
        }
        Value::Table(table) => {
            let package = parse_package(&cx, table)?;
            let names = package.provides.bins.keys().cloned().collect();
            return Ok((PackageIntent::locked(package), names));
        }
        other => {
            return Err(LuaError::RuntimeError(format!(
                "rb.package expected a package table or request string, got {}",
                other.type_name()
            )))
        }
    };
    let mut request = PackageRequest::parse(&request);
    if request.name.trim().is_empty()
        || request
            .version
            .as_ref()
            .is_some_and(|version| version.trim().is_empty())
    {
        return Err(LuaError::RuntimeError(
            "package request must name a package and a nonempty version when specified".into(),
        ));
    }
    if let Some(opts) = opts {
        fields(
            &opts,
            if is_structured {
                &["request", "asset", "bins"]
            } else {
                &["asset", "bins"]
            },
        )?;
        request.asset = optional(&opts, "asset")?;
        let bins = optional::<Table>(&opts, "bins")?;
        if (!is_structured || request.asset.is_some() || bins.is_some())
            && request.resolver.as_deref() != Some("github")
        {
            return Err(LuaError::RuntimeError(
                "package asset and bins options require the github resolver".into(),
            ));
        }
        if request
            .asset
            .as_ref()
            .is_some_and(|asset| asset.trim().is_empty())
        {
            return Err(LuaError::RuntimeError(
                "package asset must be nonempty".into(),
            ));
        }
        if let Some(bins) = bins {
            request.bins = parse_provides(bins)?.bins;
        }
    }
    let mut names: Vec<String> = request.bins.keys().cloned().collect();
    names.extend(request_bins(lua, &cx, &request)?);
    for name in &names {
        validate_command(name)?;
    }
    Ok((PackageIntent::request(request), names))
}

fn batch_entries(table: Table) -> LuaResult<Vec<Value>> {
    let mut entries = BTreeMap::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let index = match key {
            Value::Integer(index) if index > 0 => index as usize,
            Value::Number(index)
                if index >= 1.0 && index.fract() == 0.0 && index < usize::MAX as f64 =>
            {
                index as usize
            }
            _ => {
                return Err(LuaError::RuntimeError(
                    "rb.packages expects a dense list with positive integer keys".into(),
                ))
            }
        };
        entries.insert(index, value);
    }
    if !entries.keys().copied().eq(1..=entries.len()) {
        return Err(LuaError::RuntimeError(
            "rb.packages expects a dense list without missing entries".into(),
        ));
    }
    Ok(entries.into_values().collect())
}

fn request_bins(lua: &Lua, cx: &Ctx<'_>, request: &PackageRequest) -> LuaResult<Vec<String>> {
    if let Ok(lock) = RootbeerLock::read(cx.runtime.script_dir.join("rootbeer.lock")) {
        if lock.inputs.explicit_package_index() == lua.app_data_ref::<PackageIndexPin>().as_deref()
        {
            if let Ok(package) = lock.package_for_request(request, &ResolveContext::current()) {
                return Ok(package.provides.bins.keys().cloned().collect());
            }
        }
    }
    Ok(Vec::new())
}

fn fields(table: &Table, allowed: &[&str]) -> LuaResult<()> {
    for pair in table.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let Value::String(key) = key else {
            return Err(LuaError::RuntimeError(
                "package fields must be named strings".into(),
            ));
        };
        let key = key.to_str()?;
        if !allowed.contains(&key.as_ref()) {
            return Err(LuaError::RuntimeError(format!(
                "unknown package field `{key}`"
            )));
        }
    }
    Ok(())
}

fn validate_command(name: &str) -> LuaResult<()> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err(LuaError::RuntimeError(
            "command name must be one nonempty normal filename".into(),
        ));
    }
    Ok(())
}

fn relative_path(path: &str) -> LuaResult<PathBuf> {
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(LuaError::RuntimeError(
            "package install paths must be nonempty relative paths without traversal".into(),
        ));
    }
    Ok(path)
}

fn parse_package(cx: &Ctx<'_>, spec: Table) -> LuaResult<LockedPackage> {
    parse_package_at_depth(cx, spec, 0)
}

fn parse_package_at_depth(cx: &Ctx<'_>, spec: Table, depth: usize) -> LuaResult<LockedPackage> {
    if depth >= 64 {
        return Err(LuaError::RuntimeError(
            "runtime dependency depth exceeds 64".into(),
        ));
    }
    fields(
        &spec,
        &[
            "name",
            "version",
            "source",
            "install",
            "bins",
            "apps",
            "runtime_dependencies",
            "output_sha256",
        ],
    )?;
    let name: String = required(&spec, "name")?;
    validate_command(&name)?;
    let version: String = required(&spec, "version")?;
    let source = parse_source(cx, required(&spec, "source")?)?;
    let install = parse_install(required(&spec, "install")?)?;
    let mut provides = parse_provides(required(&spec, "bins")?)?;
    if let Some(apps) = optional::<Table>(&spec, "apps")? {
        for pair in apps.pairs::<Value, Value>() {
            let (name, path) = pair?;
            let name = String::parse(name)?;
            validate_command(&name)?;
            let path = relative_path(&String::parse(path)?)?;
            if !name.ends_with(".app")
                || name.len() <= 4
                || path.extension().is_none_or(|extension| extension != "app")
            {
                return Err(LuaError::RuntimeError(
                    "application exports require .app names and bundle paths".into(),
                ));
            }
            provides.apps.insert(name, path);
        }
    }

    let mut runtime_dependencies = BTreeMap::new();
    if let Some(runtime) = optional::<Table>(&spec, "runtime_dependencies")? {
        for pair in runtime.pairs::<String, Table>() {
            let (key, spec) = pair?;
            runtime_dependencies.insert(key, parse_package_at_depth(cx, spec, depth + 1)?);
        }
    }
    let output_sha256: Option<String> = optional(&spec, "output_sha256")?;
    if output_sha256
        .as_deref()
        .is_some_and(|hash| !rootbeer_package::index::is_sha256(hash))
    {
        return Err(LuaError::RuntimeError(
            "output_sha256 must be a lowercase SHA-256".into(),
        ));
    }
    let package = LockedPackage {
        name,
        version,
        source,
        install,
        provides,
        runtime_dependencies,
        output_sha256,
    };
    rootbeer_package::runtime::closure(&package).map_err(LuaError::RuntimeError)?;
    Ok(package)
}

fn parse_source(cx: &Ctx<'_>, source: Table) -> LuaResult<LockedSource> {
    fields(&source, &["sha256", "path", "file", "url"])?;
    let variants = ["path", "file", "url"]
        .into_iter()
        .map(|key| source.raw_get::<Value>(key))
        .collect::<LuaResult<Vec<_>>>()?;
    if variants
        .iter()
        .filter(|value| !matches!(value, Value::Nil))
        .count()
        != 1
    {
        return Err(LuaError::RuntimeError(
            "package source requires exactly one of path, file, or url".into(),
        ));
    }
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
    fields(
        &install,
        &["binary", "directory", "archive", "strip_prefix"],
    )?;
    let binary = optional::<String>(&install, "binary")?;
    let directory = optional::<bool>(&install, "directory")?.unwrap_or(false);
    let archive = optional::<String>(&install, "archive")?;
    if usize::from(binary.is_some()) + usize::from(directory) + usize::from(archive.is_some()) != 1
    {
        return Err(LuaError::RuntimeError(
            "package install requires exactly one of directory, archive, or binary".into(),
        ));
    }
    let strip_prefix = optional::<String>(&install, "strip_prefix")?
        .map(|path| relative_path(&path))
        .transpose()?;

    if let Some(path) = binary {
        if strip_prefix.is_some() {
            return Err(LuaError::RuntimeError(
                "binary install cannot use strip_prefix".to_string(),
            ));
        }
        return Ok(LockedInstall::Binary {
            path: relative_path(&path)?,
        });
    }

    if directory {
        return Ok(LockedInstall::Directory { strip_prefix });
    }

    if let Some(archive) = archive {
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
    for pair in bins.pairs::<Value, Value>() {
        let (name, path) = pair?;
        let name = String::parse(name)?;
        validate_command(&name)?;
        out.insert(name, relative_path(&String::parse(path)?)?);
    }

    Ok(Provides {
        apps: Default::default(),
        bins: out,
    })
}

trait PackageField: Sized {
    fn parse(value: Value) -> LuaResult<Self>;
}

impl PackageField for String {
    fn parse(value: Value) -> LuaResult<Self> {
        match value {
            Value::String(value) => Ok(value.to_str()?.to_string()),
            other => Err(LuaError::RuntimeError(format!(
                "expected string, got {}",
                other.type_name()
            ))),
        }
    }
}

impl PackageField for Table {
    fn parse(value: Value) -> LuaResult<Self> {
        match value {
            Value::Table(value) => Ok(value),
            other => Err(LuaError::RuntimeError(format!(
                "expected table, got {}",
                other.type_name()
            ))),
        }
    }
}

impl PackageField for bool {
    fn parse(value: Value) -> LuaResult<Self> {
        match value {
            Value::Boolean(value) => Ok(value),
            other => Err(LuaError::RuntimeError(format!(
                "expected boolean, got {}",
                other.type_name()
            ))),
        }
    }
}

fn required<T: PackageField>(table: &Table, field: &str) -> LuaResult<T> {
    T::parse(table.raw_get(field)?).map_err(|error| {
        LuaError::RuntimeError(format!(
            "package field `{field}` is required or invalid: {error}"
        ))
    })
}

fn optional<T: PackageField>(table: &Table, field: &str) -> LuaResult<Option<T>> {
    match table.raw_get(field)? {
        Value::Nil => Ok(None),
        value => T::parse(value).map(Some).map_err(|error| {
            LuaError::RuntimeError(format!("invalid package field `{field}`: {error}"))
        }),
    }
}
