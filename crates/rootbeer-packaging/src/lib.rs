//! Package qualification and publication, independent of configuration execution.

pub use rootbeer_build::{
    audit, build_package, verify_environment, BuildCache, BuildEnvironment, BuildOptions, BuildPlan,
};
pub use rootbeer_package::*;
mod bundle;
mod export;
mod publication;
pub use bundle::bundle_artifacts;
pub use export::{
    export_catalog, export_catalog_shard, export_catalog_with_cache, export_catalog_with_workers,
    ExportCache, ExportShard,
};
pub use publication::{assemble_indexes, publish_index, PublishOptions};

mod sign;
pub use sign::sign_index;

pub mod upstream;
pub use upstream::{
    discover_definition_updates, discover_updates, import_github_packages, seed_upstreams,
    UpdateReport,
};

#[cfg(test)]
#[path = "../../rootbeer-package/tests/support/catalog.rs"]
mod test_catalog;
