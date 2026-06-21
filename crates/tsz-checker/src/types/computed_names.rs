//! Canonical owner for computed-property-name semantics.
//!
//! Computed property names are late-bound. tsc decides a computed key in one
//! place (`isLateBindableName`/`getLateBoundNameFromType`); tsz historically
//! grew several divergent copies of that decision split between the checker's
//! symbol-resolution layer (`CheckerState`) and the type-node lowering layer
//! (`TypeNodeChecker`). This module is the single implementation both layers
//! delegate to.
//!
//! The structural rules implemented here:
//!
//! - `[Symbol.<name>]` well-known keys require the `Symbol` base identifier to
//!   resolve to the global lib value. A same-named local shadow does not
//!   produce a well-known key (callers may instead recover a literal-type key
//!   from the shadowing binding).
//! - `[s]` binding-identity keys (`__unique_<symbol id>`) require `s` to
//!   resolve to a binding with unique-symbol identity: a `const` variable
//!   whose annotation is `unique symbol` or whose initializer is a verified
//!   global `Symbol(...)`/`Symbol.for(...)` call, or a `static readonly`
//!   class property typed `unique symbol`. Bindings annotated with the plain
//!   `symbol` type also receive a binding-identity key so element accesses
//!   can match members by `SymbolRef`.
//!
//! Layer-specific symbol *resolution* (which scope tables answer "what does
//! this identifier refer to") stays with the callers; the *policy* (what
//! counts as a symbol-valued key and which key text it produces) lives here.

use crate::context::CheckerContext;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::SymbolRef;

use super::property_access_type::known_globals::identifier_resolves_to_unshadowed_global_in_context;
use super::unique_symbol_arena::{
    has_declared_unique_symbol_owner, is_symbol_type_node,
    is_unique_symbol_type_annotation_unwrapped, unwrap_parenthesized_type,
};
use super::unique_symbol_construction::synthetic_unique_symbol_ref;

/// Canonical binding-identity key for a symbol-valued computed property name.
fn unique_symbol_binding_name(sym_id: SymbolId) -> String {
    format!("__unique_{}", sym_id.0)
}

/// Syntactic match of a well-known-symbol access expression.
pub(crate) struct WellKnownSymbolShape {
    /// The `Symbol` base identifier expression.
    base: NodeIndex,
    /// The canonical `[Symbol.<name>]` key when the accessed member is
    /// spelled as an identifier or a non-empty string literal.
    pub(crate) name: Option<String>,
}

/// Syntactic match of `<base ident>.<member>` / `<base ident>["<member>"]`
/// with parenthesized wrappers peeled.
struct MemberAccessParts {
    /// The base identifier expression.
    base: NodeIndex,
    /// The base identifier text.
    base_text: String,
    /// The member name when spelled as an identifier or a non-empty string
    /// literal.
    member: Option<String>,
}

fn member_access_parts(arena: &NodeArena, expr_idx: NodeIndex) -> Option<MemberAccessParts> {
    let mut current = expr_idx;
    while let Some(node) = arena.get(current)
        && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
    {
        current = arena.get_parenthesized(node)?.expression;
    }

    let node = arena.get(current)?;
    let (base, name) = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
    {
        let access = arena.get_access_expr(node)?;
        (access.expression, access.name_or_argument)
    } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
        let qualified = arena.get_qualified_name(node)?;
        (qualified.left, qualified.right)
    } else {
        return None;
    };

    let base_node = arena.get(base)?;
    let base_ident = arena.get_identifier(base_node)?;

    let name_node = arena.get(name)?;
    let member = if let Some(ident) = arena.get_identifier(name_node) {
        Some(ident.escaped_text.clone())
    } else if matches!(
        name_node.kind,
        k if k == SyntaxKind::StringLiteral as u16
            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
    ) && let Some(lit) = arena.get_literal(name_node)
        && !lit.text.is_empty()
    {
        Some(lit.text.clone())
    } else {
        None
    };

    Some(MemberAccessParts {
        base,
        base_text: base_ident.escaped_text.clone(),
        member,
    })
}

