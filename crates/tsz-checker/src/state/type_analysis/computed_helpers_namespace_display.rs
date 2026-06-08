/// Normalize a resolved file path to the display form used in `typeof import("...")`.
///
/// Rules (mirroring tsc):
/// - Virtual-FS-root `node_modules` (`/node_modules/pkg/…`, `node_modules_idx == 0`):
///   keep the full root-relative path so the message reads
///   `import("node_modules/pkg/index")`.
/// - Paths with a virtual-root prefix (`/p123/node_modules/…`):
///   strip the absolute prefix but keep from the `p123` segment onwards.
/// - Deeper project paths (`/home/user/project/node_modules/pkg/…`):
///   strip the host/project prefix and keep the package subpath
///   (`node_modules/pkg/...`) so resolved declaration packages match tsc's
///   stable display form.
/// - No `node_modules` segment: return the trimmed path as-is.
pub(crate) fn trim_namespace_display_path(resolved_name: &str) -> String {
    let trimmed = resolved_name
        .strip_prefix("./")
        .unwrap_or(resolved_name)
        .trim_start_matches('/');

    let components: Vec<_> = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if let Some(node_modules_idx) = components
        .iter()
        .position(|segment| *segment == "node_modules")
    {
        if node_modules_idx > 0 {
            let previous = components[node_modules_idx - 1];
            let looks_like_virtual_root =
                previous.starts_with('p') && previous[1..].chars().all(|ch| ch.is_ascii_digit());
            if looks_like_virtual_root {
                return components[node_modules_idx - 1..].join("/");
            }
        }
        // Resolved declaration packages display from their stable
        // package path, not the original bare specifier. Drop any
        // host temp/project prefix before node_modules, but preserve
        // the package subpath that tsc includes in diagnostics.
        return components[node_modules_idx..].join("/");
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::trim_namespace_display_path;

    #[test]
    fn virtual_fs_root_node_modules_keeps_full_path() {
        // `/node_modules/pkg/index.d.ts` → `node_modules/pkg/index.d.ts`
        // (caller strips extension; we keep the full path including node_modules)
        assert_eq!(
            trim_namespace_display_path("/node_modules/mdast-util-to-string/index.d.ts"),
            "node_modules/mdast-util-to-string/index.d.ts"
        );
    }

    #[test]
    fn virtual_fs_root_scoped_package_keeps_full_path() {
        assert_eq!(
            trim_namespace_display_path("/node_modules/@scope/pkg/index.d.ts"),
            "node_modules/@scope/pkg/index.d.ts"
        );
    }

    #[test]
    fn deep_project_path_keeps_package_subpath() {
        // Real project: /home/user/project/node_modules/shortid/index.d.ts →
        // "node_modules/shortid/index.d.ts" (host/project prefix dropped, package
        // subpath preserved to match tsc's stable display form).
        assert_eq!(
            trim_namespace_display_path("/home/user/project/node_modules/shortid/index.d.ts"),
            "node_modules/shortid/index.d.ts"
        );
    }

    #[test]
    fn deep_project_scoped_package_keeps_full_subpath() {
        assert_eq!(
            trim_namespace_display_path("/home/user/project/node_modules/@types/react/index.d.ts"),
            "node_modules/@types/react/index.d.ts"
        );
    }

    #[test]
    fn virtual_root_prefix_path_kept() {
        // /p123/node_modules/csv-parse/lib/index.d.ts → "p123/node_modules/csv-parse/lib/index.d.ts"
        assert_eq!(
            trim_namespace_display_path("/p123/node_modules/csv-parse/lib/index.d.ts"),
            "p123/node_modules/csv-parse/lib/index.d.ts"
        );
    }

    #[test]
    fn no_node_modules_returns_trimmed() {
        assert_eq!(trim_namespace_display_path("/src/utils.ts"), "src/utils.ts");
        assert_eq!(
            trim_namespace_display_path("./src/utils.ts"),
            "src/utils.ts"
        );
        assert_eq!(trim_namespace_display_path("server.d.ts"), "server.d.ts");
    }

    #[test]
    fn relative_prefix_stripped() {
        assert_eq!(trim_namespace_display_path("./mod.d.ts"), "mod.d.ts");
    }
}
