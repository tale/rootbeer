# Secrets

`rb.secret` is the namespace for pluggable secret providers. Each provider
wraps an external CLI (1Password's `op`, in the future `rage`, …) and
exposes the same two-shape API: a **sync read** for embedding into config
you generate in Lua, and a **deferred write** for materializing binary
blobs straight to disk.

```lua
local rb = require("rootbeer")
```

## Two shapes, two use cases

Pick the shape that matches what you're doing with the secret.

```diagram
┌─────────────────────────┬──────────────────────────────────────────┐
│ You want the value…     │ Use…                                     │
├─────────────────────────┼──────────────────────────────────────────┤
│ embedded in a string    │ rb.secret.<provider>(reference)          │
│ (config files, env vars)│   → returns string at plan time          │
├─────────────────────────┼──────────────────────────────────────────┤
│ as a standalone file    │ rb.secret.<provider>_document(ref, dst)  │
│ (SSH keys, certs, …)    │   → deferred write, bytes never in Lua   │
└─────────────────────────┴──────────────────────────────────────────┘
```

The split exists because the **plan log** records every operation rootbeer
will perform. Inline secrets read at plan time become part of the
generated file contents (which is what you want when templating a config).
Binary blobs you only want to materialize on disk should never enter that
log — the deferred form keeps the fetch in the apply phase, where the
bytes go straight from the provider CLI to the destination file.

## Plan vs apply timing

| Call                              | When the provider runs | Visible in `rb plan` output |
| --------------------------------- | ---------------------- | --------------------------- |
| `rb.secret.<provider>(ref)`       | Plan time (synchronous) | The fetched value is embedded in the resulting `WriteFile` content. |
| `rb.secret.<provider>_document(…)` | Apply time (deferred) | A `fetch <provider> <ref>` preamble + `write <dst> (deferred)` line. |

A consequence worth knowing: under the sync form, `rb plan` must already
have access to the provider (1Password unlocked, etc.) because the value
is needed to construct the plan. Under the deferred form, `rb plan` does
not touch the provider at all — useful for previewing changes in CI or
on a fresh machine.

## Providers

### 1Password (`op`)

Requires the [1Password CLI](https://developer.1password.com/docs/cli) to
be installed and signed in. Touch ID / biometric prompts surface
synchronously when the CLI runs.

**Embed a field into a generated config file** — the dominant use case
for things like API keys, tokens, and URLs that live inside dotfiles you
template:

```lua
local lines = {
    "[settings]",
    "debug = false",
    'api_url = "' .. rb.secret.op("op://Development/WakaTime/url") .. '"',
    'api_key = "' .. rb.secret.op("op://Development/WakaTime/credential") .. '"',
}
rb.file("~/.wakatime.cfg", table.concat(lines, "\n"))
```

**Materialize a binary document with strict permissions** — for SSH keys,
GPG keys, certificates, license files, or anything you'd otherwise paste
into a file. The bytes flow from `op document get` straight to disk:

```lua
rb.secret.op_document("op://Private/work-ssh-key", "~/.ssh/work_rsa", {
    mode = 0x180, -- 0o600
})
```

The `mode` option queues a `chmod` immediately after the write, so SSH
won't reject the key on the next connection.

**Reference shapes.** Both functions accept `op://`-style references:

- `op://<vault>/<item>/<field>` — a specific field within an item.
- `op://<vault>/<item>` — used with `op_document` to fetch the document
  attached to an item.

## Adding a new provider

The Lua surface follows the same two-shape convention so users get a
predictable API across providers. At the Rust layer, deferred writes
flow through a single `Op::WriteFile { source: WriteSource::<Provider> }`
variant — there is no per-provider write op. To add a provider:

1. Add a `WriteSource` variant in [`plan.rs`](https://github.com/tale/rootbeer/blob/main/crates/rootbeer-core/src/plan.rs)
   carrying whatever the provider needs to fetch at apply time (e.g.
   `Rage { ciphertext: PathBuf, identity: PathBuf }`).
2. Extend `resolve_source` in [`apply.rs`](https://github.com/tale/rootbeer/blob/main/crates/rootbeer-core/src/executor/apply.rs)
   with the shell-out, and `WriteSource::fetch_label` in `plan.rs` so
   the CLI announces the fetch automatically.
3. Add `rb.secret.<provider>(…)` (sync) and `rb.secret.<provider>_document(…)`
   (deferred) bindings in [`lua/secret.rs`](https://github.com/tale/rootbeer/blob/main/crates/rootbeer-core/src/lua/secret.rs),
   plus matching annotations in [`lua/rootbeer/secret.lua`](https://github.com/tale/rootbeer/blob/main/lua/rootbeer/secret.lua).

No CLI changes are required — the dry-run / apply output picks up the
new provider through `fetch_label`.

## API Reference

<!--@include: ../api/_generated/secret.md-->