/// Pure shape match for `Symbol.<name>` / `Symbol["<name>"]` (with
/// parenthesized wrappers peeled). Performs no symbol resolution; callers
/// that have a binder must verify the base with
/// [`well_known_symbol_property_name`] before trusting the key.
pub(crate) fn well_known_symbol_access_shape(
    arena: &NodeArena,
    expr_idx: NodeIndex,
) -> Option<WellKnownSymbolShape> {
    let parts = member_access_parts(arena, expr_idx)?;
    if parts.base_text != "Symbol" {
        return None;
    }
    Some(WellKnownSymbolShape {
        base: parts.base,
        name: parts.member.map(|member| format!("[Symbol.{member}]")),
    })
}

/// Outcome of resolving a well-known-symbol access against the binder.
pub(crate) enum WellKnownSymbolName {
    /// The `Symbol` base resolves to the global lib value.
    Global(String),
    /// Shaped like `Symbol.<name>`, but `Symbol` is shadowed by a local
    /// binding. The caller may recover a literal-type key from the shadow.
    Shadowed,
}

/// Match and verify a well-known-symbol computed name expression.
pub(crate) fn well_known_symbol_property_name(
    ctx: &CheckerContext<'_>,
    arena: &NodeArena,
    binder: &BinderState,
    expr_idx: NodeIndex,
) -> Option<WellKnownSymbolName> {
    let shape = well_known_symbol_access_shape(arena, expr_idx)?;
    if identifier_resolves_to_unshadowed_global_in_context(ctx, arena, binder, shape.base, "Symbol")
    {
        shape.name.map(WellKnownSymbolName::Global)
    } else {
        Some(WellKnownSymbolName::Shadowed)
    }
}

/// Look up a symbol preferring the binder of its authoritative declaration
/// file.
///
/// Raw `SymbolId` values are per-binder-local: different binders in the same
/// compilation can assign the same `u32` to unrelated symbols. When
/// `resolve_symbol_file_index` has recorded an authoritative file for
/// `sym_id` that differs from the current checker file, return THAT file's
/// symbol rather than the current binder's colliding symbol.
pub(crate) fn symbol_from_any_context<'a>(
    ctx: &'a CheckerContext<'_>,
    sym_id: SymbolId,
) -> Option<&'a tsz_binder::Symbol> {
    let auth_file = ctx.resolve_symbol_file_index(sym_id);
    if let Some(file_idx) = auth_file
        && file_idx != ctx.current_file_idx
        && let Some(binder) = ctx.get_binder_for_file(file_idx)
        && let Some(sym) = binder.get_symbol(sym_id)
    {
        return Some(sym);
    }

    ctx.binder
        .get_symbol(sym_id)
        .or_else(|| {
            if let Some(file_idx) = auth_file
                && let Some(binder) = ctx.get_binder_for_file(file_idx)
                && let Some(sym) = binder.get_symbol(sym_id)
            {
                return Some(sym);
            }
            ctx.all_binders
                .as_ref()
                .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
        })
        .or_else(|| {
            ctx.lib_contexts
                .iter()
                .find_map(|lib_ctx| lib_ctx.binder.get_symbol(sym_id))
        })
}

/// Follow import-alias indirection to the aliased target symbol.
pub(crate) fn follow_import_aliases(ctx: &CheckerContext<'_>, mut sym_id: SymbolId) -> SymbolId {
    let mut hops = 0usize;
    while hops < 32 {
        hops += 1;
        let Some(next) = ctx.binder.resolve_import_symbol(sym_id) else {
            break;
        };
        if next == sym_id {
            break;
        }
        sym_id = next;
    }
    canonical_binding_symbol(ctx, sym_id)
}

