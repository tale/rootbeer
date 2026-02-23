use mlua::{IntoLua, Lua, Result as LuaResult, Table, Value};

/// Information about the host system.
struct HostInfo {
    /// The operating system, e.g. "linux", "windows", "macos".
    os: String,

    /// The CPU architecture, e.g. "x86_64", "aarch64".
    arch: String,

    /// The hostname of the machine, if available.
    hostname: Option<String>,

    /// The username of the current user.
    user: String,

    /// The home directory of the current user.
    home: String,

    /// The login shell of the current user.
    shell: String,
}

/// Unsafely retrieves the hostname using libc. Returns None if the call fails.
fn get_hostname() -> Option<String> {
    let size = unsafe { libc::sysconf(libc::_SC_HOST_NAME_MAX + 1).max(256) } as usize;
    let mut buf = vec![0u8; size];

    let result = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), size) };
    if result != 0 {
        return None;
    }

    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8(buf[..len].to_vec()).ok()
}

/// Information from the passwd entry for the current user.
struct PasswdInfo {
    name: String,
    dir: String,
    shell: String,
}

/// Unsafely retrieves the passwd entry for the current user using libc.
/// Returns None if the call fails.
fn get_passwd() -> Option<PasswdInfo> {
    let uid = unsafe { libc::getuid() };
    let buf_size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX).max(4096) } as usize;
    let mut buf = vec![0u8; buf_size];

    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let ret = unsafe {
        libc::getpwuid_r(
            uid,
            &mut pwd,
            buf.as_mut_ptr().cast(),
            buf_size,
            &mut result,
        )
    };

    if ret != 0 || result.is_null() {
        return None;
    }

    let to_string = |ptr: *const libc::c_char| -> Option<String> {
        if ptr.is_null() {
            return None;
        }

        let len = unsafe { libc::strlen(ptr) };
        let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
        std::str::from_utf8(bytes).ok().map(String::from)
    };

    Some(PasswdInfo {
        name: to_string(pwd.pw_name)?,
        dir: to_string(pwd.pw_dir)?,
        shell: to_string(pwd.pw_shell)?,
    })
}

impl IntoLua for HostInfo {
    fn into_lua(self, lua: &Lua) -> LuaResult<Value> {
        let table = lua.create_table()?;
        table.set("os", self.os)?;
        table.set("arch", self.arch)?;
        table.set("hostname", self.hostname)?;
        table.set("user", self.user)?;
        table.set("home", self.home)?;
        table.set("shell", self.shell)?;
        Ok(Value::Table(table))
    }
}

pub(crate) fn register(table: &Table) -> LuaResult<()> {
    let passwd = get_passwd()
        .ok_or_else(|| mlua::Error::runtime("failed to read passwd entry for current user"))?;

    let host_info = HostInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        hostname: get_hostname(),
        user: passwd.name,
        home: passwd.dir,
        shell: passwd.shell,
    };

    table.set("host", host_info)?;
    Ok(())
}
