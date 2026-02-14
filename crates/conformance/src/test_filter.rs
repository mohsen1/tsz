use std::path::Path;

pub fn matches_path_filter(_path: &Path, _filter: Option<&str>) -> bool {
    let Some(filter) = _filter else {
        return true;
    };
    let normalized = _path.to_string_lossy().replace('\\', "/");
    normalized.contains(filter)
}
