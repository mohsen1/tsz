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