/// Map a binding to the `SymbolId` assigned by the binder that OWNS its
/// declaration.
///
/// Cross-file resolution mints a per-binder copy of an imported binding: one
/// declaration acquires several distinct `SymbolId`s, one per importing file's
/// view. Re-export chains land on whichever copy `resolve_import_symbol` reached
/// (path-dependent), while a direct import lands on the declaring binder's own
/// id. Keying `__unique_<id>` on the copy makes a member reached through a
/// re-export chain (e.g. `Matcher.[matcher]`) mismatch the directly-imported
/// member of the *same* `const` in a fresh object literal — a false TS2353/
/// TS2561. Collapsing every copy onto the declaring binder's id gives one
/// stable identity per declaration regardless of import path, matching tsc's
/// single-symbol model. Idempotent for symbols already owned by their declaring
/// binder (the common case), so local bindings are unaffected.
fn canonical_binding_symbol(ctx: &CheckerContext<'_>, sym_id: SymbolId) -> SymbolId {
    let Some(symbol) = symbol_from_any_context(ctx, sym_id) else {
        return sym_id;
    };
    let owner_file = symbol.decl_file_idx;
    // A symbol owned by the file under check already carries its declaring
    // binder's id, so it is canonical and needs no remapping (the common case).
    if owner_file == u32::MAX || owner_file as usize == ctx.current_file_idx {
        return sym_id;
    }
    let Some(owner_binder) = ctx.get_binder_for_file(owner_file as usize) else {
        return sym_id;
    };
    let decl = if symbol.value_declaration.is_some() {
        symbol.value_declaration
    } else {
        symbol.primary_declaration().unwrap_or(NodeIndex::NONE)
    };
    if decl.is_none() {
        return sym_id;
    }
    // The binder maps a variable declaration's NAME identifier to the canonical
    // symbol; resolve through to it so the const declaration recovers the
    // owner-binder id (non-variable declarations are bound at the node itself).
    let owner_arena = ctx.get_arena_for_file(owner_file);
    let name_node = owner_arena
        .get(decl)
        .and_then(|node| owner_arena.get_variable_declaration(node))
        .and_then(|var_decl| var_decl.name.into_option())
        .unwrap_or(decl);
    owner_binder.get_node_symbol(name_node).unwrap_or(sym_id)
}

/// Run `pred` over every (owner binder, candidate arena, declaration) triple
/// of `sym_id`. Declarations bound from other files are inspected in their
/// owning binder's arenas; the current arena is only consulted when the
/// owner binder IS the current binder.
fn any_declaration_matches(
    ctx: &CheckerContext<'_>,
    sym_id: SymbolId,
    pred: impl Fn(&BinderState, &NodeArena, NodeIndex) -> bool,
) -> bool {
    let Some(symbol) = symbol_from_any_context(ctx, sym_id) else {
        return false;
    };
    let owner_binder = ctx
        .get_binder_for_file(symbol.decl_file_idx as usize)
        .unwrap_or(ctx.binder);

    // The declaration node lives in the *current* checker arena when the
    // symbol is declared in the file under check. Pointer-equality between
    // `owner_binder` (resolved via `get_binder_for_file`, an `Arc`-backed
    // binder from `all_binders`) and `ctx.binder` does not always hold even
    // for that file, so also admit `ctx.arena` whenever the symbol's
    // declaration file is the current file. Without this, a symbol whose
    // `declaration_arenas`/`symbol_arenas` maps were not populated (e.g. a
    // top-level `declare const s: unique symbol` consulted from a
    // `TypeNodeChecker` lowering path) had no candidate arena, so its
    // declaration was never inspected and `unique symbol` identity queries
    // silently returned `false`.
    let decl_is_current_file = symbol.decl_file_idx as usize == ctx.current_file_idx;
    // The arena that actually holds the symbol's declaration nodes. For a
    // cross-file import copy (`import * as ns` / a named import of the same
    // const reached from another file) the per-binder `declaration_arenas` /
    // `symbol_arenas` maps were populated only in the importing binder, not in
    // the binder of the file recorded as `decl_file_idx`; consulting the
    // owner file's arena directly recovers the original declaration
    // (e.g. the `Symbol.for(...)` initializer of a re-imported `unique symbol`
    // const) so identity queries do not silently return `false` when the
    // declaration is reached through an import alias.
    let owner_file_arena =
        (symbol.decl_file_idx != u32::MAX).then(|| ctx.get_arena_for_file(symbol.decl_file_idx));
    symbol.all_declarations().into_iter().any(|decl_idx| {
        let mut candidate_arenas: Vec<&NodeArena> = Vec::new();
        if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
        }
        if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
            candidate_arenas.push(symbol_arena.as_ref());
        }
        if let Some(arena) = owner_file_arena {
            candidate_arenas.push(arena);
        }
        if decl_is_current_file || std::ptr::eq(owner_binder, ctx.binder) {
            candidate_arenas.push(ctx.arena);
        }

        candidate_arenas
            .into_iter()
            .any(|arena| pred(owner_binder, arena, decl_idx))
    })
}

