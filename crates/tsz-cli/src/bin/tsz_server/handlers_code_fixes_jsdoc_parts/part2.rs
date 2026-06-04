impl Server {
    /// Apply "add missing new" to ALL class constructor calls.
    pub(super) fn apply_add_missing_new_all_fallback(content: &str) -> Option<String> {
        let class_names = Self::collect_class_names(content);
        if class_names.is_empty() {
            return None;
        }
        let mut result = content.to_string();
        let mut changed = false;
        loop {
            let mut found = false;
            for class_name in &class_names {
                let pattern = format!("{class_name}(");
                let mut search_from = 0;
                while let Some(pos) = result[search_from..].find(&pattern) {
                    let abs_pos = search_from + pos;
                    let prefix = result[..abs_pos].trim_end();
                    if prefix.ends_with("new") {
                        search_from = abs_pos + 1;
                        continue;
                    }
                    if abs_pos > 0 {
                        let prev = result.as_bytes()[abs_pos - 1];
                        if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
                            search_from = abs_pos + 1;
                            continue;
                        }
                    }
                    result.insert_str(abs_pos, "new ");
                    changed = true;
                    found = true;
                    break;
                }
                if found {
                    break;
                }
            }
            if !found {
                break;
            }
        }
        changed.then_some(result)
    }
}
