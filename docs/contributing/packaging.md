# Contribute packages

Package definitions live in the [official index repository](https://github.com/tale/rootbeer-index).
Rootbeer owns the package engine; the index owns recipes and publication. A new
recipe normally needs no Rust changes or Rootbeer release.

The `packages/` directory in the engine repository is the smaller embedded fallback
and authoring fixture. It is not the complete hosted catalog.

## Packaging entrypoint

`rootbeer-forge` owns recipe inspection, builds, updates, qualification, and
publication. `rb` owns configuration and user package environments.

```sh
cargo build --bin rootbeer-forge
rootbeer-forge --catalog packages plan xz
rootbeer-forge --catalog packages build xz --output /tmp/xz-build \
  --cache /tmp/rootbeer-builds --cache-context "$BUILD_ENVIRONMENT_ID"
```

`plan` prints direct dependencies, transitive inputs, and execution order without
fetching or building anything. Build execution resolves binary inputs up front,
then uses those locked facts throughout the run.

The build cache reuses individual dependencies across different root packages.
Its key includes the recipe, verified dependency outputs and selected library
exports, platform, build engine, job count, and explicit environment identity.
Archive paths, unrelated catalog entries, and publication destinations do not
change build keys. Receipts retain the key and environment identity.

Set `BUILD_ENVIRONMENT_ID` to identify the host image, compiler, SDK, and system
tools. These are trusted host builds with network access, not isolated builds.
Change that identity whenever the host build environment changes. `--recheck`
rebuilds each node; `--phase-timeout SECONDS` overrides the default 20-minute
command limit. Successful outputs enter the cache only after verification and
checks; cache hits verify archive and output hashes and rerun package checks.

Export's existing `--cache` also enables this dependency cache under `builds/`.
Qualification results remain separate because they include publication details.

New recipes can scope dependencies explicitly:

```lua
dependencies = {
    { package = "cmake@4.0.0", kind = "build" },
    { package = "zlib@1.3.1", kind = "link" },
}
```

These are syntax examples; use versions available in your selected catalog.
`build` exposes the dependency’s commands. `link` exposes its libraries and
headers, including transitive link inputs, without leaking its build tools onto
`PATH`. String entries retain their previous combined behavior, also available
as `kind = "all"`. Scoped entries require artifact index schema 5 when published;
existing catalogs retain their current schema and digest.

`custom` is the authoring name for explicit build phases. The serialized catalog
continues to use `commands` for compatibility with existing pinned catalog
hashes; both spellings are accepted. No recipe migration is required.

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

### Archive commands and moving releases

Use `bin_paths` to map each declared command to its path inside a GitHub archive.
For example, bobrwm maps `bobrwm` to `Bobrwm.app/Contents/MacOS/bobrwm-cli`.
Mappings must cover every declared command and stay inside the archive. The full
archive tree is retained, including resources and app bundles.

Declare macOS app exports explicitly:

```lua
apps = { ["Bobrwm.app"] = "Bobrwm.app" },
```

Keys are exported `.app` filenames; values are relative paths to contained `.app`
directories. App recipes must target macOS only. Versions inherit `apps` unless
an explicit version map overrides it. `rb use` and Lua package application manage
links in `~/Applications`; existing apps and unmanaged links cause a conflict.
Temporary `rb run --app` launches do not create these links. Neither operation
configures login items or grants permissions.

A moving tag such as `tip` is not a package version. Give each approved snapshot
an exact version, select its exact asset name, and pin every platform's verified
SHA-256 in that version's `checksums` map. Set `mirror = true` to retain the
qualified package in the index's content-addressed registry. Receipts retain the
original upstream URL and checksum. Old installations then remain available even
if upstream replaces or removes the release assets.

Mirrored snapshots require manual checksum qualification; set `source.track = false`
instead of treating a moving tag as a stable release. Increment the recipe revision
if another build of the same version changes its bytes. Applications can be opened
from the verified store with `rb run bobrwm --app Bobrwm.app`.

### Source builds

Use a shared `build` instead of `source`. It declares the supported `backend`
(`autotools`, `zig`, or `custom`), archive format, source URL, strip prefix, and exact build
dependencies. Autotools accepts `configure` flags. URL and strip prefix accept `{version}`. Build
packages must declare `homepage` and `systems` explicitly.

Each version supplies its own verified `sha256`; checksums cannot be shared or
inferred. A version's `build` can replace the complete build definition for a
historical exception, including its exact URL and checksum.

Zig recipes must depend on an exact catalog compiler such as `zig@0.16.0`.
Use `args` for project `-D` options; Rootbeer controls the install prefix and
isolates build caches. Installed commands belong under `bin/`; runtime resources
must continue working after the installation is moved. Checks run through profile
symlinks; offline reconstruction must recover the same commands and output tree.

Use `patches` for reviewed unified diffs applied with `-p1` before compilation.
Patch contents are part of the recipe and build receipt. Keep changes narrowly
focused, such as pinning a git-derived version or replacing a fixed installation
path with executable-relative lookup. Pin unreleased source archives to a full
commit and use an exact snapshot version, retaining the original source checksum.

### Library dependencies

Declare static archives in `build.libraries`, for example
`libraries = { "lib/libssl.a", "lib/libcrypto.a" }`. Library-only packages use
`bins = {}` and `checks = {}`; their build must still run the upstream test suite.
Rootbeer validates declared archives before export and after offline reconstruction.
Archives must be self-contained `.a` files under `lib/` or `lib64/`.

Add exact packages to `build.dependencies`. Rootbeer makes the complete transitive
dependency set available during the build: commands on `PATH`, headers under
`include/`, declared archives under `lib/` and `lib64/`, and package metadata from
`lib/pkgconfig`, `lib64/pkgconfig`, and `share/pkgconfig`. Conflicting exports fail
the build. Dependency store entries remain unchanged.

The builder supplies `CPPFLAGS`, `LDFLAGS`, `PKG_CONFIG_LIBDIR`, and
`PKG_CONFIG_SYSROOT_DIR` for the merged dependency prefix. Declare a package
providing `pkg-config` or `pkgconf` when the project needs that tool. Configure
flags can use `{dependencies}`, such as `--with-openssl={dependencies}`.
Metadata should use the installation prefix `/`; `pkg-config` applies the build
sysroot to header and library flags.

Static dependencies become part of the consuming binary. Shared-library runtime
dependencies are not resolved by this mechanism. Recipes must select static
dependencies explicitly and verify the installed program after relocation. The
host C toolchain and operating-system libraries remain build inputs; this is not
a complete compiler sysroot.

### Command build phases

Use `backend = "custom"` when a project's build does not fit a preset. Each
phase contains argument arrays, executed in order from the unpacked source root:

```lua
steps = {
    configure = {
        { "perl", "./Configure", "--prefix=/", "--libdir=lib",
          "--openssldir=/etc/ssl", "no-shared", "no-module" },
    },
    build = { { "make", "-j{jobs}" } },
    check = { { "make", "test" } },
    install = { { "make", "DESTDIR={prefix}", "install_sw" } },
}
```

`{prefix}` is the isolated staging directory, `{dependencies}` is the dependency
prefix, and `{jobs}` is the compiler parallelism limit. Build, check, and install
phases must contain commands; configure may be empty. Arguments are passed
directly, without shell evaluation. For shell syntax, explicitly invoke `sh` and
pass paths as positional arguments. Rootbeer applies the same environment,
logging, time limits, and artifact verification as the presets. Recipe commands
and upstream build scripts execute trusted code.

### Expansion and compatibility

Compact files expand into the same exact recipes used by resolution, caches,
qualification, and signed snapshots. Existing expanded Lua definitions still load.
`rootbeer-forge index` shows the expanded result. Discovery preserves existing templates
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
rootbeer-forge --catalog packages import github:owner/tool \
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

Discovery considers all three supported platforms by default. Repeat `--system` to
request a smaller set. Ambiguous assets stop the import; select a reusable pattern
with `--asset 'x86_64-linux=tool-{tag}-x86_64-unknown-linux-musl.tar.gz'`.
Patterns support `{tag}` and `{version}`. Existing recipes seed asset rules when
available, and newly discovered asset names become saved patterns.

The importer orders stable dotted numeric versions, including calendar versions;
it skips drafts, prereleases, and tags outside that numeric format, including
unmarked release candidates and moving tags such as `stable`. Tags may have an
optional `v`. Use `--tag-prefix` to restrict and strip a prefix. Discovery fails
when no matching stable releases remain or numeric versions are ambiguous. Discovery reads paginated release history, up to `--max-pages` (20 by
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
rootbeer-forge --catalog packages import --packages selected-packages --output candidates
rootbeer-forge --catalog candidates/packages check
rootbeer-forge --catalog candidates/packages export \
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
can override the discovery filter. `v{version}` selects only tags beginning with
`v`; `{version}` selects bare numeric tags. `source.update_systems` narrows discovery without removing retained
platform recipes. Set `source.track = false` to opt out. Source builds remain
untracked; automatic discovery currently supports GitHub binary releases only.

Run discovery directly against the package directory:

```sh
rootbeer-forge --catalog packages updates \
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
candidates as workflow artifacts, and qualifies changed packages on all three
platforms using verified-result caches. Discovery errors remain visible while
successful candidates can still be checked. It has read-only repository permissions
and does not publish, open pull requests, or advance defaults automatically.
Review qualified candidates before copying them into the index's `packages/` directory.

## Qualify a change

```sh
rootbeer-forge --catalog packages check
rootbeer-forge --catalog packages export --registry tale/rootbeer-index --output result
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
rootbeer-forge --catalog packages export --registry tale/rootbeer-index --output result \
  --cache /tmp/rootbeer-package-results --cache-context "$BUILD_ENVIRONMENT_ID"
```

Use a trusted cache and identify the OS image and tools in `BUILD_ENVIRONMENT_ID`.
A missing entry runs full verification; corruption fails. `--recheck` bypasses
reuse. CI keeps scheduled full checks to detect upstream and platform drift.

## Qualify packages in parallel

Each platform job builds its engine, then immediately exports the catalog with a
bounded worker queue:

```sh
rootbeer-forge --catalog packages export --registry tale/rootbeer-index \
  --output result --workers 2 --jobs 2
```

`--workers` limits concurrent package exports; `--jobs` limits compiler jobs within
each source build. The index runs one job for each supported platform, with its
own cache scoped to the engine and build environment. Successful results are saved
even when another package fails, so a retry reuses verified work.

Assembly combines the three platform bundles and requires every declared version
and platform before publication. Pin `ROOTBEER_REV` to an engine commit supporting
these flags before deploying the workflow.

## Publish independently

`rootbeer-forge assemble` merges platform bundles and requires complete coverage.
`rootbeer-forge publish` uploads source-built archives to GHCR, verifies anonymous
access, and signs an immutable index snapshot. Imported binaries retain their
upstream URLs. CI retains snapshots and receipts, then deploys the latest signed
manifest through GitHub Pages.

Source-build jobs receive no publication credentials. The separate publisher uses
ORAS for uploads; users need neither ORAS nor a container runtime to install.
Package publication does not create Rootbeer GitHub releases.

Read [index hosting and trust](/contributing/package-hosting) for deployment
boundaries and endpoint changes.
