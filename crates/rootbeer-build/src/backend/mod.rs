use std::path::Path;

use rootbeer_package::{BuildBackend, SourceBuild};

mod autotools;
mod custom;
mod rust;
mod zig;

#[derive(Debug, Clone)]
pub struct Phase {
    pub name: &'static str,
    pub requires_network: bool,
    pub commands: Vec<Vec<String>>,
}

pub struct Context<'a> {
    pub prefix: &'a Path,
    pub downloads: &'a Path,
    pub host_tools: &'a Path,
    pub bins: &'a [String],
    pub dependencies: &'a Path,
    pub tools: &'a Path,
    pub workspace: &'a Path,
    pub jobs: usize,
    pub runtime: &'a std::collections::BTreeMap<String, std::path::PathBuf>,
}

impl Context<'_> {
    pub fn expand(&self, argument: &str) -> String {
        let mut expanded = argument
            .replace("{prefix}", &self.prefix.to_string_lossy())
            .replace("{dependencies}", &self.dependencies.to_string_lossy())
            .replace("{jobs}", &self.jobs.to_string());
        for (package, directory) in self.runtime {
            expanded = expanded.replace(
                &format!("{{runtime:{package}}}"),
                &directory.to_string_lossy(),
            );
        }
        expanded
    }
}

pub fn plan(build: &SourceBuild, context: &Context<'_>) -> Result<Vec<Phase>, String> {
    match build.backend {
        BuildBackend::Autotools => Ok(autotools::plan(&build.configure, context)),
        BuildBackend::Custom => custom::plan(build.steps.as_ref(), context),
        BuildBackend::Zig => zig::plan(&build.args, context),
        BuildBackend::Rust => {
            rust::plan(build.rust.as_ref().ok_or("missing Rust settings")?, context)
        }
    }
}
