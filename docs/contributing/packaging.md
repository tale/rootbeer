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

## Import GitHub projects

Generate candidates without editing the current catalog:

```sh
rb package --catalog recipes import github:owner/tool \
  --name tool --bin tool --output candidates
```

The output contains `recipes/tool.lua` with exact version and asset names, and
`upstreams/tool.lua` with reusable discovery rules and GitHub's repository ID.
Keep upstream definitions alongside the index's recipes; they are authoring inputs,
not part of the published catalog. The output directory must not already exist.

Names default to the lowercase repository name. Use `--name` to choose the canonical
identity and repeat `--alias` for alternate names. The importer rejects names and
aliases owned by another package, duplicate upstream projects, and changes to a
recorded repository ID or location. It checks the selected `--catalog`; the embedded
fallback alone cannot detect collisions with every official package.

Exported commands are explicit: repeat `--bin` for each command. Checks default to
`--version` for each command; use `--check '["tool", "--help"]'` or edit the saved
checks to exercise real functionality. These checks run during qualification, not
metadata discovery.

Discovery considers all four supported platforms by default. Repeat `--system` to
request a smaller set. Ambiguous assets stop the import; select a reusable pattern
with `--asset 'x86_64-linux=tool-{tag}-x86_64-unknown-linux-musl.tar.gz'`.
Patterns support `{tag}` and `{version}`. Existing recipes seed asset rules when
available, and newly discovered asset names become saved patterns.

The importer orders stable dotted numeric versions, including calendar versions;
it skips drafts and releases marked as prereleases. Tags may have an optional `v`.
Use `--tag-prefix` to restrict and strip another prefix. Other version schemes fail
explicitly. Discovery reads paginated release history, up to `--max-pages` (20 by
default), and fails if the history is incomplete. `GITHUB_TOKEN` authenticates API
requests.

Each platform selects its newest matching release. Existing recipes stay intact,
defaults never downgrade, and discontinued targets retain their older defaults.
Changing an existing version's assets or checks requires a manual recipe revision.
Missing required targets fail for new packages. Generated candidates still need
the platform checks below before publication.

## Batch imports and updates

Put one saved Lua definition per canonical name in `upstreams/`, then run:

```sh
rb package --catalog recipes import --upstreams upstreams --output candidates
rb package --catalog candidates/recipes check
rb package --catalog candidates/recipes export \
  --registry tale/rootbeer-index --output result
```

Rerunning those definitions discovers newer versions using the same rules. The
candidate directory contains only the requested packages, including their retained
versions. A failed batch leaves no output directory. After qualification, review
and copy the candidate recipes and upstream definitions into the index repository.

This is the shared import/update engine. Scheduled discovery, conditional metadata
caching, and automatic promotion are not implemented yet. The importer does not
download binaries, execute imported code, or publish candidates.

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
