Rootbeer is a rust library and command line tool that executes a user-provided
lua script in a sandboxed environment, creating a system configuration. Think of
it akin to a dotfile manager like home-manager or chezmoi.

- `crates/rootbeer-cli`: The command line tool the user interacts with
- `crates/rootbeer-core`: The core library that runs the lua script

User configuration is provided through a layering system in the core library,
where the base fundamentals (such as symlinking files, creating new files,
running commands, etc.) are provided by the library, callable from the user's
lua script.

Because the lua scripts are meant to run with 0 external dependencies, the core
library also builds up fundamentals such as JSON serialization, string formats,
and more (akin to Nix's stdlib).

The highest API layer is mostly defined in Lua and wraps the lower level APIs
in nice types, functions, and design patterns. For example, a `zsh` module is
defined in the highest layer which consumes the lower level APIs, allowing the
user to follow a nicely typed API to manage their zsh configuration.

This pattern needs to remain consistent across all modules and there are a few
different tools built around these assumptions defined below:

- I/O operations run in a plan/execute mode, where calls only append to a log of
  operations that need to be executed on the apply stage.
- The lua language server is used to automatically generate markdown docs for
  the documentation site defined in the `docs` directory (built with Vitepress).