/// Does `sym_id` denote a binding with unique-symbol identity?
///
/// True for a `const` variable whose (paren-unwrapped) annotation is
/// `unique symbol` or whose initializer is a verified global
/// `Symbol(...)`/`Symbol.for(...)` call, and for a property declaration
/// typed `unique symbol` on an owner that can declare one (`static
/// readonly` class property or a const-bound object shape).
pub(crate) fn symbol_is_unique_symbol_binding(ctx: &CheckerContext<'_>, sym_id: SymbolId) -> bool {
    any_declaration_matches(ctx, sym_id, |owner_binder, arena, decl_idx| {
        declaration_is_unique_symbol_binding(ctx, owner_binder, arena, decl_idx)
    })
}

fn declaration_is_unique_symbol_binding(
    ctx: &CheckerContext<'_>,
    owner_binder: &BinderState,
    arena: &NodeArena,
    mut decl_idx: NodeIndex,
) -> bool {
    let Some(mut node) = arena.get(decl_idx) else {
        return false;
    };

    if node.kind == SyntaxKind::Identifier as u16 {
        let Some(parent_idx) = arena.get_extended(decl_idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(parent_node) = arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            || parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
        {
            decl_idx = parent_idx;
            node = parent_node;
        }
    }

    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
        let Some(var_decl) = arena.get_variable_declaration(node) else {
            return false;
        };
        if !arena.is_const_variable_declaration(decl_idx) {
            return false;
        }
        return (var_decl.type_annotation.is_some()
            && is_unique_symbol_type_annotation_unwrapped(arena, var_decl.type_annotation))
            || is_global_symbol_factory_call_initializer(
                ctx,
                owner_binder,
                arena,
                var_decl.initializer,
            );
    }

    if node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
        let Some(prop) = arena.get_property_decl(node) else {
            return false;
        };
        return prop.type_annotation.is_some()
            && is_unique_symbol_type_annotation_unwrapped(arena, prop.type_annotation)
            && has_declared_unique_symbol_owner(arena, prop.type_annotation);
    }

    false
}

/// Is `init_idx` a call to the global `Symbol` factory (`Symbol(...)` or
/// `Symbol.for(...)`), with the callee verified to resolve to the lib value?
fn is_global_symbol_factory_call_initializer(
    ctx: &CheckerContext<'_>,
    owner_binder: &BinderState,
    arena: &NodeArena,
    init_idx: NodeIndex,
) -> bool {
    let Some(node) = arena.get(init_idx) else {
        return false;
    };
    if node.kind != syntax_kind_ext::CALL_EXPRESSION {
        return false;
    }
    let Some(call) = arena.get_call_expr(node) else {
        return false;
    };
    is_global_symbol_factory_callee(ctx, owner_binder, arena, call.expression)
}

