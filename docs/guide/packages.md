# Packages

Declare the command-line tools you want alongside the rest of your system
configuration. Rootbeer installs them into its own store, exposes their commands
through a package profile, and records exact selections in `rootbeer.lock`.

[Browse the package catalog](/packages/) to find canonical names, available
versions, and platform support. Copy a declaration from a package's result into
your Lua configuration.

## Declare your tools

`rb.package("name")` selects the catalog default for the current platform.
`rb.package("name@version")` requests an exact catalog version. Names stay the
same across macOS and Linux; recipes determine how each platform's package is
obtained.

Unversioned requests select the newest catalog release available for that
platform. A platform that upstream no longer supports can retain an older
default without holding back other machines. Explicit versions never silently
fall back, and an existing matching lock stays unchanged until you update it.

## Make commands available

Add Rootbeer's package environment to your shell configuration:

```lua
local rb = require("rootbeer")
local zsh = require("rootbeer.zsh")

zsh.config({
    sources = { rb.env_export("sh") },
})
```

This places the package profile on `PATH`. Other shell integrations can source
the path returned by `rb.env_export("sh")` as well. Open a new shell after applying
changes to shell startup files.

## Apply and lock

```sh
rb apply
```

Rootbeer resolves declarations, verifies downloaded artifacts, installs them into
its content-addressed store, and creates or updates `rootbeer.lock` beside your
configuration. Commit the lock with your config. A normal apply reuses a matching
lock rather than asking upstream for newer versions.

Published catalog packages install from prebuilt archives, including packages
built from source by the index workflow. A normal apply does not compile those
packages locally. Their runtime files stay together in the store.

## Keep configuration portable

The catalog targets macOS and Linux on ARM64 and x86-64. Availability is tracked
per package version and platform; support for one platform does not imply support
for every OS release or Linux distribution. Use the catalog's platform filter
before adding a package to a shared configuration.

Locks are platform-specific. A lock created for another platform is stale: normal
apply can resolve for the new platform, while `--locked` refuses the change. Use
separate configuration checkouts when you need to retain each platform's lock.

A missing package or platform produces an error. Rootbeer does not silently switch
providers, compile a replacement, or install an older version for an exact request.

## Continue

- [Updates and offline use](/guide/package-locks): control when selections change.
- [Catalogs and backends](/guide/package-sources): understand trust and overrides.
- [Contribute packages](/contributing/packaging): grow the catalog without changing `rb`.
