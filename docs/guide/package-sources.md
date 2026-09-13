# Other package sources

Use [package search](/packages/) for tools available directly by name. If a tool
is missing, you can install a release from GitHub or use an Aqua recipe.

## Install from GitHub

Pass the repository and release tag to `rb.package()`:

```lua
local rb = require("rootbeer")

rb.package("github:BurntSushi/ripgrep@15.2.0")
```

The tag must match exactly, including a leading `v` if the project uses one.
Rootbeer downloads a release for your platform. If several files match, use the
`asset` option to choose the filename and `bins` to specify commands inside it;
see the [package API](/reference/core#rootbeer-package).

Rootbeer supports tar.gz, tar.xz, ZIP, and standalone executables. Not every
project's release layout is supported.

## Install from Aqua

Use `aqua:owner/repository@version` to select a recipe from the Aqua registry.
You do not need to install Aqua separately.

Direct GitHub and Aqua downloads are hashed and saved in your lockfile, but are
not signed by the Rootbeer index publisher. Aqua signatures and attestations
are not verified.

## Use another index

For a private or custom package collection, call `rb.package_index()` before
adding packages. Supply the snapshot URL and SHA-256 supplied by its publisher.
See the [API reference](/reference/core#rootbeer-package-index) for the fields.

Use a source you trust: the checksum identifies the download but does not verify
who published it. HTTPS URLs and absolute local `file://` URLs are supported.

Rootbeer uses that collection instead of its official catalog.
`rb apply --update` keeps your selected snapshot; change the URL and checksum to
use a different one. Changing or removing this setting updates the lock on your
next apply. Offline installs still require a matching lock and cached packages.

For verification and fallback details, see
[index hosting and trust](/contributing/package-hosting).
