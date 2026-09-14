# Index hosting and trust

## Two independent deployments

| Site                            | Owner               | Published content                                               |
| ------------------------------- | ------------------- | --------------------------------------------------------------- |
| `rootbeer.tale.me`              | Rootbeer repository | Documentation, package search, installer, and nightly binaries. |
| `tale.github.io/rootbeer-index` | Index repository    | Signed latest manifest, immutable snapshots, and receipts.      |
| GHCR                            | Index publisher     | Source-built and mirrored package archives addressed by digest. |

Package search fetches the live signed index in the browser. Publishing new
packages therefore updates search without rebuilding the documentation site.
The index endpoint permits cross-origin reads. Search has loading, empty, and
failure states; it does not substitute a hard-coded package list.

Keep these deployments separate. Copying the catalog into the documentation build
would couple package availability to website deployments and make search stale.
GitHub Pages does not route two independent repositories under arbitrary paths of
one custom host.

## Release trust configuration

Rootbeer release builds embed `ROOTBEER_INDEX_URL` and `ROOTBEER_INDEX_PUBLIC_KEY`.
Set both together. The key is the 32-byte Ed25519 public key encoded as 64 lowercase
hex characters. The website build uses the same public values for catalog search.
The private signing key belongs only in the publishing environment and its secure
backup, never in a repository or a binary.

The manifest contains schema 1, an increasing sequence, a snapshot URL and SHA-256,
and a signature. The signed UTF-8 payload is the compact JSON array:

```text
["rootbeer-index-v1",sequence,"index URL","index SHA-256"]
```

Publish immutable snapshot bytes before advancing the signed manifest. Retain old
snapshots, receipts, and GHCR manifests for existing locks. The CLI rejects rollback
relative to its local verified history; a fresh installation has no earlier
sequence to compare. The manifest currently has no expiry policy.

## Snapshot format versions

Schema 2 snapshots support command-path mappings, pinned mirrors, Zig builds, and
source patches. New clients and package search read schemas 1 and 2. Older clients
cannot parse these additions, even when selecting an unrelated package.

Publish schema 2 through `--manifest latest-v2.json` and configure new CLI/site
builds to use that endpoint. Keep `latest.json` and its schema 1 snapshot available
for older clients; do not advance it to an incompatible snapshot. Those clients
retain their last catalog and need an updated Rootbeer build for newer packages.
Manifest signatures keep the same format; rollback sequences are checked per
endpoint. Both channels share retained, immutable snapshots and receipts.

Publish and verify the new channel before switching the public CLI and website.

## Changing the endpoint

1. Provision and verify the new HTTPS endpoint while retaining old URLs.
2. Verify that snapshots and receipts remain readable, including cross-origin
   browser requests if search uses the new host.
3. Update the index publisher's `INDEX_URL` for future snapshots.
4. Update Rootbeer's public URL/key variables and rebuild the CLI and website.
5. Test a fresh install, an existing lock, and offline replay before retiring any
   routing configuration. Retained artifact URLs must keep working.

The CLI scopes cached index history to its endpoint and verification key. Moving
an endpoint or rotating a key therefore needs an explicit migration plan.

## Catalog selection and fallback

Released builds verify the official manifest signature and snapshot hash before
using package information. The verification key is embedded at build time;
runtime environment variables cannot replace it. A matching lock needs no
catalog fetch.

When the network is unavailable, resolution can use the last verified cached
snapshot, then the smaller embedded catalog. Rootbeer reports this fallback.
Invalid signatures, corrupt snapshots, and rollback attempts fail. A missing
package does not trigger another provider. Developer builds without an official
endpoint report embedded fallback. `rb apply --update` requires a successful
refresh; offline installs use the matching lock and cached package contents.

Package search displays the published catalog. `rb package list` and
`rb package show` inspect the embedded authoring catalog or the directory passed
to `--catalog`; they do not search the remote catalog.

Explicit `rb.package_index()` pins bypass official catalog selection and fallback.
The pin identifies exact bytes, not a publisher signature. Published artifact URLs
use HTTPS or immutable GHCR blobs.

## Package locks

Locks record the platform, package identity, version, source URL, archive hash,
exported commands, and installed-tree hash. Catalog packages also record the
snapshot and recipe revision; direct sources record their resolution metadata.
Hashes bind subsequent installs to the selected contents, but do not ensure a
download URL remains available.

Catalog packages install from prebuilt archives, including source-built packages
published by the index workflow. Normal apply does not compile them locally.
Packages live under the Rootbeer state directory, with command paths under
`$XDG_STATE_HOME/rootbeer/profiles`. Use `rb.env_export()` instead of hard-coding
these paths. New catalog features can require a newer CLI when resolving packages.
