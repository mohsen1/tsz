/// Returns `true` if the module specifier looks like it should be rewritten
/// by `rewriteRelativeImportExtensions`.
///
/// Mirrors tsc's `shouldRewriteModuleSpecifier`: the specifier must be a
/// relative path with a TypeScript file extension (.ts/.tsx/.mts/.cts) that
/// is NOT a declaration file (.d.ts/.d.mts/.d.cts).
pub(crate) fn should_rewrite_module_specifier(specifier: &str) -> bool {
    (specifier.starts_with("./") || specifier.starts_with("../"))
        && ts_extension_suffix(specifier).is_some()
}

/// Returns the TypeScript extension suffix (e.g. `".ts"`, `".tsx"`) if the module path
/// ends with a TS-specific extension that requires `allowImportingTsExtensions`.
/// Returns `None` for `.d.ts`/`.d.mts`/`.d.cts` (handled separately by TS2846) and
/// non-TS extensions.
pub(crate) fn ts_extension_suffix(module_name: &str) -> Option<&'static str> {
    // .d.ts/.d.mts/.d.cts are declaration files - handled by TS2846, not TS5097.
    if module_name.ends_with(".d.ts")
        || module_name.ends_with(".d.mts")
        || module_name.ends_with(".d.cts")
    {
        return None;
    }
    if module_name.ends_with(".ts") {
        Some(".ts")
    } else if module_name.ends_with(".tsx") {
        Some(".tsx")
    } else if module_name.ends_with(".mts") {
        Some(".mts")
    } else if module_name.ends_with(".cts") {
        Some(".cts")
    } else {
        None
    }
}

/// Returns `(declaration_suffix, ts_extension, js_extension)` when the module
/// specifier names a declaration file (`.d.ts`/`.d.mts`/`.d.cts`).
///
/// These specifiers are handled by TS2846 ("a declaration file cannot be
/// imported without `import type`"), never by TS5097.
pub(crate) fn declaration_file_extension(
    module_name: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    if module_name.ends_with(".d.ts") {
        Some((".d.ts", ".ts", ".js"))
    } else if module_name.ends_with(".d.mts") {
        Some((".d.mts", ".mts", ".mjs"))
    } else if module_name.ends_with(".d.cts") {
        Some((".d.cts", ".cts", ".cjs"))
    } else {
        None
    }
}

pub(crate) fn imported_types_package_target(module_name: &str) -> Option<String> {
    let package = module_name.strip_prefix("@types/")?;
    if package.is_empty() {
        return None;
    }
    if let Some((scope, name)) = package.split_once("__")
        && !scope.is_empty()
        && !name.is_empty()
    {
        return Some(format!("@{scope}/{name}"));
    }
    Some(package.to_string())
}
