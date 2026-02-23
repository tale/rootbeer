--- @meta

--- Information about the current host machine and user, populated at startup
--- from POSIX syscalls (`gethostname`, `getpwuid_r`).

--- @class rootbeer.HostInfo
--- @field os string The operating system (e.g. `"macos"`, `"linux"`).
--- @field arch string CPU architecture (e.g. `"aarch64"`, `"x86_64"`).
--- @field hostname? string Machine hostname, or `nil` if it cannot be determined.
--- @field user string Current username (from passwd).
--- @field home string Home directory path (from passwd).
--- @field shell string Default login shell (from passwd, e.g. `"/bin/zsh"`).
