--- @meta

--- @class rootbeer
--- Primitives for managing files, creating symlinks, and querying system information.

--- Information about the current host machine and user.
--- See `host.lua` for the full type definition.
--- @type rootbeer.HostInfo
rootbeer.host = {}

--- The first-class profile system. See `profile.lua` for the full type
--- definition.
--- @type profile
rootbeer.profile = {}

--- Absolute path to the rootbeer source directory (the directory containing
--- the entry-point script). Useful for commands that need to operate on the
--- source repo itself (e.g. `git remote set-url`).
--- @type string
rootbeer.source_dir = ""

--- Writes content to a file. Parent directories are created automatically.
--- Paths starting with `~` are expanded to `$HOME`; relative paths resolve
--- from the script directory.
--- @param path string The destination file path.
--- @param content string The content to write.
function rootbeer.file(path, content) end

--- Creates a symbolic link from a file in the script directory.
--- The source path is relative to the script directory and must exist.
--- The destination supports `~` expansion.
--- Idempotent — existing correct links are skipped, stale links are replaced.
--- @param src string Source path relative to the script directory.
--- @param dst string Destination path (supports `~` expansion).
function rootbeer.link_file(src, dst) end

--- Copies a file from the script directory to the destination, but only if
--- the destination does not already exist. Useful for seeding configuration
--- files that the user (or an application) is expected to modify after the
--- initial bootstrap (e.g. an editor config that records UI state).
--- The source path is relative to the script directory and must exist.
--- The destination supports `~` expansion. If the destination already exists,
--- the operation is reported as `skip`.
--- @param src string Source path relative to the script directory.
--- @param dst string Destination path (supports `~` expansion).
function rootbeer.copy_file(src, dst) end

--- Creates a symbolic link between arbitrary paths.
--- Both paths support `~` expansion and relative path resolution.
--- Unlike `link_file`, the source is not restricted to the script directory.
--- The source must exist at plan time.
--- @param src string Source path (supports `~` expansion).
--- @param dst string Destination path (supports `~` expansion).
function rootbeer.link(src, dst) end

--- Executes a command in the source directory. The command is deferred until the apply stage.
--- @param cmd string The command to run (e.g. `"brew"`).
--- @param args? string[] Optional arguments passed to the command.
function rootbeer.exec(cmd, args) end

--- Checks whether a path exists (file, directory, or symlink).
--- Supports `~` expansion and relative paths.
--- @param path string The path to check.
--- @return boolean
function rootbeer.path_exists(path) end

--- Checks whether a path is a regular file.
--- Supports `~` expansion and relative paths.
--- @param path string The path to check.
--- @return boolean
function rootbeer.is_file(path) end

--- Checks whether a path is a directory.
--- Supports `~` expansion and relative paths.
--- @param path string The path to check.
--- @return boolean
function rootbeer.is_dir(path) end

--- Sets the `origin` remote URL for the rootbeer source directory.
--- The change is deferred until the apply stage. Idempotent — skipped when
--- the current URL already matches.
--- @param url string The desired remote URL (any git URL).
function rootbeer.remote(url) end


--- @class rootbeer.Secret
rootbeer.secret = {}

--- Reads a secret from 1Password via the `op` CLI.
--- @param reference string The `op://` reference (e.g. `"op://vault/item/field"`).
--- @return string The secret value.
function rootbeer.secret.op(reference) end

--- JSON codec. See `json.lua` for the full type definition.
--- @type rootbeer.json
rootbeer.json = {}

--- TOML codec. See `toml.lua` for the full type definition.
--- @type rootbeer.toml
rootbeer.toml = {}

--- YAML codec. See `yaml.lua` for the full type definition.
--- @type rootbeer.yaml
rootbeer.yaml = {}

--- Apple plist codec. See `plist.lua` for the full type definition.
--- @type rootbeer.plist
rootbeer.plist = {}

--- Script writers — executable scripts with shebang + chmod 0755.
--- See `scripts.lua` for the full type definition.
--- @type rootbeer.scripts
rootbeer.scripts = {}
