//! Package qualification and publication, independent of configuration execution.

pub use rootbeer_build::{
    audit, build_package, verify_environment, BuildCache, BuildEnvironment, BuildOptions, BuildPlan,
};
pub use rootbeer_package::*;
mod bundle;
mod checks;
mod prepare;
pub use prepare::prepare_package;
mod export;
mod package_plan;
mod publication;
mod publish_records;
mod release;
pub use bundle::bundle_artifacts;
pub use export::{
    candidate_files, checkpoint_results, export_catalog, export_catalog_shard,
    export_catalog_with_cache, export_catalog_with_workers, import_results,
    import_results_for_system, plan_export, CandidateFiles, ExportCache, ExportDecision,
    ExportPlan, ExportShard,
};
pub use package_plan::{plan_packages, PackageTask};
pub use publication::{
    assemble_indexes, publish_index, verify_bundle, verify_candidate, PublishOptions,
};
pub use publish_records::publish_records;
pub use release::{push_package, release_package};

mod sign;
pub use sign::{sign_index, sign_package_record};

pub mod upstream;
pub use upstream::{
    discover_definition_updates, discover_updates, import_github_packages, seed_upstreams,
    UpdateReport,
};

#[cfg(test)]
#[path = "../../rootbeer-package/tests/support/catalog.rs"]
mod test_catalog;

#[cfg(test)]
#[path = "../../../scripts/cache_inputs.rs"]
mod cache_inputs;
