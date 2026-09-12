# Contribute packages

Package definitions live in the [official index repository](https://github.com/tale/rootbeer-index).
Rootbeer owns the package engine; the index owns recipes and publication. A new
recipe normally needs no Rust changes or Rootbeer release.

The `packages/` directory in the engine repository is the smaller embedded fallback
and authoring fixture. It is not the complete hosted catalog.

## Recipe contract

A Lua recipe declares a canonical name, description, homepage, exact versions,
revision, supported platforms, exported commands, and executable checks. Each
version imports an upstream binary or describes a supported source build.

Choose the newest upstream release available for each platform. Use
`default_versions` when a discontinued target needs an older default. Preserve
older version recipes, and increment the revision when changing an existing one.

Prefer direct GitHub releases with explicit per-platform asset names. Source
recipes currently use Autotools with declared build dependencies. Verify source
bytes before recording their hashes. See the
[recipe authoring reference](https://github.com/tale/rootbeer/tree/main/packages)
for the format and the index repository's contributor instructions for policy.

## Qualify a change

```sh
rb package --catalog recipes check
rb package --catalog recipes export --registry tale/rootbeer-index --output result
```

Export installs every applicable version, runs its command checks, and recreates
its locked outputs offline. Local success only qualifies the current platform.
The index workflow checks all declared Linux/macOS and ARM64/x86-64 combinations
before a package becomes publishable.

Checks should exercise the package's actual functionality and bundled runtime
files where appropriate. They run as argument arrays, without shell interpolation.
Source builds execute trusted upstream code with host tools; they are not OS
sandboxes or hermetic builds. Do not depend accidentally on a developer's Homebrew
installation or CI-only libraries.

## Reuse successful work

The export cache reuses verified results when a recipe, its transitive dependency
recipes, platform, registry, engine, and build environment match. A new package
must not force unrelated packages to rebuild. Original receipts stay intact when
results are reused in a newer catalog snapshot.

```sh
rb package --catalog recipes export --registry tale/rootbeer-index --output result \
  --cache /tmp/rootbeer-package-results --cache-context "$BUILD_ENVIRONMENT_ID"
```

Use a trusted cache and identify the OS image and tools in `BUILD_ENVIRONMENT_ID`.
A missing entry runs full verification; corruption fails. `--recheck` bypasses
reuse. CI keeps scheduled full checks to detect upstream and platform drift.

## Publish independently

`rb package assemble` merges platform bundles and requires complete coverage.
`rb package publish` uploads source-built archives to GHCR, verifies anonymous
access, and signs an immutable index snapshot. Imported binaries retain their
upstream URLs. CI retains snapshots and receipts, then deploys the latest signed
manifest through GitHub Pages.

Source-build jobs receive no publication credentials. The separate publisher uses
ORAS for uploads; users need neither ORAS nor a container runtime to install.
Package publication does not create Rootbeer GitHub releases.

Read [index hosting and trust](/contributing/package-hosting) for deployment
boundaries and endpoint changes.
