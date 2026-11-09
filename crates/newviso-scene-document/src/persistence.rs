use std::{fs, io::Write, path::Path};

/// Compare-before-save, same-directory temporary file, durable atomic replacement.
pub fn write_authoring_file(path: &Path, expected: &[u8], bytes: &[u8]) -> Result<(), String> {
    if fs::read(path).map_err(|e| e.to_string())? != expected {
        return Err("file changed on disk; reopen before saving".into());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temp = path.with_extension(format!("newviso-{}-{nonce}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        if fs::read(path).map_err(|e| e.to_string())? != expected {
            return Err("file changed while preparing save".into());
        }
        replace(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let from = from
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // REPLACE_EXISTING | WRITE_THROUGH, same volume. Failure preserves the old file.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conflicting_disk_write_is_preserved() {
        let p = std::env::temp_dir().join(format!("newviso-authoring-{}.json", std::process::id()));
        fs::write(&p, b"newer").unwrap();
        assert!(write_authoring_file(&p, b"old", b"edit").is_err());
        assert_eq!(fs::read(&p).unwrap(), b"newer");
        write_authoring_file(&p, b"newer", b"saved").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"saved");
        fs::remove_file(p).unwrap();
    }
}
