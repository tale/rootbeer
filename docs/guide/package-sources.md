# Catalogs and backends

## The official catalog

Released Rootbeer builds use an official signed index for canonical package
names. The index is published independently of the CLI: adding a recipe does not
require a new Rootbeer release unless it needs a new engine capability.

When resolution is needed, Rootbeer verifies the index manifest's signature and
the immutable snapshot's hash before using it. The public verification key is
embedded in the release; a runtime environment variable cannot replace it.
A matching lock skips catalog fetching.

If the network is unavailable, Rootbeer can use its last verified cached snapshot,
then the smaller embedded catalog if no cached snapshot exists. It reports that
fallback. Invalid signatures, corrupt snapshots, and rollback attempts fail.
A missing package in the selected catalog does not trigger another provider.

`rb apply --update` requires a successful refresh. Offline mode uses the lock and
local artifacts, without fetching an index. Developer builds without an official
endpoint report embedded fallback instead of claiming to use the hosted catalog.

[Package search](/packages/) shows the published catalog. `rb package list` and
`rb package show` inspect the embedded authoring catalog, or the directory passed
to `--catalog`; they are not remote catalog search commands.

## Explicit backends

Canonical names are the normal interface. To select a source directly, use an
explicit request in `rb.package()`:

- `github:owner/repository@tag` selects an upstream GitHub release.
- `aqua:owner/repository@version` uses an Aqua registry recipe.

These are request formats, not additional programs you must install. Rootbeer
resolves and installs them natively. Prefer direct upstream releases when suitable
assets exist. Aqua remains available for registry recipes that supply useful
platform and installation metadata.

GitHub tags are exact, including any leading `v`. Ambiguous asset selection fails;
the `asset` and `bins` options can select an exact filename and archive paths. See
[the generated package API](/reference/core) for the signatures.

Supported inputs include tar.gz, tar.xz, ZIP, and raw executables. Backend-specific
limitations still apply. Direct backend downloads receive content hashes, but do
not gain the official index publisher's signature. Aqua signature and attestation
verification is not implemented.

## Use another index

`rb.package_index()` selects an explicit snapshot URL and SHA-256 before package
declarations. Obtain that pin through a trusted channel: it identifies exact bytes
but is not a publisher signature. HTTPS snapshots and absolute local `file://`
snapshots are supported; published artifact URLs use HTTPS or immutable GHCR blobs.

An explicit pin bypasses the official catalog and its fallback policy. Updating
packages keeps that pin unchanged. Changing or removing the declaration makes the
lock stale. Matching offline locks need cached package contents, not an index fetch.

## Scope

Rootbeer manages command-line package contents and their profile. It does not yet
provide general runtime dependency closure handling, hermetic source builds,
desktop application installers, or service management. Keep other package-manager
declarations where those capabilities are required. Declaring a package here does
not uninstall another manager's copy.
