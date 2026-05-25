/// Check if a filename is a Windows-style absolute path, such as
/// `A:/foo/bar.ts` or `C:\dir\file.ts`.
///
/// TSC conformance tests use Windows drive-letter paths to test cross-drive
/// scenarios. On Unix, these paths cannot represent real filesystem locations;
/// tsc's virtual filesystem also cannot find files at these paths via
/// `include` patterns, so it emits TS18003 ("No inputs found in config file").
pub(super) fn is_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}
