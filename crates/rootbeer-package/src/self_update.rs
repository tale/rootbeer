//! Side channel that resolves Rootbeer itself, and nothing else.
//!
//! `rb self-update` is how a client recovers, so it must keep working however far the
//! repository has moved ahead of this build. Roots and package documents are decoded one entry
//! at a time, so anything newer elsewhere cannot reach Rootbeer's own entry; this channel adds
//! only the guarantee that it resolves no package other than [`PACKAGE`].

use std::path::PathBuf;

use crate::{
    PackageRequest, PackageResolution, PackageResolver, RepositoryPin, RepositoryResolver,
    ResolveContext,
};

/// The only package this channel will ever resolve.
pub const PACKAGE: &str = "rootbeer";

/// Resolves [`PACKAGE`] from a pinned root, and refuses every other request.
pub struct Resolver {
    repository: RepositoryResolver,
}

impl Resolver {
    pub fn new(pin: &RepositoryPin, cache: impl Into<PathBuf>) -> Self {
        Self {
            repository: RepositoryResolver::with_cache(pin, cache, false),
        }
    }
}

impl PackageResolver for Resolver {
    fn name(&self) -> &str {
        "rootbeer"
    }

    fn resolve(
        &self,
        request: &PackageRequest,
        context: &ResolveContext,
    ) -> Result<Option<PackageResolution>, String> {
        if request.name != PACKAGE {
            return Err(format!("self-update resolves {PACKAGE} only"));
        }
        self.repository.resolve(request, context)
    }
}
