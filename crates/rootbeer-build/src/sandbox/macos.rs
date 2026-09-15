use super::*;

fn quoted(path: &Path) -> Result<String, String> {
    let value = path.to_str().ok_or("sandbox paths must be UTF-8")?;
    if value.chars().any(char::is_control) {
        return Err("sandbox paths cannot contain control characters".into());
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

pub(super) fn command(
    sandbox: &Sandbox,
    args: &[String],
    _source: &Path,
) -> Result<Command, String> {
    let mut profile = String::from(
        "(version 1)\n(deny default)\n(allow process-exec process-fork)\n(deny syscall-unix (syscall-number SYS_setsid SYS_setpgid))\n(allow signal (target self))\n(allow sysctl-read)\n(allow file-read-metadata)\n(allow file-read* (subpath \"/System/Library\") (subpath \"/usr/lib\") (subpath \"/private/var/db/dyld\") (literal \"/dev/null\") (literal \"/dev/random\") (literal \"/dev/urandom\"))\n(allow file-write* (literal \"/dev/null\"))\n",
    );
    let loader = Path::new("/System/Library/Sandbox/Profiles/dyld-support.sb");
    if loader.is_file() {
        profile.push_str(&format!("(import {})\n", quoted(loader)?));
    }
    for path in &sandbox.reads {
        let filter = if path.is_dir() { "subpath" } else { "literal" };
        profile.push_str(&format!(
            "(allow file-read* ({filter} {}))\n",
            quoted(path)?
        ));
    }
    profile.push_str(&format!(
        "(allow file-read* file-write* (subpath {}))\n",
        quoted(&sandbox.workspace)?
    ));
    let mut command = Command::new(launcher()?);
    command.args(["-p", &profile]).args(args);
    Ok(command)
}
