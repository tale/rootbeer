use std::path::Path;

use rootbeer_package::{BuildBackend, SourceBuild};

mod autotools;
mod custom;
mod zig;

#[derive(Debug, Clone)]
pub struct Phase {
    pub name: &'static str,
    pub commands: Vec<Vec<String>>,
}

pub struct Context<'a> {
    pub prefix: &'a Path,
    pub dependencies: &'a Path,
    pub tools: &'a Path,
    pub workspace: &'a Path,
    pub jobs: usize,
}

impl Context<'_> {
    pub fn expand(&self, argument: &str) -> String {
        argument
            .replace("{prefix}", &self.prefix.to_string_lossy())
            .replace("{dependencies}", &self.dependencies.to_string_lossy())
            .replace("{jobs}", &self.jobs.to_string())
    }
}

pub fn plan(build: &SourceBuild, context: &Context<'_>) -> Result<Vec<Phase>, String> {
    match build.backend {
        BuildBackend::Autotools => Ok(autotools::plan(&build.configure, context)),
        BuildBackend::Custom => custom::plan(build.steps.as_ref(), context),
        BuildBackend::Zig => zig::plan(&build.args, context),
    }
}
