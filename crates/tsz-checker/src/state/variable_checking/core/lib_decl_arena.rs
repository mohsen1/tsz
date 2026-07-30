//! Cross-arena declaration ownership guard for the TS2403 lib-global lookup.
//!
//! A merged binder's symbol carries declaration `NodeIndex` values from *every*
//! file that contributed to it. `NodeIndex` is an arena-local offset, so reading
//! one against a different arena silently yields an unrelated node rather than
//! failing. The TS2403 prior-declaration scan walks every `LibContext` and, for
//! each one, materializes `lib_sym.declarations` against *that* context's arena
//! — so a `Symbol` declaration that really lives in `lib.es2015.symbol.d.ts`
//! got materialized against `lib.dom.d.ts` and produced the type of whatever
//! member happened to sit at that index (`blur`, `parseInt`, `CSSImportRule`).
//!
//! That value is not only rendered in the message, it also gates emission
//! through `are_var_decl_types_compatible`, so redeclaring a lib global with its
//! own correct type (`declare var Symbol: SymbolConstructor;`, the ordinary
//! ambient-shim shape) reported a false TS2403.
//!
//! The intra-file arm of the same scan already guards this way by comparing the
//! declaration's name text; these helpers give the lib arm the same guard plus
//! the binder's recorded per-declaration arena provenance.

use crate::context::CheckerContext;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// True when `decl_idx` is a declaration of `name` that genuinely lives in
/// `arena`, and is therefore safe to materialize through a child checker built
/// over `arena`.
///
/// Two independent checks, both required:
///
/// 1. **Recorded provenance.** When the binder recorded arenas for this
///    `(symbol, declaration)` pair, `arena` must be one of them.
/// 2. **Name agreement.** The node at `decl_idx` *in `arena`* must be a
///    declaration whose name is `name`. This is the check that survives
///    `NodeIndex` collisions, where `declaration_arenas` legitimately records
///    several arenas for one index because two files declare something at the
///    same offset.
pub(super) fn lib_declaration_belongs_to_arena(
    binder: &BinderState,
    arena: &NodeArena,
    lib_sym_id: SymbolId,
    decl_idx: NodeIndex,
    name: &str,
) -> bool {
    if let Some(arenas) = binder.declaration_arenas.get(&(lib_sym_id, decl_idx))
        && !arenas
            .iter()
            .any(|recorded| std::ptr::eq(recorded.as_ref(), arena))
    {
        return false;
    }

    declaration_name_text_in_arena(arena, decl_idx).is_some_and(|text| text == name)
}

/// Declared type of a lib global whose annotation is a bare type reference
/// (`declare var Symbol: SymbolConstructor;`), resolved through the *parent*
/// checker's canonical, name-verified lib def query.
///
/// The alternative — running a child `CheckerState` over the lib arena and
/// asking it for the declaration's type — lowers that annotation through the
/// raw `SymbolId -> DefId` map. Every lib binder's symbols carry the `u32::MAX`
/// declaration-file sentinel, so that map is first-writer-wins across lib
/// binders and a per-lib `SymbolId` resolves to whichever def another lib binder
/// registered at the same raw index. `SymbolConstructor` came back as `blur`
/// (lib.dom) or `parseInt` (lib.es5) depending only on the lib set — the
/// `SymbolId`-reinterpreted-as-`DefId` collision family.
///
/// `actual_lib_def_id_for_bare_name` is the hardened query the rest of the
/// checker already uses for exactly this: it canonicalizes the symbol and then
/// verifies the elected def's recorded name against the requested name, so a
/// collided identity can never be committed.
///
/// Returns `None` for anything that is not a bare, non-generic type reference;
/// the caller keeps its existing materialization for those.
pub(super) fn lib_global_annotation_type(
    ctx: &CheckerContext<'_>,
    arena: &NodeArena,
    decl_idx: NodeIndex,
) -> Option<TypeId> {
    let node = arena.get(decl_idx)?;
    if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
        return None;
    }
    let annotation_idx = arena.get_variable_declaration(node)?.type_annotation;
    if annotation_idx.is_none() {
        return None;
    }
    let annotation = arena.get(annotation_idx)?;
    if annotation.kind != syntax_kind_ext::TYPE_REFERENCE {
        return None;
    }
    let type_ref = arena.get_type_ref(annotation)?;
    if type_ref.type_arguments.is_some() {
        return None;
    }
    let annotation_name = arena.get_identifier_text(type_ref.type_name)?;
    let def_id = ctx.actual_lib_def_id_for_bare_name(annotation_name)?;
    Some(ctx.types.lazy(def_id))
}

/// Name text of the declaration at `decl_idx`, read from `arena` rather than
/// from the checker's own arena.
///
/// Covers the declaration kinds a value global can take (`var`/`function`/
/// `class`/`enum`/`namespace`, plus parameters, which the same scan also
/// compares against). Anything else has no bare name to verify and is treated
/// as unverifiable.
fn declaration_name_text_in_arena(arena: &NodeArena, decl_idx: NodeIndex) -> Option<String> {
    let node = arena.get(decl_idx)?;
    let name_idx = match node.kind {
        k if k == tsz_scanner::SyntaxKind::Identifier as u16 => decl_idx,
        syntax_kind_ext::VARIABLE_DECLARATION => arena.get_variable_declaration(node)?.name,
        syntax_kind_ext::FUNCTION_DECLARATION => arena.get_function(node)?.name,
        syntax_kind_ext::CLASS_DECLARATION => arena.get_class(node)?.name,
        syntax_kind_ext::ENUM_DECLARATION => arena.get_enum(node)?.name,
        syntax_kind_ext::MODULE_DECLARATION => arena.get_module(node)?.name,
        syntax_kind_ext::PARAMETER => arena.get_parameter(node)?.name,
        _ => return None,
    };
    let name_node = arena.get(name_idx)?;
    if let Some(ident) = arena.get_identifier(name_node) {
        return Some(ident.escaped_text.to_string());
    }
    arena.get_literal(name_node).map(|lit| lit.text.clone())
}
