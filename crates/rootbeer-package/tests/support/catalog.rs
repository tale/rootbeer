use rootbeer_package::PackageCatalog;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub fn catalog() -> &'static PackageCatalog {
    static CATALOG: OnceLock<PackageCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let systems = ["aarch64-macos", "aarch64-linux", "x86_64-linux"];
        let mut packages = BTreeMap::new();
        for (name, repository, versions, prefix, revision, bins) in [
            ("age", "FiloSottile/age", vec!["1.3.1"], "v", 3, vec!["age", "age-keygen"]),
            ("fd", "sharkdp/fd", vec!["10.4.2", "10.5.0"], "v", 2, vec!["fd"]),
            ("ripgrep", "BurntSushi/ripgrep", vec!["15.2.0"], "", 3, vec!["rg"]),
        ] {
            let recipes: BTreeMap<_, _> = versions.iter().map(|version| {
                let targets = if name == "age" {
                    ["darwin-arm64", "linux-arm64", "linux-amd64"]
                } else {
                    ["aarch64-apple-darwin", "aarch64-unknown-linux-musl", "x86_64-unknown-linux-musl"]
                };
                let assets: BTreeMap<_, _> = systems.iter().zip(targets).map(|(system, target)| {
                    (*system, format!("{name}-{prefix}{version}-{target}.tar.gz"))
                }).collect();
                (*version, json!({
                    "revision": revision, "source": format!("github:{repository}@{prefix}{version}"),
                    "systems": systems, "assets": assets, "bins": bins,
                    "checks": bins.iter().map(|bin| vec![*bin, "--version"]).collect::<Vec<_>>()
                }))
            }).collect();
            packages.insert(name, json!({
                "name": name, "description": "Resolver test package",
                "homepage": format!("https://github.com/{repository}"),
                "default_version": versions.last().unwrap(),
                "aliases": if name == "ripgrep" { vec!["rg"] } else { vec![] },
                "versions": recipes
            }));
        }
        packages.insert("xz", json!({
            "name": "xz", "description": "Source build test package",
            "homepage": "https://tukaani.org/xz/", "default_version": "5.8.3",
            "versions": {"5.8.3": {
                "revision": 2, "systems": systems,
                "bins": ["xz", "xzdec", "lzmadec", "lzmainfo"],
                "checks": [["xz", "--version"], ["xzdec", "--version"], ["lzmadec", "--version"], ["lzmainfo", "--version"]],
                "build": {
                    "backend": "autotools",
                    "url": "https://github.com/tukaani-project/xz/releases/download/v5.8.3/xz-5.8.3.tar.gz",
                    "sha256": "3d3a1b973af218114f4f889bbaa2f4c037deaae0c8e815eec381c3d546b974a0",
                    "archive": "tar.gz", "strip_prefix": "xz-5.8.3",
                    "configure": ["--disable-shared", "--enable-static", "--disable-nls", "--disable-scripts", "--disable-doc"]
                }
            }}
        }));
        let catalog: PackageCatalog = serde_json::from_value(json!({"schema": 1, "packages": packages})).unwrap();
        catalog.validate().unwrap();
        catalog
    })
}