fn is_global_symbol_factory_callee(
    ctx: &CheckerContext<'_>,
    owner_binder: &BinderState,
    arena: &NodeArena,
    callee_idx: NodeIndex,
) -> bool {
    let Some(node) = arena.get(callee_idx) else {
        return false;
    };

    if let Some(ident) = arena.get_identifier(node) {
        if ident.escaped_text != "Symbol" {
            return false;
        }
        return owner_binder
            .resolve_identifier(arena, callee_idx)
            .or_else(|| owner_binder.file_locals.get("Symbol"))
            .or_else(|| {
                ctx.lib_contexts
                    .iter()
                    .find_map(|lib_ctx| lib_ctx.binder.file_locals.get("Symbol"))
            })
            .is_some_and(|sym_id| {
                ctx.symbol_is_from_actual_or_cloned_lib(sym_id) || ctx.symbol_is_from_lib(sym_id)
            });
    }

    if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
        return false;
    }
    let Some(access) = arena.get_access_expr(node) else {
        return false;
    };
    arena
        .get_identifier_text(access.name_or_argument)
        .is_some_and(|name| name == "for")
        && is_global_symbol_factory_callee(ctx, owner_binder, arena, access.expression)
}

/// Is this declaration a variable or parameter with an explicit plain
/// `: symbol` annotation (not `: unique symbol`)?
///
/// These "non-unique symbol" bindings produce computed property keys stored
/// as `__unique_<id>` so that element-access evaluation can match them by
/// binding identity, mirroring how TypeScript resolves `ws[sym]` when `sym`
/// is typed as the general `symbol` type.
fn declaration_has_nonunique_symbol_annotation(arena: &NodeArena, decl_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(decl_idx) else {
        return false;
    };

    let type_annotation = if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
        let Some(var_decl) = arena.get_variable_declaration(node) else {
            return false;
        };
        var_decl.type_annotation
    } else if node.kind == syntax_kind_ext::PARAMETER {
        let Some(param) = arena.get_parameter(node) else {
            return false;
        };
        param.type_annotation
    } else {
        return false;
    };

    if !type_annotation.is_some() {
        return false;
    }

    // `: symbol` parses as SyntaxKind::SymbolKeyword (a keyword type node),
    // which is never unique symbol. `: unique symbol` parses as
    // TYPE_OPERATOR + SymbolKeyword.
    if let Some(ann_node) = arena.get(type_annotation)
        && ann_node.kind == SyntaxKind::SymbolKeyword as u16
    {
        return true;
    }

    // Fallback: handle `symbol` written as a TypeReference (rare but possible
    // in some AST forms); exclude `unique symbol` so the unique-symbol path
    // stays intact.
    is_symbol_type_node(arena, type_annotation)
        && !is_unique_symbol_type_annotation_unwrapped(arena, type_annotation)
}

fn declaration_has_unique_symbol_member(
    arena: &NodeArena,
    decl_idx: NodeIndex,
    member_name: &str,
) -> bool {
    let Some(mut node) = arena.get(decl_idx) else {
        return false;
    };
    if node.kind == SyntaxKind::Identifier as u16 {
        let Some(parent_idx) = arena.get_extended(decl_idx).map(|ext| ext.parent) else {
            return false;
        };
        let Some(parent_node) = arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            node = parent_node;
        }
    }
    if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
        return false;
    }
    let Some(var_decl) = arena.get_variable_declaration(node) else {
        return false;
    };
    if !var_decl.type_annotation.is_some() {
        return false;
    }
    let type_node_idx = unwrap_parenthesized_type(arena, var_decl.type_annotation);
    let Some(type_node) = arena.get(type_node_idx) else {
        return false;
    };
    if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
        return false;
    }
    let Some(type_lit) = arena.get_type_literal(type_node) else {
        return false;
    };
    type_lit.members.nodes.iter().any(|&member_idx| {
        let Some(member_node) = arena.get(member_idx) else {
            return false;
        };
        if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            return false;
        }
        let Some(sig) = arena.get_signature(member_node) else {
            return false;
        };
        super::queries::core::get_literal_property_name(arena, sig.name).is_some_and(|name| {
            name == member_name
                && sig.type_annotation.is_some()
                && is_unique_symbol_type_annotation_unwrapped(arena, sig.type_annotation)
        })
    })
}

