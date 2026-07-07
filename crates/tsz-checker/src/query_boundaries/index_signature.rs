//! AST-level index signature parameter type validity checks.
//!
//! Mirrors tsc's `isValidIndexKeyType` for the AST surface. Used at TS1268
//! emission sites as a fallback when the resolved key `TypeId` is a composite
//! (e.g. a `string | number` union, or a `string & Brand` intersection) that
//! doesn't match the primitive equality check but the AST shape is structurally
//! valid.

use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::objects::index_signatures::IndexSignatureResolver;

pub(crate) use tsz_solver::IndexSignature;
pub(crate) use tsz_solver::objects::index_signatures::IndexKind;

/// Resolve the string index signature value type from `type_id`.
///
/// Checker callers ask this boundary rather than constructing the raw solver
/// resolver directly, so resolver ownership stays in one place.
pub(crate) fn resolve_string_index(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    IndexSignatureResolver::new(db).resolve_string_index(type_id)
}

/// Resolve the numeric index signature value type from `type_id`.
pub(crate) fn resolve_number_index(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    IndexSignatureResolver::new(db).resolve_number_index(type_id)
}

/// Resolve the `symbol` index signature value type from `type_id`.
///
/// Returns `Some` only for a signature keyed by the bare `symbol` intrinsic; a
/// `string` index signature (which cannot accept `symbol` keys) yields `None`.
pub(crate) fn resolve_symbol_index(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    IndexSignatureResolver::new(db).resolve_symbol_index(type_id)
}

/// Return whether `type_id` exposes a `symbol` index signature.
pub(crate) fn has_symbol_index_signature(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    resolve_symbol_index(db, type_id).is_some()
}

/// Return the declared string index signature for `type_id`, if present.
pub(crate) fn string_index_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<IndexSignature> {
    IndexSignatureResolver::new(db)
        .get_index_info(type_id)
        .string_index
}

/// Return the declared numeric index signature for `type_id`, if present.
pub(crate) fn number_index_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<IndexSignature> {
    IndexSignatureResolver::new(db)
        .get_index_info(type_id)
        .number_index
}

/// Return the declared `symbol` index signature for `type_id`, if present.
pub(crate) fn symbol_index_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<IndexSignature> {
    IndexSignatureResolver::new(db)
        .get_index_info(type_id)
        .symbol_index
}

/// Return whether `type_id` exposes the requested index signature kind.
pub(crate) fn has_index_signature(db: &dyn TypeDatabase, type_id: TypeId, kind: IndexKind) -> bool {
    IndexSignatureResolver::new(db).has_index_signature(type_id, kind)
}

/// Return whether `type_id` exposes either broad object index signature.
pub(crate) fn has_string_or_number_index_signature(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    has_index_signature(db, type_id, IndexKind::String)
        || has_index_signature(db, type_id, IndexKind::Number)
}

pub(crate) fn index_key_type_satisfies_index_signature(
    db: &dyn TypeDatabase,
    index_type: TypeId,
    signature_key_type: TypeId,
) -> bool {
    matches!(signature_key_type, TypeId::STRING | TypeId::SYMBOL)
        || tsz_solver::relations::subtype::is_subtype_of(db, index_type, signature_key_type)
}

/// Mirror tsc's `everyType(type, isValidIndexKeyType)` over a *resolved* index
/// key `TypeId`.
///
/// tsc validates index-signature parameter types against the resolved type
/// rather than its syntactic spelling: a union is a valid key iff every
/// constituent is, an intersection iff some constituent is, and the leaf cases
/// are `string`/`number`/`symbol` and template-literal (pattern) types. Crucially
/// this is *not* an assignability test — `any` is assignable to
/// `string | number | symbol` yet is rejected by tsc, so we inspect the type's
/// shape directly.
///
/// Callers must resolve the top-level key type before invoking this (e.g. the
/// lib global `PropertyKey` is a `Lazy(DefId)` alias for
/// `string | number | symbol`); union/intersection members are expected to be
/// resolved as a side effect of building the compound type. Spellings that
/// can't be reached this way are still covered by the AST fallback at the call
/// sites, so this only ever participates in *accepting* a key type.
pub(crate) fn resolved_index_key_type_is_valid(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // `everyType` distributes the predicate over union constituents.
    if let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id) {
        return !members.is_empty()
            && members
                .iter()
                .all(|&member| resolved_index_key_constituent_is_valid(db, member));
    }
    resolved_index_key_constituent_is_valid(db, type_id)
}

