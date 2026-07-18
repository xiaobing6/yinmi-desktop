use std::{io, path::Path};

pub async fn available_bytes(path: &Path) -> io::Result<Option<u64>> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || available_bytes_sync(&path))
        .await
        .map_err(|_| io::Error::other("disk space query task failed"))?
}

#[cfg(target_os = "windows")]
fn available_bytes_sync(path: &Path) -> io::Result<Option<u64>> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory_name: *const u16,
            free_bytes_available: *mut u64,
            total_number_of_bytes: *mut u64,
            total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut available = 0_u64;
    let mut total = 0_u64;
    let mut total_free = 0_u64;
    if unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, &mut total_free) }
        == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(Some(available))
    }
}

#[cfg(target_os = "macos")]
fn available_bytes_sync(path: &Path) -> io::Result<Option<u64>> {
    use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt};

    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let mut stats = MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let stats = unsafe { stats.assume_init() };
    Ok(Some(
        (stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64),
    ))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn available_bytes_sync(_path: &Path) -> io::Result<Option<u64>> {
    Ok(None)
}