fn declaration_unique_symbol_member_ref(
    arena: &NodeArena,
    decl_idx: NodeIndex,
    member_name: &str,
    file_name: &str,
) -> Option<SymbolRef> {
    let mut node = arena.get(decl_idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        let parent_idx = arena.get_extended(decl_idx).map(|ext| ext.parent)?;
        let parent_node = arena.get(parent_idx)?;
        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            node = parent_node;
        }
    }
    if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
        return None;
    }
    let var_decl = arena.get_variable_declaration(node)?;
    if !var_decl.type_annotation.is_some() {
        return None;
    }
    let type_node_idx = unwrap_parenthesized_type(arena, var_decl.type_annotation);
    let type_node = arena.get(type_node_idx)?;
    if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
        return None;
    }
    let type_lit = arena.get_type_literal(type_node)?;
    for &member_idx in type_lit.members.nodes.iter() {
        let Some(member_node) = arena.get(member_idx) else {
            continue;
        };
        if member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
            continue;
        }
        let Some(sig) = arena.get_signature(member_node) else {
            continue;
        };
        let Some(name) = super::queries::core::get_literal_property_name(arena, sig.name) else {
            continue;
        };
        if name != member_name || !sig.type_annotation.is_some() {
            continue;
        }
        let annotation = unwrap_parenthesized_type(arena, sig.type_annotation);
        if !is_unique_symbol_type_annotation_unwrapped(arena, sig.type_annotation) {
            continue;
        }
        let annotation_node = arena.get(annotation)?;
        return Some(synthetic_unique_symbol_ref(
            file_name,
            annotation_node.pos,
            annotation_node.end,
        ));
    }
    None
}

/// Resolve `sym_id` (after import-alias hops) to a `SymbolRef` strictly when
/// it has unique-symbol identity. Used where a `unique symbol` TYPE is
/// constructed, so plain `: symbol` binding-identity keys must not qualify.
pub(crate) fn unique_symbol_property_ref(
    ctx: &CheckerContext<'_>,
    sym_id: SymbolId,
) -> Option<SymbolRef> {
    let sym_id = follow_import_aliases(ctx, sym_id);
    symbol_is_unique_symbol_binding(ctx, sym_id).then_some(SymbolRef(sym_id.0))
}

/// The binding-identity key atom for `sym_id` (after import-alias hops) when
/// it is unique-symbol or plain-`symbol` valued; `None` otherwise.
pub(crate) fn symbol_binding_property_atom(
    ctx: &CheckerContext<'_>,
    sym_id: SymbolId,
) -> Option<Atom> {
    let sym_id = follow_import_aliases(ctx, sym_id);
    any_declaration_matches(ctx, sym_id, |owner_binder, arena, decl_idx| {
        declaration_is_unique_symbol_binding(ctx, owner_binder, arena, decl_idx)
            || declaration_has_nonunique_symbol_annotation(arena, decl_idx)
    })
    .then(|| ctx.types.intern_string(&unique_symbol_binding_name(sym_id)))
}

/// Canonical computed-property-name key for `expr_idx` in the current file:
/// a verified `[Symbol.<name>]` well-known key, a declared unique-symbol
/// member key (`[<base>.<member>]` where the base variable's type-literal
/// annotation declares `<member>: unique symbol`), or the binding-identity
/// key of the symbol `resolve_symbol` finds for the expression.
///
/// `resolve_symbol` stays caller-supplied because the symbol-resolution layer
/// and the lowering layer consult different scope tables; everything after
/// resolution is shared policy.
pub(crate) fn computed_property_name_atom(
    ctx: &CheckerContext<'_>,
    resolve_symbol: impl Fn(NodeIndex) -> Option<SymbolId>,
    expr_idx: NodeIndex,
) -> Option<Atom> {
    if let Some(parts) = member_access_parts(ctx.arena, expr_idx)
        && let Some(member) = parts.member.as_deref()
    {
        if parts.base_text == "Symbol"
            && identifier_resolves_to_unshadowed_global_in_context(
                ctx, ctx.arena, ctx.binder, parts.base, "Symbol",
            )
        {
            return Some(ctx.types.intern_string(&format!("[Symbol.{member}]")));
        }
        // The base is shadowed or not `Symbol` at all: late-bind through a
        // declared unique-symbol member, keeping the source-spelled key.
        if let Some(base_sym) = resolve_symbol(parts.base) {
            let base_sym = follow_import_aliases(ctx, base_sym);
            if any_declaration_matches(ctx, base_sym, |_owner_binder, arena, decl_idx| {
                declaration_has_unique_symbol_member(arena, decl_idx, member)
            }) {
                return Some(
                    ctx.types
                        .intern_string(&format!("[{}.{member}]", parts.base_text)),
                );
            }
        }
    }
    symbol_binding_property_atom(ctx, resolve_symbol(expr_idx)?)
}

