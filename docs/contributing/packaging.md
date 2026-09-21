# Contribute packages

Package definitions live in the [official index repository](https://github.com/tale/rootbeer-index).
Rootbeer owns the package engine; the index owns recipes and publication. A new
recipe normally needs no Rust changes or Rootbeer release.

Package discovery, build checks, and publication CI run in the index repository,
which pins Forge with its `engine-revision` file. Engine CI runs regression tests.

The engine ships no recipes. Pass `--catalog` with the recipe directory from an
index checkout for commands that inspect or build packages.

## Packaging entrypoint

`rootbeer-forge` owns recipe inspection, builds, updates, qualification, and
publication. `rb` owns configuration and user package environments.

```sh
cargo build --bin rootbeer-forge
rootbeer-forge --catalog ../rootbeer-index/packages plan xz
rootbeer-forge --catalog ../rootbeer-index/packages build xz --output /tmp/xz-build \
  --cache /tmp/rootbeer-builds --cache-context "$BUILD_ENVIRONMENT_ID"
```

`plan` prints direct dependencies, transitive inputs, and execution order without
fetching or building anything. Build execution resolves binary inputs up front,
then uses those locked facts throughout the run.

The build cache reuses individual dependencies across different root packages.
Its key includes compilation inputs and revision, verified dependency outputs and
selected library exports, platform, build engine, environment variables, and tool/input
hashes. The caller-supplied host identity also remains part of the key.
Checks, job allocation, archive paths, unrelated catalog entries, and publication destinations do not
change build keys. Receipts retain the key, environment lock, and host identity.

### Release one package

For a dependency-free source build from a trusted producer:

```sh
rootbeer-forge release --recipe ../rootbeer-index/packages/shfmt.lua \
  --receipt /tmp/shfmt-build/receipt.json --registry tale/rootbeer-index/shfmt \
  --output /tmp/shfmt-release --key "$SIGNING_KEY_FILE" --public-key "$PUBLISHER_PUBLIC_KEY"
rootbeer-forge push /tmp/shfmt-release --public-key "$PUBLISHER_PUBLIC_KEY"
```

`release` checks the recipe, archive, installed contents, and runtime audit before
signing. It produces `package.json`, `package.tar.gz`, and the original `receipt.json`.
It reads only the selected recipe and does not rebuild or execute the package.
The signature approves that build; it does not independently authenticate the
receipt's producer. CI must establish producer trust before giving it to the signer.

`push` requires ORAS authenticated to GHCR with package write access. It uploads
the three files together, checks anonymous downloads, and prints the immutable
record reference. Retry the same command after upload failures; no signing key or
rebuild is needed. This does not update catalog discovery.

For independent CI jobs, `package-plan` emits one task per exact package supported
on the current machine:

```sh
rootbeer-forge --catalog ../rootbeer-index/packages package-plan \
  shfmt@3.14.1 --context "$BUILD_CONTEXT"
```

Each task has an input key covering its recipe (including checks), platform,
relevant build engine, and actual tool/environment hashes. Pass that key to
`build --input-key` with `--cache` and `--cache-context`, then to
`release --input-key`. Changed inputs stop the job rather than publishing a result
under the wrong key. The planner currently supports host builds without package
dependencies; requests must use canonical `name@version` identities.

After verifying public downloads, `push` adds an `inputs-<key>` OCI tag for lookup.
Tags are only locators: `verify-record --package ... --system ... --input-key ...
--public-key ...` checks the immutable record's signature and exact inputs before
reuse. Package planning and verification do not require GitHub Actions.

### Go source builds

Use `backend = "go"` with explicit binary entry points:

```lua
build = {
    backend = "go",
    go = {
        binaries = { tool = "./cmd/tool" },
        variables = { ["main.version"] = "v{version}" },
        tags = { "netgo" },
    },
},
```

`binaries` must match `outputs.bins`. Linker `variables` accept `{version}` and
`{tag}`; discovery expands them for each release. CGO is disabled unless `cgo = true`.
The source archive must contain `go.mod` and `go.sum`. Fetch vendors dependencies
and verifies their checksums, rejecting changes to either module file. Build and
`go vet` use that vendor tree with module downloads disabled, followed by the
recipe's executable checks. `generate` selects local packages whose `go generate` directives run offline
before compilation; generator dependencies must already be vendored or declared
build dependencies. `experiments` explicitly selects `GOEXPERIMENT` values.
Projects requiring frontend assets must provide them in the verified source or
use an explicit build recipe.

Host builds hash the installed Go compiler and its complete toolchain directory.
Explicit environments must declare `tools.go` and `inputs.go-toolchain` pointing
to that compiler's GOROOT. Rootbeer disables automatic toolchain downloads,
workspace discovery, and user Go configuration. The toolchain and source hashes
are retained in the normal build receipt.

### Pinning a build environment

Write an environment specification with absolute executable paths in `tools`,
SDK/sysroot and compiler resource directories in `inputs`, and explicit values
in `variables`. For example, this specification uses a toolchain installed under
`/opt/toolchain`:

```json
{
  "tools": {
    "sh": "/opt/toolchain/bin/sh",
    "cc": "/opt/toolchain/bin/cc",
    "c++": "/opt/toolchain/bin/c++",
    "make": "/opt/toolchain/bin/make",
    "patch": "/opt/toolchain/bin/patch"
  },
  "inputs": {
    "toolchain": "/opt/toolchain",
    "sysroot": "/opt/sysroot"
  },
  "variables": {
    "CFLAGS": "--sysroot=/opt/sysroot -O2",
    "LDFLAGS": "--sysroot=/opt/sysroot"
  }
}
```

List other utilities required by the recipe, such as `ar`, `ranlib`, `sed`,
`mkdir`, and `cp`, in `tools`. Declare script interpreters at their exact paths
(for example `/bin/sh` for a `#!/bin/sh` script). On macOS, declare the actual compiler and its
resource directory, include the SDK directory in `inputs`, and set `SDKROOT`
in `variables`. Hashing an SDK directory can take time; it reads the full tree.
Directory symlinks must resolve within their declared input root. Use
`xcrun --find make` and `xcrun --find libtool` to locate the actual macOS tools;
`/usr/bin` may contain developer-directory launchers.

```sh
rootbeer-forge pin-environment environment.json > environment.lock.json
rootbeer-forge --catalog ../rootbeer-index/packages build xz --output /tmp/xz-build \
  --environment environment.lock.json \
  --cache /tmp/rootbeer-builds --cache-context "$BUILD_ENVIRONMENT_ID"
```

`build` and `export` accept `--environment`. A lock pins tool bytes, input tree
contents, platform, paths, and variables. The executor verifies it before cache
lookup and again after each package's checks. Changed inputs fail the build;
review the change and regenerate the lock to get new cache keys.

Pinned execution clears inherited variables and puts only dependency tools and
declared tools on `PATH`. Rootbeer owns `PATH`, `CC`, `CXX`, shell selection,
temporary directories, locale, and package-config lookup. Dependency library
paths are prepended to `CPATH` and `LIBRARY_PATH`; your `CPPFLAGS` and
`LDFLAGS` remain unchanged. Path lists preserve spaces without shell quoting.

Environment locks describe input selection. Add `--isolate` to enforce filesystem
and network restrictions while package code runs:

```sh
rootbeer-forge --catalog ../rootbeer-index/packages build xz --output /tmp/xz-build \
  --environment environment.lock.json --isolate \
  --cache /tmp/rootbeer-builds --cache-context "$BUILD_ENVIRONMENT_ID"
```

`export` accepts the same flag. Isolation requires a lock and never falls back
to host execution. Rootbeer downloads and extracts sources before starting
package commands. Tool probes, patches, build phases, and package checks all run
inside the sandbox; cache hits rerun their checks there too.

Declared tools, SDK inputs, and realized dependencies are read-only. Each build
or check gets a separate writable scratch directory. Undeclared file contents
and host network access are denied, including loopback. The executor opens log
files before launching commands, so logs remain available on failure. Successful
isolated commands also trigger cleanup of their remaining process group.

On macOS, Rootbeer uses `/usr/bin/sandbox-exec`. Select actual developer tools
with `xcrun --find clang`, `xcrun --find make`, and similar commands when writing
the environment specification: `/usr/bin/cc` and `/usr/bin/make` may be discovery
shims that need access to undeclared developer directories.

On Linux, install Bubblewrap at `/usr/bin/bwrap`. The host must permit user,
mount, process, and network namespaces. Restricted containers can prevent this;
Rootbeer reports a sandbox startup failure instead of reducing isolation.
On Ubuntu 24.04, install `apparmor-profiles` and load the supplied
`/usr/share/apparmor/extra-profiles/bwrap-userns-restrict` profile into AppArmor.
It permits Bubblewrap to set up namespaces while denying capabilities to its
children. The engine CI installs this profile and checks sandbox startup before
running tests.
The Linux launcher creates a private filesystem with read-only input mounts,
a writable scratch mount, and private process and network namespaces.
Compiler helpers must also be declared: GCC reports its `cc1` executable with
`cc -print-prog-name=cc1`.

The OS runtime remains a declared exception: macOS loader/framework locations
and Linux library/loader locations are readable. macOS also permits filesystem
metadata queries. These runtime inputs still need a stable `BUILD_ENVIRONMENT_ID`;
this mode does not provide a fully pinned OS image. Input locks are verified
before and after execution, but other host processes can still change files.

Cache keys distinguish host and isolated execution and include the launcher
identity and policy implementation. Receipts record the selected isolation
identity. Completed export qualifications can be reused in the same isolated
environment; `--recheck` executes the checks again.

Without a lock, trusted host builds remain available. Rootbeer hashes the
selected host compiler and basic tools before lookup, but the host identity
must still account for SDKs, libraries, and other utilities. `--recheck` rebuilds
each node; `--phase-timeout SECONDS` sets the build command time limit.

Successful build outputs enter the cache only after input verification and checks;
build-cache hits verify archive and output hashes and rerun package checks.
Export's `--cache` also retains completed qualifications for both source and
binary packages. A matching qualification reuses the original receipt and archives
without entering the executor. The dependency build cache lives under `builds/`.

Build environments set `SOURCE_DATE_EPOCH=1` and `ZERO_AR_DATE=1`; the latter
prevents Apple archive-member timestamps from changing otherwise identical outputs.
Failed command phases retain their scratch directory, including upstream diagnostic
files such as `config.log`. The error reports its path; remove the failed output
directory when it is no longer needed.

### Auditing native outputs

```sh
rootbeer-forge audit /path/to/installed/package
rootbeer-forge audit /path/to/store/package --receipt /path/to/receipt.json
```

The command emits a JSON report and exits unsuccessfully when native loader
references cannot be justified. It parses ELF and Mach-O metadata, including
all slices of universal Mach-O files, without executing package code.

Source builds run this audit before packaging and again on realized outputs,
including cache hits. Failed audits cannot publish build-cache results. Bundling
source receipts reruns the audit too. `runtime-audit.json` contains relative
file paths, architectures, library identities, interpreters, search paths, and
violations; receipts record `runtime_audit_sha256`.

Libraries must resolve inside the package or its pinned runtime closure through loader-relative paths:
ELF `$ORIGIN`/`${ORIGIN}` or Mach-O `@loader_path`, `@executable_path`, and
`@rpath`. Resolution checks architecture, word size, and byte order. Mach-O
inherited rpaths and ELF RPATH/RUNPATH inheritance are followed along dependency
chains. Declared runtime references may reach sibling content-addressed store
entries; other escaping targets fail. Symlinks must stay inside their owning
package. Absolute
library identities, working-directory searches, undeclared host prefixes, and
missing bundled libraries fail even if the current machine could load them.

The OS-runtime baseline remains explicit: macOS libraries under `/usr/lib` and
`/System/Library/Frameworks`, and Linux libc, libm, libdl, libpthread, librt,
libgcc_s, libstdc++, and the supported glibc/musl loaders. This baseline is tied
to the host-image identity; the audit does not establish symbol-version or OS
version compatibility. Standard system search directories are permitted, but
arbitrary library names in them are not automatically approved on Linux.

Scripts, static archives, relocatable object files, and runtime `dlopen` calls
are outside this check. ELF audit/filter/configuration loading and Mach-O
embedded loader environment settings are rejected. Weak dependencies are
required to resolve too. A shared library using `@executable_path` without a
known executable context cannot be qualified by this pass.

With `--receipt`, the audit verifies installed output hashes and reads runtime
entries beside the installed package. It does not fetch missing inputs. Without
a receipt, only the package itself and the OS baseline are allowed.

### Dependency roles

Source-capable dependencies use Rootbeer's source recipes, even when upstream
binaries are available. Their verified build outputs are reused from the local
build cache; a cache miss compiles the recipe and its required dependencies.
Binary-only dependencies use their pinned upstream artifacts. A failed source
build never falls back to an upstream binary.

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
as `kind = "all"`. These existing roles do not imply runtime installation.
`runtime` installs a dependency without exposing build commands or headers.
`link_runtime` exposes link inputs and installs the dependency. Runtime edges
propagate only through other runtime edges; a compiler's own runtime dependencies
remain build inputs for its consumer.

Runtime closures require lock schema 3, build receipt schema 2, and artifact index
schema 6. The authoring schema described below is separate from lock and artifact schemas.

`custom` names explicit build phases in both recipes and serialized catalogs.
The old `commands` spelling is rejected.

## Definition API

Package files use `schema = 2`. Four sections have separate responsibilities:

- `upstream` optionally describes release discovery.
- `inputs` describes prebuilt artifacts or source archives.
- `build` describes source compilation, when required.
- `outputs` describes exported commands, applications, libraries, and checks.

Prebuilt-only packages need no source code, build backend, or discovery rules.
Shared settings apply to every version; version entries supply their own hashes
and explicit exceptions.

```lua
return {
    schema = 2,
    name = "tool",
    description = "A command-line tool",
    homepage = "https://github.com/owner/tool",
    default_version = "2.0.0",
    systems = { "aarch64-macos", "aarch64-linux", "x86_64-linux" },
    upstream = { github = "owner/tool", tag_prefix = "v" },
    inputs = {
        prebuilt = {
            github = "owner/tool",
            tag = "v{version}",
            assets = {
                ["aarch64-linux"] = "tool-{tag}-linux-arm64.tar.gz",
                ["x86_64-linux"] = "tool-{tag}-linux-amd64.tar.gz",
                ["aarch64-macos"] = "tool-{tag}-darwin-arm64.tar.gz",
            },
        },
    },
    outputs = {
        bins = { "tool" },
        checks = { { "tool", "--version" } },
    },
    versions = {
        ["1.0.0"] = { revision = 2 },
        ["2.0.0"] = {},
    },
}
```

The filename matches `name`; aliases, homepage, supported systems, and default
versions are explicit. `upstream.github` selects where to discover releases;
`inputs.prebuilt.github` selects where to fetch binaries. Removing `upstream`
disables discovery without changing installation inputs.

Prebuilt asset patterns accept `{version}` and `{tag}`; the tag template accepts
`{version}`. Aqua-backed prebuilt inputs use `aqua = "owner/project"` and an exact
tag instead of GitHub asset patterns. Expansion happens before catalog validation.
Command arguments and configure flags are literal. Unsupported placeholders and
unknown fields fail validation.

Version `inputs.prebuilt.assets` overrides individual platforms. Version `outputs`
fields and `systems` replace their shared counterparts, including explicit empty
arrays or maps. `inputs.prebuilt.tag` selects an exceptional release tag. Revisions
default to 1. Discovery uses shared outputs for new releases and preserves retained
versions' exceptions, including exceptions on the current default.

Choose the newest release for each platform. `default_version` stays explicit;
`default_versions` selects older defaults for discontinued targets. Retain older
versions and their exceptions. Explicit version requests never fall back.

### Archive commands and moving releases

Use `outputs.bin_paths` to map each declared command to its path inside a GitHub archive.
For example, bobrwm maps `bobrwm` to `Bobrwm.app/Contents/MacOS/bobrwm-cli`.
Mappings must cover every declared command and stay inside the archive. The full
archive tree is retained, including resources and app bundles.

Declare macOS app exports explicitly:

```lua
outputs = { apps = { ["Bobrwm.app"] = "Bobrwm.app" } },
```

Keys are exported `.app` filenames; values are relative paths to contained `.app`
directories. App recipes must target macOS only. Versions inherit `apps` unless
an explicit version map overrides it. `rb use` and Lua package application manage
links in `~/Applications`; existing apps and unmanaged links cause a conflict.
Temporary `rb run --app` launches do not create these links. Neither operation
configures login items or grants permissions.

DMG release assets use the same `outputs.apps` and `outputs.bin_paths` fields.
Preparation mounts the verified image read-only on macOS, copies only the declared
bundles at their relative paths, and detaches the image even when copying fails.
Finder backgrounds and the image's `/Applications` shortcut are not included.
Internal relative bundle symlinks are preserved; links outside a bundle are rejected.
The published artifact is a normal archive, so installation needs no disk-image mount.
PKG installers and installer scripts are unsupported.

For a locked DMG in a Lua configuration, use `install = { dmg = true }` and declare
its `apps` and `bins` paths. DMG installation does not use `strip_prefix`.

A moving tag such as `tip` is not a package version. Give each approved snapshot
an exact version, select its exact asset name, and pin every platform's verified
SHA-256 in that version's `inputs.prebuilt.checksums` map. Set `inputs.prebuilt.mirror = true` to retain the
qualified package in the index's content-addressed registry. Receipts retain the
original upstream URL and checksum. Old installations then remain available even
if upstream replaces or removes the release assets.

Mirrored snapshots require manual checksum qualification; omit `upstream`
instead of treating a moving tag as a stable release. Increment the recipe revision
if another build of the same version changes its bytes. Applications can be opened
from the verified store with `rb run bobrwm --app Bobrwm.app`.

### Source builds

Put the archive URL, archive format, strip prefix, and patches under `inputs.source`.
Use `build` for the backend (`autotools`, `zig`, or `custom`), backend settings, and
exact build dependencies. Autotools accepts `configure` flags. URL and strip prefix
accept `{version}`; `{tag}` additionally requires an explicit `upstream.tag_prefix`.

Each version supplies its own verified `inputs.source.sha256`; hashes cannot be
shared or inferred from version names. Version `inputs.source` fields override
individual download settings. A version's `build` replaces the complete backend
configuration, independently of its source hash and outputs.

Zig recipes must depend on an exact catalog compiler such as `zig@0.16.0`.
Use `args` for project `-D` options; Rootbeer controls the install prefix and
isolates build caches. Installed commands belong under `bin/`; runtime resources
must continue working after the installation is moved. Checks run through profile
symlinks; offline reconstruction must recover the same commands and output tree.

Use `inputs.source.patches` for reviewed unified diffs applied with `-p1` before compilation.
Patch contents are part of the recipe and build receipt. Keep changes narrowly
focused, such as pinning a git-derived version or replacing a fixed installation
path with executable-relative lookup. Pin unreleased source archives to a full
commit and use an exact snapshot version, retaining the original source checksum.

### Library dependencies

Declare source-built static or shared libraries in `outputs.libraries`, for example
`libraries = { "lib/libssl.a", "lib/libcrypto.a" }`. Library-only packages use
`outputs.bins = {}` and `outputs.checks = {}`; their build must still run the upstream test suite.
Rootbeer validates declared libraries before export and after offline reconstruction.
Use self-contained `.a` archives, `.dylib`, `.so`, or versioned `.so.*` files
under `lib/` or `lib64/`.

Add exact packages to `build.dependencies`. Rootbeer makes the complete transitive
dependency set available during the build: commands on `PATH`, headers under
`include/`, declared libraries under `lib/` and `lib64/`, and package metadata from
`lib/pkgconfig`, `lib64/pkgconfig`, and `share/pkgconfig`. Conflicting exports fail
the build. Dependency store entries remain unchanged.

The builder supplies `CPATH`, `LIBRARY_PATH`, `PKG_CONFIG_LIBDIR`, and
`PKG_CONFIG_SYSROOT_DIR` for the merged dependency prefix. Declare a package
providing `pkg-config` or `pkgconf` when the project needs that tool. Configure
flags can use `{dependencies}`, such as `--with-openssl={dependencies}`.
Metadata should use the installation prefix `/`; `pkg-config` applies the build
sysroot to header and library flags.

Static dependencies become part of the consuming binary. Shared libraries need
explicit `link_runtime` edges and loader-relative references to their installed
store entries. Rootbeer does not rewrite binaries or set global loader variables.

### Runtime layout

Runtime packages occupy separate content-addressed store entries. The build
placeholder `{runtime:name@version}` expands to the dependency's pinned directory
name, such as `sha256-<hash>-name-version`. It is available for the runtime closure.
For a binary in `bin/` or a library in `lib/`, a custom ELF linker argument can be:

```lua
"-Wl,-rpath,$ORIGIN/../../{runtime:example-library@1}/lib"
```

On macOS use `@loader_path` in place of `$ORIGIN`, and give shared libraries an
`@rpath/libname.dylib` install name. These are literal command-array arguments;
shell commands and Makefiles need their own dollar-sign escaping. Outputs in
other subdirectories must adjust the number of parent components.

Each package pins its direct runtime packages, including their transitive facts.
Installation verifies and realizes the closure before the requested output,
including on cache hits. Runtime commands are not automatically added to the
user profile. Locks expose the full set of store paths for retention; a garbage
collector has not been implemented yet.

Build output includes `runtime/` archives alongside the root archive and receipt.
Keep them together when moving builds. Generated installation files include the
runtime closure, and bundling verifies and exports every runtime archive. Moving
the whole store preserves relative library references. Host C toolchains and
OS libraries still belong to the pinned build environment; this is not a complete
compiler sysroot or an ABI compatibility check.

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
prefix, and `{jobs}` is the compiler parallelism limit, not an artifact input.
Changing it must not change the intended output. Build, check, and install
phases must contain commands; configure may be empty. Arguments are passed
directly, without shell evaluation. For shell syntax, explicitly invoke `sh` and
pass paths as positional arguments. Rootbeer applies the same environment,
logging, time limits, and artifact verification as the presets. Recipe commands
and upstream build scripts execute trusted code.

### Expansion

Schema 2 definitions expand into the resolved recipes used by builds, caches,
installation, and signed snapshots. `rootbeer-forge index` shows those resolved
inputs. Import, discovery, and rendering all write schema 2. Older compact and
expanded Lua layouts are rejected; there is no authoring compatibility adapter.

Shared changes affect every version that inherits them. Review expanded output,
retain old behavior with version overrides, or increment every affected revision.
Authoring-only changes must leave expanded recipes unchanged to reuse their results.

## Import GitHub projects

Generate candidates without editing the current catalog:

```sh
rootbeer-forge --catalog packages import github:owner/tool \
  --name tool --bin tool --output candidates
```

The output contains one complete schema 2 `packages/tool.lua`, including its
GitHub repository ID and reusable source patterns. The output directory must not
already exist.

Names default to the lowercase repository name. Use `--name` to choose the canonical
identity and repeat `--alias` for alternate names. The importer rejects names and
aliases owned by another package, duplicate upstream projects, and changes to a
recorded repository ID or location. Pass the complete index with `--catalog` so
collision checks include every official package.

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

An optional `upstream` section describes GitHub release discovery, independently of
the build backend. `upstream.tag_prefix` selects a release series;
`upstream.exclude_tags` excludes unrelated tags, and `upstream.systems` narrows
updates without removing retained platform recipes. Omit `upstream` to opt out.
A pinned `upstream.repository_id` detects repository identity changes.

For prebuilt packages, discovery selects platform assets using `inputs.prebuilt`.
For source packages, it selects a stable release, expands the shared source URL,
downloads the archive, and records the hash of the actual bytes. Source discovery
requires a shared build definition and a source URL containing `{version}` or
`{tag}`. It does not look for platform binaries or run builds during discovery.
Existing versions remain pinned; generated candidates still require build checks.

Run discovery directly against the package directory:

```sh
rootbeer-forge --catalog packages updates \
  --cache .upstream-metadata --output candidates
```

For older catalogs without update rules, `seed-upstreams --output tracked-packages`
creates schema 2 copies of GitHub-backed binary packages with inferred asset patterns.
Review their asset patterns and tag filters before adopting them. The output must
be a new directory; seeding does not preserve customized discovery rules.

`updates` reports each project independently. It writes `report.json`, `summary.md`,
and a complete candidate catalog in `packages/` when recipes or discovery rules
change. Keeping the catalog together preserves build dependencies. The report
separates recipe changes (`updated`) from rule changes (`rules_changed`); package
CI runs when recipes change and reuses verified results for unchanged packages. A no-change scan writes no
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

The index checks upstream releases daily and qualifies changed candidates on all
three platforms, reusing verified results for unaffected packages. After successful
assembly, publication verifies the artifact digest and exact catalog, commits the
changed recipes, and publishes those same artifacts without rebuilding. Candidates
whose catalog, engine, or pipeline inputs changed during verification are discarded;
the next scan retries against current inputs. Discovery errors remain in the report
while successful candidates continue independently.

Rootbeer has a separate main-branch update path. Successful engine CI pushes a
`rootbeer-update` request file to the index; a 15-minute scheduled poll is the
fallback. Both resolve the current main commit and require successful `CI` checks,
hash its source archive, and retain existing version recipes. Index qualification
still builds and checks the new package on each supported platform. The package
update does not advance `engine-revision`.

The notification workflow uses `INDEX_UPDATE_SSH_KEY`, a writable deploy key scoped
to the index repository. It only commits the update request; discovery independently
checks the current source revision and CI result. The index also accepts a
`rootbeer-update` repository dispatch. Without the key, polling remains active. The
index's existing `PUBLISH_INDEX` switch controls automatic promotion and publication.
Packages without discovery rules remain listed as untracked in the report.

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

Each result has a versioned `record.json` binding its expanded recipe closure,
engine, environment, and destination to the published artifact and original
receipt digest. Source identities include the effective tools and Rust sysroot
when needed; pinned environment files are verified before lookup. Runtime
dependency archives travel with the result. Job allocation and presentation
metadata do not affect qualification identity.

```sh
rootbeer-forge --catalog packages export --registry tale/rootbeer-index --output result \
  --cache /tmp/rootbeer-package-results --cache-context "$BUILD_ENVIRONMENT_ID"
```

Add `--plan --output plan.json` to inspect reuse without downloading sources or
executing packages. The JSON contains each selected package's qualification input
digest and either `reuse` with the original receipt digest or `qualify` with a
reason: `no_matching_result`, `explicit_recheck`, or `cache_disabled`. A missing
qualification may still reuse compilation through the separate build cache.
Planning verifies local receipt and archive contents; corrupt evidence fails.

Build identities combine shared executor/package/store inputs with the selected
backend implementation. Qualification identities include the backends used by the
entire recipe dependency closure. A Rust-backend edit invalidates Rust results and
dependent qualifications without invalidating independent Autotools, custom, or Zig
results. A dependent compilation can still reuse identical dependency outputs.

Unclassified build files, shared executor code, manifests, the workspace lockfile,
and environment changes invalidate conservatively. Backend dispatch changes are
shared changes. Trailing inline test modules and files imported only through `#[cfg(test)] mod`
are excluded from source fingerprints. Other production source bytes remain exact. Scheduling and execution-budget policy are operational
inputs, separate from the commands and environment that produce package bytes.
CLI/docs changes do not change package identities. The scoped identity introduces
new cache keys; old evidence is retained but never silently relabeled as compatible.

An additive backend integration can retain predecessor qualifications through
`scripts/cache-compatibility`. Each record binds exact current shared source
digests to reviewed predecessor digests; a subsequent source change disables it.
The Go integration permits only existing non-Go backend closures, with identical
recipes, dependencies, environments, and artifact integrity checks. Reuse preserves
the original qualification, engine identity, and receipt. Newly built artifacts
always use the current identity; explicit rechecks bypass predecessor reuse.

Changing only checks reruns qualification against cached build outputs. Changing
the recipe revision still invalidates compilation. Changed dependency recipes
invalidate dependent qualifications; compilation can reuse dependencies with
identical installed outputs and export contracts. Successful qualifications are
saved per package, even when another package fails, so a retry retains completed
work. `export --timeout SECONDS` bounds execution; SIGINT/SIGTERM request the same
shutdown. Forge stops starting work, terminates its command groups, waits for active
workers, and leaves completed qualifications intact. Retry with the same cache;
only missing qualifications run. CI should set its outer deadline later than
Forge's budget to leave time for cleanup and cache transport. Forced runner loss
can still prevent retention.

Use a trusted cache and identify the OS image and tools in `BUILD_ENVIRONMENT_ID`.
A missing entry runs full verification; corruption fails. `--recheck` bypasses
persistent reuse, while shared dependencies still compile once per export invocation.
CI keeps scheduled full checks to detect upstream and platform drift.

Exports with a qualification cache include content-addressed records in
`qualifications/`. Assembly preserves these records with the original receipts
and archives. A complete bundle can become a portable candidate:

```sh
rootbeer-forge --catalog approved-packages verify-candidate retained/bundle
rootbeer-forge import-results --bundle retained/bundle --cache /tmp/rootbeer-package-results
```

`verify-candidate` requires full qualification coverage and binds each record to
the selected catalog and exact bundled artifact. `import-results` verifies all
candidate content before importing entries. It preserves valid existing entries
and executes no package code. Later export planning decides whether the imported
inputs still match the local engine and environment.

To restore only one platform, first fetch the authenticated `index.json` and all
`qualifications/*.json` records. Forge plans receipt and runtime-archive transfer
without executing package code:

```sh
rootbeer-forge candidate-files retained/bundle --system aarch64-linux
# Fetch exactly the returned paths from the authenticated candidate digest.
rootbeer-forge import-results --bundle retained/bundle --cache /tmp/rootbeer-package-results \
  --system aarch64-linux
```

`candidate-files` defaults to the current platform. It requires complete, consistent
metadata but does not require archive bytes. Platform import validates all metadata
and every selected receipt and archive before writing cache entries; foreign-platform
bytes may be absent. Complete publication still requires `verify-candidate` with all
platform contents. Each import preserves the original catalog and receipt bytes.

These records do not authenticate their producer. Before importing, the caller
must admit the producer and approve its catalog. The index workflow stores exact
candidate bytes in OCI, attests collector admission separately from publication
approval, and imports only candidates with both attestations. GitHub credentials
and workflow policy stay outside Forge. Another host can apply its own admission
policy to the same portable bundle.

Old bundles without qualification records remain usable with `verify-bundle`,
but cannot be admitted as reusable candidates. Repair missing evidence explicitly;
do not silently rebuild the catalog or manufacture qualifications from archives.

## Qualify packages in parallel

Each platform job installs its commit-pinned, attested Forge binary, then exports
the catalog with a bounded worker queue:

```sh
rootbeer-forge --catalog packages export --registry tale/rootbeer-index \
  --output result --workers 2 --jobs 2
```

`--workers` limits concurrent package exports; `--jobs` is the total compiler job
budget shared between them. Dependencies run before their consumers, while
independent source builds can overlap. Shared dependencies reuse verified outputs
within the invocation, including `--recheck` and exports without a persistent cache.
A failed dependency blocks its consumers; independent packages continue.

The index runs one job per supported platform. Cache keys include relevant engine
source, recipes, dependency outputs, and the build environment. Unrelated CLI edits
and changes to job allocation do not invalidate compiled outputs. Successful
results survive failures elsewhere, so retries can reuse verified work.

Assembly combines the three platform bundles and requires every declared version
and platform before publication. The index's `engine-revision` pins the Forge
release used throughout verification and publication.

## Publish independently

`rootbeer-forge assemble` merges platform bundles and requires complete coverage.
`rootbeer-forge publish` uploads source-built archives to GHCR, verifies anonymous
access, and signs an immutable index snapshot. Imported binaries retain their
upstream URLs. CI retains snapshots and receipts, then deploys the latest signed
manifest through GitHub Pages.

Before publication, validate a downloaded bundle against the selected recipes:

```sh
rootbeer-forge --catalog packages verify-bundle bundle
```

This checks catalog equality, publication coverage, and the hashes of referenced
local receipts and GHCR archives without running package commands or downloading
files. Coverage follows the index schema, including source alternatives.
`verify-index` checks index metadata only. The index workflow remains responsible
for trusting the producing run, approving recipes, and verifying the downloaded
transport artifact's digest. GitHub run selection, Git commits, and deployment
belong in index workflows and helpers.

Source-build jobs receive no publication credentials. The separate publisher uses
ORAS for uploads; users need neither ORAS nor a container runtime to install.
Package publication does not create Rootbeer GitHub releases.

Read [index hosting and trust](/contributing/package-hosting) for deployment
boundaries and endpoint changes.

## Source recipes with optional binaries

A recipe may declare both `inputs.source` and `inputs.prebuilt`, using one
`build` section and one output contract. Export builds the source recipe and
publishes Rootbeer’s compiled output, reusing verified build results when available.
Consumers prefer these published artifacts and can explicitly select source.
The signed index binds the recipe revision, artifact hash, and build receipt.
Upstream binaries are used for binary-only recipes, which
still omit source and build sections.

For development builds, add `git = { github = "owner/repo", branch = "main" }`
inside `inputs.source`. The branch is optional and defaults to the repository's
HEAD. Declare this only when the build steps work on Git archives, including any
preparation that release archives normally provide. Source repositories with
required submodules need a recipe that supplies those inputs; GitHub archives do
not include submodule contents.

`inputs.prebuilt.systems` can restrict binaries to a subset of the source recipe's
platforms. A version can use `inputs.prebuilt = { enabled = false }` to retain its
source build without inheriting shared binary rules. New source releases without
matching release assets can be published as source-only versions.

Artifact index schema 7 carries source alternatives and Git origins. Source
recipes can be published without prebuilt artifacts; binary-only recipes still
require artifacts for every declared platform. Coordinate these changes with the
index's engine pin. Export qualifies the source build and its published binary
before adding a source alternative.
