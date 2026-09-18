--- @meta

--- @class rootbeer.secret
--- Pluggable secret providers. Each provider exposes two complementary
--- shapes:
---
--- - A **sync read** that fetches the secret at plan time and returns the
---   value as a Lua string — designed for embedding into config files you
---   are composing in Lua (the dominant use case).
--- - A **deferred write** that schedules a fetch-and-write for the apply
---   phase — used when the secret is a binary blob (SSH key, certificate,
---   GPG key, …) that should never transit Lua memory or the plan log.
---
--- Rootbeer provisions a pinned `op` CLI; normal 1Password authentication is required. Age decryption
--- is built in and needs a native identity file or a 1Password key field.
---
--- ```lua
--- -- 1Password: embed a field into a generated config file
--- local cfg = {
---     "api_url = " .. rb.secret.op("op://Development/WakaTime/url"),
---     "api_key = " .. rb.secret.op("op://Development/WakaTime/credential"),
--- }
--- rb.file("~/.wakatime.cfg", table.concat(cfg, "\n"))
---
--- -- 1Password: materialise a binary document with strict perms
--- rb.secret.op_document("op://Private/work-ssh-key", "~/.ssh/work_rsa", {
---     mode = 0x180, -- 0o600
--- })
--- ```

--- @class rootbeer.SecretDocumentOpts
--- @field mode? integer File mode applied after the write (e.g. `0x180` for `0o600`). When set, a `Chmod` op is queued immediately after the deferred write.

--- Reads a secret from 1Password via the `op` CLI. Runs **synchronously at
--- plan time** so the value can be embedded into strings, file contents,
--- or other config you compose in Lua. Rootbeer prepares its packaged CLI on
--- first use; authentication may prompt for Touch ID / biometrics.
---
--- Use the sync form when you need the value *in Lua* (templating, config
--- composition). For raw binary files that should never enter Lua memory,
--- use `rb.secret.op_document` instead.
---
--- @param reference string The `op://` reference (e.g. `"op://vault/item/field"`).
--- @return string The secret value, with any trailing newline stripped.
function rootbeer.secret.op(reference) end

--- Materialises a 1Password document to disk via `op document get`. The
--- fetch is **deferred to the apply stage** — the binary contents never
--- enter Lua memory and the secret is never written to the plan log.
---
--- Supports `~` expansion and relative paths (anchored to the script
--- directory). Parent directories are created automatically. When
--- `opts.mode` is set, a chmod is queued immediately after the write
--- (useful for SSH keys, GPG keys, certificates, etc. that require
--- restricted permissions).
---
--- @param reference string The `op://` reference to the document (e.g. `"op://Private/work-ssh-key"`).
--- @param dest string The destination path on disk (`~` expansion supported).
--- @param opts? rootbeer.SecretDocumentOpts Optional settings.
function rootbeer.secret.op_document(reference, dest, opts) end

--- @class rootbeer.AgeOpts
--- @field identity? string Native age identity file. Supports `~` and script-relative paths. Mutually exclusive with `identity_op`.
--- @field identity_op? string 1Password `op://` field containing complete native `AGE-SECRET-KEY-...` lines. Mutually exclusive with `identity`.

--- @class rootbeer.AgeFileOpts: rootbeer.AgeOpts
--- @field mode? integer Destination permissions, from 0 to 0777. Defaults to `0x180` (0600).

--- Decrypts an age file synchronously during planning, including dry runs.
--- Supports binary or ASCII-armored ciphertext and native X25519 identities.
--- Returns UTF-8 text unchanged, including trailing newlines. Plaintext enters
--- Lua and any generated file content; avoid printing it or embedding it in
--- command arguments. Plan debug output omits file bytes, but custom handlers
--- can still inspect inline content. Use `age_file` for binary/deferred writes.
--- @param path string Ciphertext path, relative to the script directory or absolute; `~` supported.
--- @param opts rootbeer.AgeOpts Exactly one identity source is required.
--- @return string plaintext
function rootbeer.secret.age(path, opts) end

--- Decrypts and atomically writes an age file during apply. Dry runs neither
--- read ciphertext nor fetch keys. Plaintext never enters Lua or the plan.
--- Parent directories are created automatically. The destination is replaced
--- (including any symlink) only after decryption succeeds, with the specified
--- permissions. Native X25519 identities only; no passphrase, SSH or plugin keys.
--- @param path string Ciphertext path, relative to the script directory or absolute; `~` supported.
--- @param dest string Destination path; script-relative paths and `~` supported.
--- @param opts rootbeer.AgeFileOpts Identity source and optional permissions.
function rootbeer.secret.age_file(path, dest, opts) end
