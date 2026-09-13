# Contribute packages

Package definitions live in the [official index repository](https://github.com/tale/rootbeer-index).
Rootbeer owns the package engine; the index owns recipes and publication. A new
recipe normally needs no Rust changes or Rootbeer release.

The `packages/` directory in the engine repository is the smaller embedded fallback
and authoring fixture. It is not the complete hosted catalog.

## Definition API

Declare shared source settings, commands, and checks once. Version entries contain
only their checksum, revision, or exceptions:

```lua
return {
    name = "tool",
    description = "A command-line tool",
    default_version = "2.0.0",
    source = {
        github = "owner/tool",
        tag = "v{version}",
        assets = {
            ["aarch64-linux"] = "tool-{tag}-linux-arm64.tar.gz",
            ["x86_64-linux"] = "tool-{tag}-linux-amd64.tar.gz",
            ["aarch64-macos"] = "tool-{tag}-darwin-arm64.tar.gz",
            ["x86_64-macos"] = "tool-{tag}-darwin-amd64.tar.gz",
        },
    },
    bins = { "tool" },
    checks = { { "tool", "--version" } },
    versions = {
        ["1.0.0"] = { revision = 2 },
        ["2.0.0"] = {},
    },
}
```

The filename matches `name`; aliases remain explicit. GitHub packages default their
homepage to the repository URL. Override `homepage` for a project website.

`source.assets` defines supported platforms unless `systems` is explicit. Patterns
accept `{version}` and `{tag}`; the tag template accepts `{version}`. Expansion
happens before catalog validation. Command arguments and configure flags are literal.
Unsupported placeholders and unknown fields fail validation.

A version inherits `bins`, `checks`, and `systems`; explicit arrays replace them.
Version `assets` override individual platforms, filtered to that version's systems.
Use `tag` for an exceptional release tag. Revisions default to 1. Empty arrays do
not mean inheritance: invalid empty command or platform contracts are rejected.
Discovery uses the shared command contract for new releases and preserves existing
versions' explicit checks, including exceptions on the current default.

Choose the newest release for each platform. `default_version` stays explicit;
`default_versions` selects older defaults for discontinued targets. Retain older
versions and their exceptions. Explicit version requests never fall back.

### Source builds

Use a shared `build` instead of `source`. It declares the supported `backend`
(currently `autotools`), archive format, source URL, strip prefix, configure flags,
and exact build dependencies. URL and strip prefix accept `{version}`. Build
packages must declare `homepage` and `systems` explicitly.

Each version supplies its own verified `sha256`; checksums cannot be shared or
inferred. A version's `build` can replace the complete build definition for a
historical exception, including its exact URL and checksum.

### Expansion and compatibility

Compact files expand into the same exact recipes used by resolution, caches,
qualification, and signed snapshots. Existing expanded Lua definitions still load.
`rb package index` shows the expanded result. Discovery preserves existing templates
and version overrides; expanded files stay expanded. Import and seeding infer compact
templates once for new definitions.
Migration preserves array ordering, so some older definitions keep explicit platform
lists even when their membership matches the asset map.

Shared changes affect every version that inherits them. Review expanded output,
retain old behavior with version overrides, or increment every affected revision.
Authoring-only changes must leave expanded recipes unchanged to reuse their results.

## Import GitHub projects

Generate candidates without editing the current catalog:

```sh
rb package --catalog packages import github:owner/tool \
  --name tool --bin tool --output candidates
```

The output contains one complete compact `packages/tool.lua`, including its
GitHub repository ID and reusable source patterns. The output directory must not
already exist.

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

To rediscover a selected directory of complete package definitions:

```sh
rb package --catalog packages import --packages selected-packages --output candidates
rb package --catalog candidates/packages check
rb package --catalog candidates/packages export \
  --registry tale/rootbeer-index --output result
```

Rerunning those definitions discovers newer versions using the same rules. The
candidate directory contains only the requested packages, including their retained
versions. A failed batch leaves no output directory. After qualification, review
and copy the complete candidate package files into the index repository.

The importer does not download binaries, execute imported code, or publish candidates.

## Track upstream updates

GitHub `source` settings drive both discovery and version expansion. A tag template
such as `tool-{version}` also selects that release series; `source.tag_prefix`
can override the discovery filter. Optional `v` prefixes remain accepted for
numeric tags. `source.update_systems` narrows discovery without removing retained
platform recipes. Set `source.track = false` to opt out. Source builds remain
untracked; automatic discovery currently supports GitHub binary releases only.

Run discovery directly against the package directory:

```sh
rb package --catalog packages updates \
  --cache .upstream-metadata --output candidates
```

For older catalogs without update rules, `seed-upstreams --output tracked-packages`
creates compact copies of GitHub-backed packages with inferred source patterns.
Review their asset patterns and tag filters before adopting them. The output must
be a new directory; seeding does not preserve customized discovery rules.

`updates` reports each project independently. It writes `report.json`, `summary.md`,
and complete files in `packages/` for changed recipes or discovery rules, including
newly recorded repository IDs. The report separates recipe changes (`updated`) from
rule changes (`rules_changed`); qualify only the former. A no-change scan writes no
package files and needs no qualification.
Errors return a nonzero exit status after writing the report and successful candidates.

The metadata cache sends GitHub ETags with conditional requests and reuses responses
only after HTTP 304. It verifies cached hashes and never treats network failures as
unchanged upstreams. Use a trusted cache directory; its entries are not signed.
Release histories are still checked page by page, and changed metadata can require
a fresh response even when no package version changes.

Legacy tags that do not belong to the tracked release series can be listed explicitly
in a package's `source.exclude_tags`, or supplied with repeated `--exclude-tag`
arguments during import. Unsupported tags otherwise remain visible errors.

The index's discovery workflow runs daily or manually, retains the report and
candidates as workflow artifacts, and qualifies changed packages on all four
platforms using verified-result caches. Discovery errors remain visible while
successful candidates can still be checked. It has read-only repository permissions
and does not publish, open pull requests, or advance defaults automatically.
Review qualified candidates before copying them into the index's `packages/` directory.

## Qualify a change

```sh
rb package --catalog packages check
rb package --catalog packages export --registry tale/rootbeer-index --output result
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
rb package --catalog packages export --registry tale/rootbeer-index --output result \
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