/// True when a *resolved* index-signature key type is a concrete index key:
/// free of type parameters *and* structurally valid per
/// [`resolved_index_key_type_is_valid`].
///
/// tsc decides TS1337 ("a literal type or generic type") from the resolved key
/// type via `someType(isStringOrNumberLiteralOrUnique) || isGenericType(type)`,
/// never the syntactic spelling. The checker's AST pre-pass
/// (`is_type_param_or_literal_in_index_sig`) can over-report "generic" for a
/// generic-alias *application* whose uninstantiated body mentions the alias's
/// own bound type parameters — e.g. `Brand<string, 'event'>` with
/// `type Brand<T, Tag> = T & { __tag: Tag }`, which instantiates to the
/// concrete, valid key `string & { __tag: 'event' }`. This predicate lets the
/// caller accept such an instantiated key while still emitting TS1337 for a
/// genuinely generic key like `T & string` (which carries a free type
/// parameter) or a literal key like `'a' | 'b'` (which is not a valid key).
pub(crate) fn resolved_index_key_is_concrete_valid(
    db: &dyn TypeDatabase,
    resolved_key: TypeId,
) -> bool {
    !tsz_solver::type_queries::contains_free_type_parameters_db(db, resolved_key)
        && resolved_index_key_type_is_valid(db, resolved_key)
}

/// Validity of a single (non-union) resolved constituent. The generic
/// intersection case (`T & string`) is steered to TS1337 by the AST pre-check
/// at the call sites before validity is consulted, so this mirrors tsc's
/// `some(types, isValidIndexKeyType)` for the intersection arm.
fn resolved_index_key_constituent_is_valid(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if matches!(type_id, TypeId::STRING | TypeId::NUMBER | TypeId::SYMBOL) {
        return true;
    }
    if tsz_solver::type_queries::is_template_literal_type(db, type_id) {
        return true;
    }
    if let Some(members) = tsz_solver::type_queries::get_intersection_members(db, type_id) {
        return members
            .iter()
            .any(|&member| resolved_index_key_constituent_is_valid(db, member));
    }
    false
}

