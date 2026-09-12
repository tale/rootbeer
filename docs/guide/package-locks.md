# Updates and offline use

Your Lua configuration describes what you want. `rootbeer.lock` records the exact
package selections and verified contents used to realize it.

## Choose when to update

| Command              | Behavior                                                                                             |
| -------------------- | ---------------------------------------------------------------------------------------------------- |
| `rb apply`           | Reuse a matching lock; resolve when declarations or platform change.                                 |
| `rb apply --locked`  | Require a matching lock and refuse to rewrite it. Missing artifacts may be downloaded.               |
| `rb apply --update`  | Refresh resolver inputs and package selections, then rewrite the lock.                               |
| `rb apply --offline` | Require a matching lock and locally available package contents. Never fetch packages or the catalog. |

Review and commit lockfile changes after an update. Version pins in your Lua
configuration remain exact during updates. An explicit index pin also stays fixed
until you edit that declaration.

`rb update` updates Rootbeer itself. `rb apply --update` updates the package
selections in your configuration.

## What gets recorded

The lock contains the target platform, canonical identity, version, source URL,
archive hash, exported commands, and installed-tree hash. Catalog resolutions also
record the selected snapshot and recipe revision. Explicit backend resolutions
record the metadata used to make their decision.

Hashes keep later installs tied to the selected bytes. They do not guarantee
that an upstream download will remain available forever. A cached artifact can
still be used offline after its original URL disappears.

## Offline operation

An offline apply can reuse an existing store output or recreate it from the
download cache. A lockfile alone is not enough: the required package bytes must
already be present. If anything is missing, the operation fails instead of
contacting the network.

The profile lives under `$XDG_STATE_HOME/rootbeer/profiles`, and package state
normally lives under `~/.local/state/rootbeer`. Source your generated package
environment rather than hard-coding store paths.

## Moving between machines

Locks are tied to a platform. Keep separate checkouts for machines with different
platforms when preserving both locks matters. Profiles can vary your declarations,
but they do not make one resolved binary portable across architectures.

Upgrading Rootbeer does not itself require replacing a matching package lock.
New catalog features can require a newer `rb` when resolving new declarations.
