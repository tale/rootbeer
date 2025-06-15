# Rootbeer
> Deterministically manage your system configuration with lua!

This tool aims to make managing your system configuration easy.
You can think of it as a tool that's very similar in function to
[home-manager](https://github.com/nix-community/home-manager).
The idea is that you can define exactly how you want your system (or userland)
to be configured within a lua script and then run the tool to apply those
changes to your system.

Why not Nix? I have plenty of reasons why I don't like Nix which I go into on
my [blog post](https://tale.me/blog/nix-might-be-overengineered/). However,
I'm also not a fan of the `nix` language and wanted to see if I could make
something similar with a language that is fully functional, scriptable, and
has good error messages (lua). Also, this is a fun project to work on!

## Goals
> At the moment this project will not work if you try to compile it.
> Much of the functionality is just flat out missing and is TODO.

- Be able to define system configurations in a lua script
- Maintain a store of configurations, allowing rollbacks and revisions
- Interface with OS-specific tools to apply configurations
    - Package managers such as `brew` on macOS or `dnf` on Fedora
    - Daemon services such as `launchd` on macOS or `systemd` on Linux

## Technical Components
I plan on extracting this out to separate documentation later, but here's a
quick overview of the technical components of this project and how they all
work together. The end result is `rootbeer`, a CLI tool to control everything.

Here are the components:
- An embedded LuaJIT interpreter to load and evaluate lua scripts. All of the
functionality provided by Rootbeer will be exposed to the lua script via the
`rootbeer` module.

- A store system to manage configurations. This will be similar to Nix's store
system, where configurations are stored in a directory and can be rolled back
or revised.

- A pluggable module system that serves as the basis of all functionality
offered by Rootbeer. The idea is that different integrations with the OSes
can be written as modules and compiled in-tree to ship in the final binary.

## Building
The project is still in its early stages, stuff might break!
The only prerequisite is that you have `meson` and `ninja` installed.

You'll need to setup and run the build like so:
```bash
meson setup build
meson compile -C build
```

This will create the `./build/rb` binary which you can run to
interact with the tool.

I highly recommend using [mise](https://mise.jdx.dev/) for anything else
as I have automatically implemented almost all the commands you need and
dependencies. With mise you can just run `mise run build`.

### Building documentation
The documentation is hard to build mostly because Doxygen is not available
in a portable way. You'll need to preinstall Doxygen and ensure it is
in your `PATH`. You'll also need to activate a python virtualenv (automatically
handled if you are using mise) and install the `docs/requirements.txt`.

Once done, you can build the documentation with:
```bash
cd docs/
doxygen
sphinx-build -b html source _build/html

# Or with mise: mise build docs
```