/// Structural AST check for index-signature parameter type validity.
///
/// Accepts `string`/`number`/`symbol` keywords, template literal types, type
/// aliases that resolve to one of the above, unions whose members are all
/// valid, and non-generic intersections where some member is valid (e.g.
/// `string & Brand`, or two pattern-literal templates intersected).
pub(crate) fn is_valid_index_sig_param_type_ast(
    arena: &NodeArena,
    binder: &tsz_binder::BinderState,
    type_annotation_idx: NodeIndex,
) -> bool {
    let Some(type_node) = arena.get(type_annotation_idx) else {
        return false;
    };
    match type_node.kind {
        k if k == SyntaxKind::StringKeyword as u16 => true,
        k if k == SyntaxKind::NumberKeyword as u16 => true,
        k if k == SyntaxKind::SymbolKeyword as u16 => true,
        k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => true,
        k if k == syntax_kind_ext::UNION_TYPE => {
            arena
                .get_composite_type(type_node)
                .is_some_and(|composite| {
                    !composite.types.nodes.is_empty()
                        && composite
                            .types
                            .nodes
                            .iter()
                            .all(|&m| is_valid_index_sig_param_type_ast(arena, binder, m))
                })
        }
        k if k == syntax_kind_ext::INTERSECTION_TYPE => arena
            .get_composite_type(type_node)
            .is_some_and(|composite| {
                // Accept the intersection only when at least one member is
                // a structurally valid index-sig type AND no member contains
                // a generic type parameter or literal. This prevents
                // `T & string` from being treated as valid (which would
                // suppress the more specific TS1337 diagnostic).
                let any_valid = composite
                    .types
                    .nodes
                    .iter()
                    .any(|&m| is_valid_index_sig_param_type_ast(arena, binder, m));
                let any_generic_or_literal = composite
                    .types
                    .nodes
                    .iter()
                    .any(|&m| contains_type_param_or_literal_ast(arena, binder, m));
                any_valid && !any_generic_or_literal
            }),
        k if k == syntax_kind_ext::TYPE_REFERENCE => {
            let Some(type_ref) = arena.get_type_ref(type_node) else {
                return false;
            };
            if let Some(name_node) = arena.get(type_ref.type_name)
                && let Some(ident) = arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.as_str();
                if matches!(name, "string" | "number" | "symbol") {
                    return true;
                }
            }
            if let Some(sym_id) = binder.resolve_identifier(arena, type_ref.type_name)
                && let Some(symbol) = binder.get_symbol(sym_id)
                && (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
                && let Some(&decl_idx) = symbol.declarations.first()
                && let Some(decl_node) = arena.get(decl_idx)
                && let Some(type_alias) = arena.get_type_alias(decl_node)
            {
                return is_valid_index_sig_param_type_ast(arena, binder, type_alias.type_node);
            }
            false
        }
        _ => false,
    }
}

/// AST-level check: does `type_annotation_idx` contain (recursively) a
/// generic type parameter reference or a literal type? Used to gate the
/// intersection arm of `is_valid_index_sig_param_type_ast` so that
/// `T & string` is rejected (and the more specific TS1337 diagnostic
/// can fire instead of being suppressed).
pub(crate) fn contains_type_param_or_literal_ast(
    arena: &NodeArena,
    binder: &tsz_binder::BinderState,
    type_annotation_idx: NodeIndex,
) -> bool {
    let Some(type_node) = arena.get(type_annotation_idx) else {
        return false;
    };

    if type_node.kind == syntax_kind_ext::LITERAL_TYPE
        || type_node.kind == SyntaxKind::StringLiteral as u16
        || type_node.kind == SyntaxKind::NumericLiteral as u16
        || type_node.kind == SyntaxKind::TrueKeyword as u16
        || type_node.kind == SyntaxKind::FalseKeyword as u16
    {
        return true;
    }

    if type_node.kind == syntax_kind_ext::UNION_TYPE
        || type_node.kind == syntax_kind_ext::INTERSECTION_TYPE
    {
        if let Some(composite) = arena.get_composite_type(type_node) {
            return composite
                .types
                .nodes
                .iter()
                .any(|&m| contains_type_param_or_literal_ast(arena, binder, m));
        }
        return false;
    }

    if type_node.kind == syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(type_node)
        && let Some(sym_id) = binder.resolve_identifier(arena, type_ref.type_name)
        && let Some(symbol) = binder.get_symbol(sym_id)
    {
        if (symbol.flags & tsz_binder::symbol_flags::TYPE_PARAMETER) != 0 {
            return true;
        }
        // Recurse into a type alias body so `type S = T & string` is also caught.
        if (symbol.flags & tsz_binder::symbol_flags::TYPE_ALIAS) != 0
            && let Some(&decl_idx) = symbol.declarations.first()
            && let Some(decl_node) = arena.get(decl_idx)
            && let Some(type_alias) = arena.get_type_alias(decl_node)
        {
            return contains_type_param_or_literal_ast(arena, binder, type_alias.type_node);
        }
    }

    false
}
