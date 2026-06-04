/// Find the byte offset of a specific lib entry string within the source text.
/// Searches for `"entry"` within the lib array section.
pub(super) fn find_lib_entry_offset(source: &str, entry: &str) -> u32 {
    let search = format!("\"{entry}\"");
    let lib_pos = source.find("\"lib\"").unwrap_or(0);
    if let Some(pos) = source[lib_pos..].find(&search) {
        (lib_pos + pos) as u32
    } else {
        0
    }
}
