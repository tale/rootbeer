# Index hosting and trust

## Two independent deployments

| Site                            | Owner               | Published content                                               |
| ------------------------------- | ------------------- | --------------------------------------------------------------- |
| `rootbeer.tale.me`              | Rootbeer repository | Documentation, package search, installer, and nightly binaries. |
| `tale.github.io/rootbeer-index` | Index repository    | Signed latest manifest, immutable snapshots, and receipts.      |
| GHCR                            | Index publisher     | Source-built package archives addressed by digest.              |

Package search fetches the live signed index in the browser. Publishing new
packages therefore updates search without rebuilding the documentation site.
The index endpoint permits cross-origin reads. Search has loading, empty, and
failure states; it does not substitute a hard-coded package list.

Keep these deployments separate. Copying the catalog into the documentation build
would couple package availability to website deployments and make search stale.
GitHub Pages does not route two independent repositories under arbitrary paths of
one custom host.

## A custom index domain

A dedicated subdomain can be assigned to the index Pages site while documentation
keeps its existing custom domain. For example, `index.rootbeer.tale.me` would need
a DNS CNAME to `tale.github.io`, the index repository's Pages domain setting, and
HTTPS provisioning. This is a proposed address, not the current endpoint.

An `api.tale.me` redirect or reverse proxy is another option, but it adds a routing
service. Any migration must preserve the immutable URLs already recorded in locks
and signed manifests. Do not change DNS or the publication base URL as a cosmetic
rename without checking old snapshot and receipt paths.

See [GitHub's custom-domain guide](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site).

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
