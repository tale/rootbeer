# Package distribution and trust

| Site            | Published content                                                                      |
| --------------- | -------------------------------------------------------------------------------------- |
| `rbpkg.com`     | Documentation, package search, installer, and nightly CLI binaries.                    |
| `pdr.rbpkg.com` | One discovery manifest, independently signed package records, and retained provenance. |
| GHCR            | Immutable package archives addressed by digest.                                        |

The web UI and `rb search` read `https://pdr.rbpkg.com/current.json`. Publishing
packages updates search without rebuilding the website. Installation fetches the
selected package's signed record and archive; it does not download other packages'
records or validate the entire distribution's artifacts.

## Published files

- `current.json`: schema 2 discovery, containing package metadata, recipes, published
  platform record URLs and hashes, an increasing sequence, and a signature.
- `manifests/<sha256>.json`: immutable copies of discovery for pinned resolutions.
- `records/<sha256>.json`: individually signed package records identifying exact
  artifact bytes, installation instructions, and provenance.
- `snapshots/` and `receipts/`: retained approvals and evidence from earlier
  publications, kept readable for existing locks and migrated package records.

The manifest is signed as compact JSON with recursively sorted object keys:

```text
["rootbeer-discovery-v1",sequence,catalog,records]
```

Each package record signs its exact embedded JSON bytes. A record distinguishes
source-build evidence from verified upstream binaries. Migrated records preserve
their earlier signed catalog approval and receipt hash rather than claiming a new
build. The publisher imports only unchanged recipes and dependency recipes from
that approval.

A package job prepares and checks one package/platform, retains the result, then
signs and uploads it to GHCR. `rootbeer-forge publish-records` merges successful
records into discovery without rebuilding or downloading package archives.
Concurrent publications merge against the latest metadata before deploying.
Failed packages do not prevent successful packages from entering discovery;
previous published defaults remain usable until their replacements are ready.

## Release configuration

CLI and website builds embed `ROOTBEER_INDEX_URL` and
`ROOTBEER_INDEX_PUBLIC_KEY`. Set both together. The key is a 32-byte Ed25519 public
key encoded as 64 lowercase hex characters. The private key belongs only in the
publishing environment and its secure backup.

Upgrade the CLI and website before switching `current.json` to schema 2. Current
clients also read the previous schema 1 pointer during migration. Old clients
cannot read schema 2 discovery; use `rb self-update` to upgrade.

Rootbeer checks discovery signatures and rejects rollback relative to its verified
local history. It verifies the selected record's signature, identity, platform,
and recipe, then checks artifact hashes during installation. Fresh clients have
no previous sequence to compare. Discovery has no expiry policy.

Matching locks require no discovery refresh. On network availability errors,
Rootbeer can use verified cached discovery and reports that fallback. Invalid
signatures, corrupt metadata, and rollback attempts fail. `rb apply --update`
requires a successful refresh; offline use requires cached metadata and contents.
A missing published package never triggers an implicit local source build.

Keep immutable manifests, records, receipts, and GHCR blobs available for existing
locks. If moving the endpoint, preserve old URLs and cross-origin browser access,
then update the publisher's `INDEX_URL` and rebuild the CLI and website with the
new URL. Cached trust history is scoped to endpoint and verification key.
