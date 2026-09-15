use std::collections::BTreeMap;
use std::fs;

use rootbeer_package::lockfile::{PackageLockEntry, RootbeerLock};
use rootbeer_package::*;
use rootbeer_store::hash_bytes;

use crate::consumer::SourceResolver;

#[test]
fn source_fallback_builds_cached_inputs_and_published_artifacts_take_precedence() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    let source = root.path().join("source");
    fs::create_dir_all(source.join("fixture")).unwrap();
    fs::write(
        source.join("fixture/configure"),
        r#"#!/bin/sh
set -eu
cat > Makefile <<'EOF'
all:
	printf '#!/bin/sh\nexit 0\n' > xz
	chmod +x xz
check: all
	./xz
install:
	mkdir -p "$(DESTDIR)/bin"
	cp xz "$(DESTDIR)/bin/xz"
EOF
"#,
    )
    .unwrap();
    let archive = root.path().join("source.tar.gz");
    crate::pack(&source, &archive).unwrap();
    let cached = download::DownloadCache::new(state.join("downloads"))
        .materialize(&format!("file://{}", archive.display()), None)
        .unwrap();
    let mut catalog = crate::test_catalog::catalog().clone();
    catalog.packages.retain(|name, _| name == "xz");
    let recipe = catalog
        .packages
        .get_mut("xz")
        .unwrap()
        .versions
        .get_mut("5.8.3")
        .unwrap();
    recipe.bins = vec!["xz".into()];
    recipe.checks = vec![vec!["xz".into()]];
    let build = recipe.build.as_mut().unwrap();
    build.url = "https://source.invalid/archive.tar.gz".into();
    build.sha256 = cached.sha256.clone();
    build.strip_prefix = "fixture".into();
    build.configure.clear();
    let mut index = ArtifactIndex {
        schema: 7,
        catalog_sha256: catalog.sha256(),
        catalog,
        artifacts: BTreeMap::new(),
    };
    index.validate_complete().unwrap();
    let save = |index: &ArtifactIndex| {
        let bytes = serde_json::to_vec(index).unwrap();
        let path = root.path().join(format!("{}.json", hash_bytes(&bytes)));
        fs::write(&path, &bytes).unwrap();
        PackageIndexPin {
            url: format!("file://{}", path.display()),
            sha256: hash_bytes(&bytes),
        }
    };
    let context = ResolveContext::current();
    let pin = save(&index);
    let resolver = SourceResolver::new(&pin, &state);
    let request = PackageRequest::parse("xz@5.8.3");
    let built = resolver.resolve(&request, &context).unwrap().unwrap();
    let ResolutionProof::SourceBuild(proof) = &built.proof else {
        panic!("expected source fallback");
    };
    assert_eq!(proof.source_sha256, cached.sha256);
    assert_eq!(proof.index, pin);
    let repeated = resolver.resolve(&request, &context).unwrap().unwrap();
    let ResolutionProof::SourceBuild(repeated_proof) = repeated.proof else {
        panic!("expected source build");
    };
    assert_eq!(proof.build_key, repeated_proof.build_key);
    assert_eq!(built.package.output_sha256, repeated.package.output_sha256);
    let mut published = built.package.clone();
    published.source = LockedSource::Url {
        url: "https://packages.invalid/prebuilt.tar.gz".into(),
        sha256: "a".repeat(64),
    };
    index.artifacts.insert(
        published.id(),
        BTreeMap::from([(
            context.system.clone(),
            PublishedArtifact {
                revision: 2,
                receipt_sha256: "b".repeat(64),
                package: published.clone(),
            },
        )]),
    );
    index.validate_complete().unwrap();
    let resolver = SourceResolver::new(&save(&index), &state);
    let normal = resolver.resolve(&request, &context).unwrap().unwrap();
    assert!(matches!(normal.proof, ResolutionProof::PublishedIndex(_)));
    assert_eq!(normal.package, published);
    let forced = PackageRequest::parse("xz@source:5.8.3");
    assert!(matches!(
        resolver.resolve(&forced, &context).unwrap().unwrap().proof,
        ResolutionProof::SourceBuild(_)
    ));
    let lock = RootbeerLock::from_package_entries([PackageLockEntry::resolved(
        &request, &context, normal,
    )
    .unwrap()])
    .unwrap();
    assert!(lock.package_for_request(&forced, &context).is_err());
    index
        .catalog
        .packages
        .get_mut("xz")
        .unwrap()
        .versions
        .get_mut("5.8.3")
        .unwrap()
        .build = None;
    let recipe = index
        .catalog
        .packages
        .get_mut("xz")
        .unwrap()
        .versions
        .get_mut("5.8.3")
        .unwrap();
    recipe.source = Some("github:owner/xz@5.8.3".into());
    index.catalog_sha256 = index.catalog.sha256();
    let resolver = SourceResolver::new(&save(&index), &state);
    assert!(resolver
        .resolve(&PackageRequest::parse("xz@HEAD"), &context)
        .unwrap_err()
        .contains("prebuilt-only"));
}
