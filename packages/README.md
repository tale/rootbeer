# Embedded package catalog

This directory contains Rootbeer's smaller fallback catalog and package-engine
test fixtures. The [official index](https://github.com/tale/rootbeer-index) owns the
complete collection; contribute new tools there.

Each Lua file defines one canonical package. Recipes are validated and embedded in
`rb`, so updating this fallback requires a Rootbeer build. See the
[package authoring guide](../docs/contributing/packaging.md) for the schema, source
builds, discovery, and publication commands.

## Edit a recipe

Start with [`jq.lua`](jq.lua) for an upstream binary or [`xz.lua`](xz.lua) for a
source build.

- Match the filename to the canonical lowercase package name.
- Declare exact versions, supported platforms, exported commands, and useful checks.
- Keep older versions and increment the revision when changing an existing recipe.
- Review inherited settings across every retained version. Use version overrides
  when historical releases differ.
- Use `default_versions` when a platform's newest supported release differs from
  `default_version`. Explicit version requests stay exact.

Validate the catalog and inspect an expanded recipe:

```sh
cargo build --release --bin rb
target/release/rb package --catalog packages check
target/release/rb package --catalog packages show jq
target/release/rb package --catalog packages index > /tmp/rootbeer-index.json
```

Verify downloads, builds, command checks, and offline replay for your platform:

```sh
target/release/rb package --catalog packages export \
  --registry tale/rootbeer-index --output /tmp/rootbeer-export
```

The output directory must be new. Initial verification requires network access;
source recipes also require their build tools. Source builds execute trusted
upstream code and are not OS-sandboxed or hermetic.

`--catalog` selects recipes for package authoring commands. It does not change
`rb apply` resolution. Normal installation uses prebuilt packages from the signed
index, with the embedded collection available as a fallback. See
[index hosting and trust](../docs/contributing/package-hosting.md) for catalog
selection and [package locks](../docs/guide/package-locks.md) for reproducibility.
