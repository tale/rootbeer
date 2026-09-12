# Distributing Rootbeer

The default `embedded-stdlib` feature packages the Lua standard library and type
metadata inside `rb`. The resulting binary can run without a separate Lua
installation or standard-library checkout.

```sh
cargo build --locked --release
```

Distributions that require Lua source files on disk can disable that feature and
choose a compile-time standard-library directory:

```sh
ROOTBEER_LUA_DIR=/usr/share/rootbeer/lua \
  cargo build --locked --release --no-default-features
```

Ship the repository's `lua/rootbeer/` tree under that directory. Users can override
the directory with `rb --lua-dir /path/to/lua apply`.

Official package-catalog access is configured separately through the public
endpoint and verification key embedded at build time. See
[index hosting and trust](/contributing/package-hosting). Do not package the
publisher's private signing key.
