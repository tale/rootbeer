# Secrets

Read secrets from 1Password into generated configuration files, or save documents
such as SSH keys directly to disk.

```lua
local rb = require("rootbeer")
```

## Providers

### 1Password (`op`)

Requires the [1Password CLI](https://developer.1password.com/docs/cli) to
be installed and signed in. Touch ID / biometric prompts surface
synchronously when the CLI runs.

**Use a secret in a config file:**

```lua
local lines = {
    "[settings]",
    "debug = false",
    'api_url = "' .. rb.secret.op("op://Development/WakaTime/url") .. '"',
    'api_key = "' .. rb.secret.op("op://Development/WakaTime/credential") .. '"',
}
rb.file("~/.wakatime.cfg", table.concat(lines, "\n"))
```

**Save a document**, such as an SSH key or certificate:

```lua
rb.secret.op_document("op://Private/work-ssh-key", "~/.ssh/work_rsa", {
    mode = 0x180, -- 0o600
})
```

The `mode` option sets the file permissions so only your user can read and
write the key.

**Reference shapes.** Both functions accept `op://`-style references:

- `op://<vault>/<item>/<field>` — a specific field within an item.
- `op://<vault>/<item>` — used with `op_document` to fetch the document
  attached to an item.

## Previewing changes

`rb.secret.op()` reads a value while evaluating your configuration, including
when you run `rb apply --dry-run`. You must be signed in to 1Password, and the
value becomes part of the generated file content shown in the preview.

`rb.secret.op_document()` downloads the document only when applying changes.
A preview shows the destination without fetching or displaying its contents.

## API Reference

<!--@include: ../api/_generated/secret.md-->