/// Recover the source-spelled key and precise `unique symbol` identity for a
/// member access whose base declaration is an object type literal containing a
/// readonly `unique symbol` member, such as `[Tags.tag]` where
/// `declare const Tags: { readonly tag: unique symbol }`.
pub(crate) fn declared_unique_symbol_member_ref_for_expr(
    ctx: &CheckerContext<'_>,
    resolve_symbol: impl Fn(NodeIndex) -> Option<SymbolId>,
    expr_idx: NodeIndex,
) -> Option<(String, SymbolRef)> {
    let parts = member_access_parts(ctx.arena, expr_idx)?;
    let member = parts.member.as_deref()?;
    let base_sym = ctx
        .binder
        .file_locals
        .get(&parts.base_text)
        .or_else(|| resolve_symbol(parts.base))?;
    let base_sym = follow_import_aliases(ctx, base_sym);
    let symbol = symbol_from_any_context(ctx, base_sym)?;
    let mut declarations = symbol.all_declarations();
    if symbol.value_declaration.is_some() && !declarations.contains(&symbol.value_declaration) {
        declarations.push(symbol.value_declaration);
    }
    if let Some(primary) = symbol.primary_declaration()
        && !declarations.contains(&primary)
    {
        declarations.push(primary);
    }
    for decl_idx in declarations {
        if let Some(symbol_ref) =
            declaration_unique_symbol_member_ref(ctx.arena, decl_idx, member, &ctx.file_name)
        {
            return Some((format!("[{}.{member}]", parts.base_text), symbol_ref));
        }
    }
    if let Some(symbol_ref) =
        current_file_declared_unique_symbol_member_ref(ctx, &parts.base_text, member)
    {
        return Some((format!("[{}.{member}]", parts.base_text), symbol_ref));
    }
    None
}

fn current_file_declared_unique_symbol_member_ref(
    ctx: &CheckerContext<'_>,
    base_name: &str,
    member_name: &str,
) -> Option<SymbolRef> {
    let source_file = ctx.arena.source_files.first()?;
    for &stmt_idx in source_file.statements.nodes.iter() {
        let Some(stmt_node) = ctx.arena.get(stmt_idx) else {
            continue;
        };
        let Some(var_data) = ctx.arena.get_variable(stmt_node) else {
            continue;
        };
        for &decl_idx in var_data.declarations.nodes.iter() {
            let Some(var_decl) = ctx
                .arena
                .get(decl_idx)
                .and_then(|node| ctx.arena.get_variable_declaration(node))
            else {
                continue;
            };
            let Some(name_node) = ctx.arena.get(var_decl.name) else {
                continue;
            };
            if !ctx
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == base_name)
            {
                continue;
            }
            if let Some(symbol_ref) = declaration_unique_symbol_member_ref(
                ctx.arena,
                decl_idx,
                member_name,
                &ctx.file_name,
            ) {
                return Some(symbol_ref);
            }
        }
    }
    None
}

/// Is the computed-name expression symbol-valued (well-known or
/// binding-identity keyed)? Same legs as [`computed_property_name_atom`].
pub(crate) fn computed_property_is_symbol_named(
    ctx: &CheckerContext<'_>,
    resolve_symbol: impl Fn(NodeIndex) -> Option<SymbolId>,
    expr_idx: NodeIndex,
) -> bool {
    computed_property_name_atom(ctx, resolve_symbol, expr_idx).is_some()
}
