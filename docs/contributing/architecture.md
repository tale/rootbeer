# Architecture

Rootbeer's native API is built in three layers. Each layer builds on the one
below it, keeping individual pieces small while giving users a high-level,
declarative interface.

## Layer 1 — Rust Primitives

The bottom layer defines the fundamental operations Rootbeer can perform.
These are pure Rust types with no Lua involvement.

**Plan** (`crates/rootbeer-core/src/plan.rs`) defines an `Op` enum
representing every side-effect the system can produce:

```rust
pub enum Op {
    WriteFile { path: PathBuf, content: String },
    Symlink   { src: PathBuf, dst: PathBuf },
}
```

**Executors** (`crates/rootbeer-core/src/executor/`) consume a `Vec<Op>` and
carry it out. `apply.rs` writes to the real filesystem; `dry_run.rs` only
reports what would happen.

Nothing in this layer knows about Lua. Adding a new kind of operation starts
here — add a variant to `Op`, then handle it in each executor.

## Layer 2 — Native Lua Bindings

The middle layer exposes Rust functionality to Lua scripts through
[mlua](https://docs.rs/mlua). Each module registers functions onto the
`rootbeer` global table:

| Module | Registers | Purpose |
|--------|-----------|---------|
| `fs.rs` | `rootbeer.file()`, `rootbeer.link_file()` | File writes and symlinks (append to the `Op` log) |
| `writer.rs` | `rootbeer.json`, `rootbeer.toml`, `rootbeer.ini` | Format writers (encode / decode / read / write) |
| `sys.rs` | `rootbeer.data()` | Runtime system info (OS, arch, hostname, etc.) |

Everything is wired together in `vm.rs`, which creates the Lua VM, registers
all modules, and sets up the custom `require` loader:

```rust
let rb = lua.create_table()?;
fs::register(&lua, &rb)?;
writer::register(&lua, &rb)?;
sys::register(&lua, &rb)?;
lua.globals().set("rootbeer", &rb)?;
```

### Plan/Execute Model

I/O functions like `rootbeer.file()` do **not** write to disk immediately.
They push an `Op` onto a shared `Vec<Op>` (the "run log"). The CLI later
drains that log and hands it to an executor. This separation means Lua
scripts are always safe to evaluate — no filesystem changes happen until the
user explicitly applies.

### Type Annotations

Because the Lua language server can't see into Rust, a `@meta` file at
`lua/rootbeer/core.lua` declares type signatures for every native function.
This file is never executed — it exists solely for editor tooling and doc
generation. When you add or change a native binding, update `core.lua` to
match.

## Layer 3 — High-Level Lua Modules

The top layer is pure Lua. Modules like `git.lua` and `zsh.lua` live in
`lua/rootbeer/` and provide opinionated, declarative APIs that consume the
lower layers.

A typical module follows this pattern:

1. Accept a structured config table from the user.
2. Transform it into the format the target tool expects.
3. Call `rootbeer.file()` or a format writer (`rootbeer.json.write()`,
   `rootbeer.toml.write()`, `rootbeer.ini.write()`, …) to produce output.

For example, `git.lua` takes a `git.Config` table and:

- Builds an INI structure from typed fields (`user`, `signing`, `lfs`, etc.)
- Optionally writes a `.gitignore` alongside the config via `rootbeer.file()`
- Serializes and writes the result in one step via `rootbeer.ini.write()`

Each module is self-contained with its own `@class` annotations for the
language server. Users load them via `require("rootbeer.git")`.

## Lua Standard Library Loading

The `rootbeer.*` modules in `lua/rootbeer/` can be loaded two ways:

- **Filesystem (debug builds)** — Modules are read from disk via `FsRequirer`,
  using the `ROOTBEER_LUA_DIR` path set at compile time. This means `cargo run`
  picks up Lua changes immediately with no Rust recompile.
- **Embedded (release builds)** — When the `embedded-stdlib` feature is enabled
  (it is by default), release builds bake every module into the binary via
  `include_str!`. The `EmbeddedRequirer` serves them from memory so the binary
  is fully self-contained.

The selection is automatic: `cargo build` (debug) always uses the filesystem,
`cargo build --release` uses embedded. Passing `--lua-dir` to the CLI forces
filesystem loading in either mode.

See [Packaging](./packaging) for distribution-specific build instructions.

## Adding a New Module

1. **Primitives** — If the module needs a new kind of side-effect, add an
   `Op` variant and handle it in the executors.
2. **Bindings** — If the module needs a new native function or serializer,
   add it in `crates/rootbeer-core/src/lua/` and register it in `vm.rs`.
   Update `lua/rootbeer/core.lua` with the type signature.
3. **Lua module** — Create `lua/rootbeer/<name>.lua`. Define `@class` types,
   accept a config table, transform it, and call the lower-level APIs.
4. **Docs** — Add a page in `docs/modules/<name>.md` and register it in
   `.vitepress/config.ts`.
