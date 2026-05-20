use tsz_parser::parser::node::NodeArena;

pub(crate) fn is_builtin_lib_file_name(file_name: &str) -> bool {
    let basename = std::path::Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name);

    if basename.starts_with("lib.") && basename.ends_with(".d.ts") {
        return true;
    }

    let stem = basename
        .strip_suffix(".generated.d.ts")
        .or_else(|| basename.strip_suffix(".d.ts"))
        .unwrap_or(basename);

    stem == "lib"
        || stem == "scripthost"
        || stem == "decorators"
        || stem == "decorators.legacy"
        || stem == "dom"
        || stem.starts_with("dom.")
        || stem == "webworker"
        || stem.starts_with("webworker.")
        || stem == "esnext"
        || stem.starts_with("esnext.")
        || (stem.starts_with("es") && stem.as_bytes().get(2).is_some_and(u8::is_ascii_digit))
}

pub(crate) fn is_builtin_lib_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        if !source_file.is_declaration_file {
            return false;
        }
        is_builtin_lib_file_name(&source_file.file_name)
    })
}

fn is_dom_like_builtin_lib_file_name(file_name: &str) -> bool {
    let basename = std::path::Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name);
    let stem = basename
        .strip_suffix(".generated.d.ts")
        .or_else(|| basename.strip_suffix(".d.ts"))
        .unwrap_or(basename);
    let stem = stem.strip_prefix("lib.").unwrap_or(stem);

    stem == "dom"
        || stem.starts_with("dom.")
        || stem == "webworker"
        || stem.starts_with("webworker.")
}

pub(crate) fn is_dom_builtin_lib_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        if !source_file.is_declaration_file {
            return false;
        }
        let basename = std::path::Path::new(&source_file.file_name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&source_file.file_name);
        let stem = basename
            .strip_suffix(".generated.d.ts")
            .or_else(|| basename.strip_suffix(".d.ts"))
            .unwrap_or(basename);
        let stem = stem.strip_prefix("lib.").unwrap_or(stem);
        stem == "dom" || stem.starts_with("dom.")
    })
}

pub(crate) fn is_direct_actual_lib_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        if !source_file.is_declaration_file {
            return false;
        }
        is_builtin_lib_file_name(&source_file.file_name)
            && !is_dom_like_builtin_lib_file_name(&source_file.file_name)
    })
}

pub(crate) fn is_external_package_declaration_file_name(file_name: &str) -> bool {
    let mut components = file_name
        .split(['/', '\\'])
        .filter(|component| !component.is_empty());
    while let Some(component) = components.next() {
        if component == "node_modules" {
            return components.next().is_some();
        }
    }
    false
}

/// Classification of a delegated arena's first source file for the
/// cross-arena symbol-type cache eligibility decision. All file-name
/// string matching that the cache layer relies on lives in this module.
/// See `cross_file_cache.rs` for the per-variant cache routing.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum DeclarationFileCacheClass {
    UserSource,
    DomOrExternalPackage,
    /// Excluded from the symbol-id-keyed cache because the name-keyed
    /// `shared_actual_lib_delegation_cache` already owns dedup for these
    /// symbols across virtual programs.
    NonDomBuiltinLib,
}

pub(crate) fn classify_declaration_file_for_cache(
    file_name: &str,
    is_declaration_file: bool,
) -> DeclarationFileCacheClass {
    if !is_declaration_file {
        return DeclarationFileCacheClass::UserSource;
    }
    if is_builtin_lib_file_name(file_name) {
        return if is_dom_like_builtin_lib_file_name(file_name) {
            DeclarationFileCacheClass::DomOrExternalPackage
        } else {
            DeclarationFileCacheClass::NonDomBuiltinLib
        };
    }
    if is_external_package_declaration_file_name(file_name) {
        return DeclarationFileCacheClass::DomOrExternalPackage;
    }
    DeclarationFileCacheClass::NonDomBuiltinLib
}
