//! File and symbol admission predicates for direct cross-file fast paths.

use crate::context::CheckerContext;
use tsz_parser::parser::node::NodeArena;

/// Track 7 transitional allowlist for actual-lib type-alias bodies that can be
/// lowered directly across checker arenas. Additions should move toward stable
/// lib identity queries instead of expanding name-only admissions.
pub(super) const DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSIONS: &[&str] = &[
    "Capitalize",
    "DecoratorMetadata",
    "DecoratorMetadataObject",
    "Exclude",
    "Extract",
    "FlatArray",
    "IteratorResult",
    "LocalesArgument",
    "Lowercase",
    "NonNullable",
    "NumberFormatOptionsCurrencyDisplay",
    "NumberFormatOptionsSignDisplay",
    "NumberFormatOptionsStyle",
    "NumberFormatOptionsUseGrouping",
    "NumberFormatPartTypes",
    "NumberFormatRangePartTypes",
    "Omit",
    "Partial",
    "Pick",
    "PropertyKey",
    "Readonly",
    "Record",
    "Required",
    "ReturnType",
    "Uncapitalize",
    "UnicodeBCP47LocaleIdentifier",
    "Uppercase",
    "WeakKey",
];

pub(super) fn is_direct_actual_lib_alias_body_admitted(name: &str) -> bool {
    DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSIONS.contains(&name)
}

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

pub(crate) fn is_direct_actual_lib_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        if !source_file.is_declaration_file {
            return false;
        }
        is_builtin_lib_file_name(&source_file.file_name)
            && !is_dom_like_builtin_lib_file_name(&source_file.file_name)
    })
}

pub(super) fn is_external_package_declaration_file_name(file_name: &str) -> bool {
    file_name.starts_with("node_modules/")
        || file_name.starts_with("node_modules\\")
        || file_name.contains("/node_modules/")
        || file_name.contains("\\node_modules\\")
}

pub(super) fn is_direct_lowering_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        source_file.is_declaration_file
            && is_external_package_declaration_file_name(&source_file.file_name)
            && !is_builtin_lib_file_name(&source_file.file_name)
    })
}

pub(super) fn is_direct_type_alias_declaration_arena(arena: &NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        source_file.is_declaration_file
            && (is_builtin_lib_file_name(&source_file.file_name)
                || is_external_package_declaration_file_name(&source_file.file_name))
    })
}

pub(super) fn is_direct_lowering_source_file_arena(arena: &NodeArena) -> bool {
    arena
        .source_files
        .first()
        .is_some_and(|source_file| !source_file.is_declaration_file)
}

pub(super) fn allow_generic_actual_lib_direct_fallback(name: &str) -> bool {
    matches!(
        name,
        "Array"
            | "ArrayIterator"
            | "Iterator"
            | "Map"
            | "MapIterator"
            | "Object"
            | "Promise"
            | "PromiseLike"
            | "RegExpStringIterator"
            | "Set"
            | "SetIterator"
            | "StringIterator"
            | "WeakMap"
            | "WeakSet"
    )
}

pub(super) fn allow_actual_lib_declaration_proof_bypass(name: &str) -> bool {
    matches!(name, "Iterator")
}

pub(super) fn is_direct_actual_lib_value_interface_name(name: &str) -> bool {
    matches!(
        name,
        "Array"
            | "Date"
            | "DateTimeFormatOptions"
            | "Error"
            | "Function"
            | "Iterator"
            | "IteratorObject"
            | "Locale"
            | "Map"
            | "NumberFormatOptions"
            | "NumberFormatOptionsCurrencyDisplayRegistry"
            | "NumberFormatOptionsSignDisplayRegistry"
            | "NumberFormatOptionsStyleRegistry"
            | "NumberFormatOptionsUseGroupingRegistry"
            | "Object"
            | "Promise"
            | "Set"
            | "Symbol"
            | "WeakMap"
            | "WeakSet"
    )
}

pub(super) fn iterator_object_has_global_augmentations(ctx: &CheckerContext<'_>) -> bool {
    if ctx
        .binder
        .global_augmentations
        .get("IteratorObject")
        .is_some_and(|augmentations| !augmentations.is_empty())
    {
        return true;
    }

    ctx.binder
        .file_locals
        .get("IteratorObject")
        .and_then(|sym_id| ctx.binder.get_symbol(sym_id))
        .is_some_and(|symbol| symbol.declarations.len() > 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_actual_lib_alias_admission_list_is_track7_ratchet() {
        const DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSION_CEILING: usize = 28;

        let admitted = DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSIONS;
        assert_eq!(
            admitted.len(),
            DIRECT_ACTUAL_LIB_ALIAS_BODY_ADMISSION_CEILING,
            "Track 7 actual-lib alias admissions are transitional; replace \
             name-only admissions with stable lib identity queries before growing \
             this ceiling.",
        );
        assert!(
            admitted.windows(2).all(|pair| pair[0] < pair[1]),
            "Keep actual-lib alias admissions sorted so additions are reviewable: {admitted:?}",
        );
        for name in admitted {
            assert!(
                is_direct_actual_lib_alias_body_admitted(name),
                "{name} must be admitted by the shared classifier",
            );
        }
        for name in ["Array", "Date", "Iterator", "Promise", "ReadonlyArray"] {
            assert!(
                !is_direct_actual_lib_alias_body_admitted(name),
                "{name} is an interface/value helper, not a type-alias body admission",
            );
        }
    }

    #[test]
    fn detects_npm_and_source_tree_builtin_lib_names() {
        assert!(is_builtin_lib_file_name("lib.es2024.d.ts"));
        assert!(is_builtin_lib_file_name("lib.dom.d.ts"));
        assert!(is_builtin_lib_file_name("es2024.d.ts"));
        assert!(is_builtin_lib_file_name("es2024.full.d.ts"));
        assert!(is_builtin_lib_file_name("dom.generated.d.ts"));
        assert!(is_builtin_lib_file_name("dom.iterable.generated.d.ts"));
        assert!(is_builtin_lib_file_name("webworker.asynciterable.d.ts"));
        assert!(is_builtin_lib_file_name("decorators.legacy.d.ts"));
    }

    #[test]
    fn does_not_treat_arbitrary_declaration_files_as_builtin_libs() {
        assert!(!is_builtin_lib_file_name("react/index.d.ts"));
        assert!(!is_builtin_lib_file_name(
            "node_modules/@types/node/fs.d.ts"
        ));
        assert!(!is_builtin_lib_file_name("packages/foo/src/types.d.ts"));
    }

    #[test]
    fn detects_external_package_declaration_paths() {
        assert!(is_external_package_declaration_file_name(
            "node_modules/react/index.d.ts"
        ));
        assert!(is_external_package_declaration_file_name(
            "/repo/node_modules/@types/node/fs.d.ts"
        ));
        assert!(is_external_package_declaration_file_name(
            r"C:\repo\node_modules\@types\node\fs.d.ts"
        ));
    }

    #[test]
    fn does_not_treat_local_declaration_paths_as_external_packages() {
        assert!(!is_external_package_declaration_file_name(
            "packages/foo/src/types.d.ts"
        ));
        assert!(!is_external_package_declaration_file_name(
            "/repo/fixtures/node-modules-like/types.d.ts"
        ));
    }
}
