use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn extract(
    source: &Path,
    destination: &Path,
    apps: &BTreeMap<String, PathBuf>,
) -> io::Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "DMG installation requires macOS",
        ));
    }
    crate::catalog::validate_apps(apps).map_err(io::Error::other)?;
    if apps.is_empty() {
        return Err(io::Error::other(
            "DMG installation requires declared application bundles",
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let mut image = macos::MountedImage::attach(source)?;
        let result = image.copy_apps(destination, apps);
        let detached = image.detach();
        match (result, detached) {
            (Ok(()), result) | (result, Ok(())) => result,
            (Err(copy), Err(detach)) => Err(io::Error::other(format!("{copy}; {detach}"))),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (source, destination);
        unreachable!()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::CString;
    use std::fs;
    use std::os::unix::fs::{symlink, MetadataExt};
    use std::process::{Command, Stdio};

    pub(super) struct MountedImage {
        mountpoint: Option<tempfile::TempDir>,
        device: u64,
    }

    impl MountedImage {
        pub(super) fn attach(source: &Path) -> io::Result<Self> {
            let mountpoint = tempfile::Builder::new().prefix("rootbeer-dmg-").tempdir()?;
            let image = Self {
                device: fs::metadata(mountpoint.path())?.dev(),
                mountpoint: Some(mountpoint),
            };
            run(Command::new("/usr/bin/hdiutil")
                .arg("attach")
                .arg(source.canonicalize()?)
                .args(["-readonly", "-nobrowse", "-noautoopen", "-mountpoint"])
                .arg(image.path()))?;
            Ok(image)
        }

        fn path(&self) -> &Path {
            self.mountpoint.as_ref().unwrap().path()
        }

        pub(super) fn copy_apps(
            &self,
            destination: &Path,
            apps: &BTreeMap<String, PathBuf>,
        ) -> io::Result<()> {
            let mountpoint = self.path().canonicalize()?;
            for relative in apps.values() {
                let source = mountpoint.join(relative);
                if !fs::symlink_metadata(&source)?.is_dir()
                    || !source.canonicalize()?.starts_with(&mountpoint)
                {
                    return Err(io::Error::other(format!(
                        "DMG application must be a contained directory: {}",
                        relative.display()
                    )));
                }
                copy_bundle(
                    &source,
                    &destination.join(relative),
                    &source.canonicalize()?,
                )?;
            }
            Ok(())
        }

        pub(super) fn detach(&mut self) -> io::Result<()> {
            let Some(mountpoint) = self.mountpoint.take() else {
                return Ok(());
            };
            if fs::metadata(mountpoint.path()).is_ok_and(|metadata| metadata.dev() == self.device) {
                return Ok(());
            }
            let result = run(Command::new("/usr/bin/hdiutil")
                .arg("detach")
                .arg(mountpoint.path()))
            .or_else(|_| {
                run(Command::new("/usr/bin/hdiutil")
                    .args(["detach", "-force"])
                    .arg(mountpoint.path()))
            });
            if let Err(error) = result {
                // Never let TempDir recursively remove a mount that could not be detached.
                let path = mountpoint.keep();
                return Err(io::Error::other(format!(
                    "could not detach DMG at {}: {error}",
                    path.display()
                )));
            }
            Ok(())
        }
    }

    impl Drop for MountedImage {
        fn drop(&mut self) {
            if let Err(error) = self.detach() {
                eprintln!("{error}");
            }
        }
    }

    fn run(command: &mut Command) -> io::Result<()> {
        let output = command.stdin(Stdio::null()).output()?;
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "hdiutil failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    fn copy_bundle(source: &Path, destination: &Path, root: &Path) -> io::Result<()> {
        let metadata = fs::symlink_metadata(source)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(source)?;
            if target.is_absolute() || !source.canonicalize()?.starts_with(root) {
                return Err(io::Error::other(format!(
                    "DMG bundle symlink must stay inside its bundle: {}",
                    source.display()
                )));
            }
            return symlink(target, destination);
        }
        reject_external_signature(source)?;
        if metadata.is_file() {
            fs::copy(source, destination)?;
            return Ok(());
        }
        if !metadata.is_dir() {
            return Err(io::Error::other(format!(
                "unsupported DMG bundle entry: {}",
                source.display()
            )));
        }

        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_bundle(&entry.path(), &destination.join(entry.file_name()), root)?;
        }
        fs::set_permissions(destination, metadata.permissions())
    }

    fn reject_external_signature(path: &Path) -> io::Result<()> {
        let name = CString::new(path.as_os_str().as_encoded_bytes())?;
        let size = unsafe {
            libc::getxattr(
                name.as_ptr(),
                c"com.apple.cs.CodeDirectory".as_ptr(),
                std::ptr::null_mut(),
                0,
                0,
                libc::XATTR_NOFOLLOW,
            )
        };
        if size >= 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("DMG bundle uses extended-attribute code signatures, which package archives cannot preserve: {}", path.display()),
            ));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOATTR) {
            return Ok(());
        }
        Err(error)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn rejects_external_code_signatures_before_copying() {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("signed.dll");
            let destination = root.path().join("copied.dll");
            fs::write(&source, "fixture").unwrap();
            let output = Command::new("/usr/bin/xattr")
                .args(["-w", "com.apple.cs.CodeDirectory", "fixture"])
                .arg(&source)
                .output()
                .unwrap();
            assert!(output.status.success());
            let error = copy_bundle(&source, &destination, root.path()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::Unsupported);
            assert!(error
                .to_string()
                .contains("extended-attribute code signatures"));
            assert!(!destination.exists());
        }

        #[test]
        fn rejects_bundle_links_outside_the_bundle() {
            let root = tempfile::tempdir().unwrap();
            let bundle = root.path().join("Demo.app");
            fs::create_dir(&bundle).unwrap();
            fs::write(root.path().join("outside"), "outside").unwrap();
            for target in [PathBuf::from("../outside"), root.path().join("outside")] {
                let link = bundle.join("escape");
                symlink(target, &link).unwrap();
                let error = copy_bundle(
                    &link,
                    &root.path().join("copied"),
                    &bundle.canonicalize().unwrap(),
                )
                .unwrap_err();
                assert!(error.to_string().contains("inside its bundle"));
                assert!(!root.path().join("copied").exists());
                fs::remove_file(link).unwrap();
            }
        }

        #[test]
        fn detaches_after_a_missing_bundle_and_preserves_relative_links() {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source");
            let bundle = source.join("Demo.app");
            fs::create_dir_all(bundle.join("Versions/A")).unwrap();
            fs::write(bundle.join("Versions/A/resource"), "resource").unwrap();
            symlink("A", bundle.join("Versions/Current")).unwrap();
            symlink("/Applications", source.join("Applications")).unwrap();
            let archive = root.path().join("test.dmg");
            run(Command::new("/usr/bin/hdiutil")
                .args(["create", "-fs", "HFS+", "-format", "UDZO", "-srcfolder"])
                .arg(&source)
                .arg(&archive))
            .unwrap();

            let mut image = MountedImage::attach(&archive).unwrap();
            let mountpoint = image.path().to_owned();
            let output = root.path().join("output");
            let apps = BTreeMap::from([("Demo.app".into(), "Demo.app".into())]);
            image.copy_apps(&output, &apps).unwrap();
            assert_eq!(
                fs::read_link(output.join("Demo.app/Versions/Current")).unwrap(),
                Path::new("A")
            );
            assert_eq!(
                fs::read(output.join("Demo.app/Versions/Current/resource")).unwrap(),
                b"resource"
            );
            assert!(!output.join("Applications").exists());
            assert!(image
                .copy_apps(
                    &output,
                    &BTreeMap::from([("Missing.app".into(), "Missing.app".into())])
                )
                .is_err());
            image.detach().unwrap();
            assert!(!mountpoint.exists());

            let image = MountedImage::attach(&archive).unwrap();
            let mountpoint = image.path().to_owned();
            drop(image);
            assert!(!mountpoint.exists());
        }
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn rejects_dmg_on_other_platforms() {
        let error =
            extract(Path::new("unused"), Path::new("unused"), &BTreeMap::new()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }
}
