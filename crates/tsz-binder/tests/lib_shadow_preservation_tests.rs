//! Tests for the shape of the per-file shadow symbol when a module-local
//! declaration shadows a lib global.
//!
//! See `crates/tsz-binder/src/nodes/binding.rs` →
//! `BinderState::collect_preserved_lib_meaning`.
//!
//! Issue #4687 (regression follow-up to PR #4634): when a VALUE-only
//! local (e.g. `const X = 1`, `declare const X: unique symbol`) shadows
//! a lib symbol that contributes a TYPE meaning (e.g. lib's
//! `type Readonly<T>`, `interface Array<T>`), the shadow symbol must
//! inherit the lib's TYPE *flag* but must **not** carry the lib's
//! INTERFACE / `TYPE_ALIAS` declaration node onto its own `declarations`
//! / `declaration_arenas` table. Polluting the shadow's declarations
//! with lib type-alias bodies makes downstream type traversal walk the
//! lib's mapped-type machinery as if it belonged to the user's symbol,
//! conflating independent type evaluations like
//! `Static<typeof Input>` vs `Static<typeof Output>` in TypeBox-style
//! fixtures.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::symbol_flags;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::{ParserState, syntax_kind_ext};

struct BoundProgram {
    binder: BinderState,
    /// Owned copy of the user parser's arena, kept alive so declaration
    /// lookups using `NodeIndex` remain valid after the parser is dropped.
    user_arena: Arc<NodeArena>,
    #[allow(dead_code)]
    lib: Arc<LibFile>,
}

fn bind_user_with_lib(user_source: &str, lib_source: &str) -> BoundProgram {
    let lib = Arc::new(LibFile::from_source(
        "lib.es5.d.ts".to_string(),
        lib_source.to_string(),
    ));
    let mut user_parser = ParserState::new("user.ts".to_string(), user_source.to_string());
    let user_root = user_parser.parse_source_file();
    let mut user_binder = BinderState::new();
    user_binder.bind_source_file_with_libs(user_parser.get_arena(), user_root, &[Arc::clone(&lib)]);
    let user_arena = Arc::new(user_parser.get_arena().clone());
    BoundProgram {
        binder: user_binder,
        user_arena,
        lib,
    }
}

/// Resolve `(sym_id, decl)` to the AST node kind by probing the
/// `declaration_arenas` map first, then falling back to the symbol's
/// own arena slot, then to the user's main arena.
fn declaration_kind(
    prog: &BoundProgram,
    sym_id: tsz_binder::SymbolId,
    decl: tsz_parser::NodeIndex,
) -> Option<u16> {
    let mut arenas: Vec<&NodeArena> = Vec::new();
    if let Some(arc_arenas) = prog.binder.declaration_arenas.get(&(sym_id, decl)) {
        arenas.extend(arc_arenas.iter().map(std::convert::AsRef::as_ref));
    }
    if let Some(symbol_arena) = prog.binder.symbol_arenas.get(&sym_id) {
        arenas.push(symbol_arena.as_ref());
    }
    arenas.push(prog.user_arena.as_ref());
    arenas
        .into_iter()
        .find_map(|arena| arena.get(decl).map(|n| n.kind))
}

fn shadow_symbol_for<'a>(
    prog: &'a BoundProgram,
    name: &str,
) -> (tsz_binder::SymbolId, &'a tsz_binder::Symbol) {
    let sym_id = prog
        .binder
        .file_locals
        .get(name)
        .unwrap_or_else(|| panic!("user file_locals must contain shadow symbol for `{name}`"));
    let sym = prog
        .binder
        .symbols
        .get(sym_id)
        .expect("shadow symbol must exist in symbol arena");
    (sym_id, sym)
}

#[test]
fn value_only_const_shadowing_lib_type_alias_preserves_type_flag_only() {
    // The user's `const Readonly = 1` (VALUE-only) shadows lib's
    // `type Readonly<T>` (TYPE-only). The shadow symbol must:
    //   - have BOTH the local's VALUE flags (variable) and the lib's TYPE flag
    //     so type lookups for `Readonly<...>` still see a TYPE-bearing symbol;
    //   - NOT carry the lib's TYPE_ALIAS_DECLARATION on its `declarations` /
    //     `declaration_arenas`. Polluting these conflates independent
    //     `Static<typeof X>`-style evaluations (issue #4687).
    let prog = bind_user_with_lib(
        "export {};\nconst Readonly = 1;\n",
        "type Readonly<T> = { readonly [P in keyof T]: T[P] };",
    );
    let (sym_id, sym) = shadow_symbol_for(&prog, "Readonly");

    assert!(
        (sym.flags & symbol_flags::VALUE) != 0,
        "shadow symbol must keep the local VALUE flag; got flags={:08x}",
        sym.flags
    );
    assert!(
        (sym.flags & symbol_flags::TYPE) != 0,
        "shadow symbol must inherit the lib TYPE flag so `Readonly<...>` \
         still resolves as a type; got flags={:08x}",
        sym.flags
    );

    for &decl in &sym.declarations {
        let kind = declaration_kind(&prog, sym_id, decl)
            .expect("declaration must be resolvable in some arena");
        assert_ne!(
            kind,
            syntax_kind_ext::TYPE_ALIAS_DECLARATION,
            "shadow symbol must NOT carry the lib's TYPE_ALIAS_DECLARATION \
             onto its `declarations` (regression for issue #4687); decl={decl:?}"
        );
        assert_ne!(
            kind,
            syntax_kind_ext::INTERFACE_DECLARATION,
            "shadow symbol must NOT carry the lib's INTERFACE_DECLARATION \
             onto its `declarations` for VALUE-only locals; decl={decl:?}"
        );
    }
}

#[test]
fn value_only_const_shadowing_lib_interface_preserves_type_flag_only() {
    // Same shape as above, but with `interface Array<T>` (lib uses
    // INTERFACE_DECLARATION rather than TYPE_ALIAS_DECLARATION). The
    // shadow must still keep the TYPE flag so `Array<number>` resolves,
    // while not carrying the lib's interface declaration on its own
    // `declarations` table.
    let prog = bind_user_with_lib(
        "export {};\nconst Array = 1;\n",
        "interface Array<T> { length: number; }",
    );
    let (sym_id, sym) = shadow_symbol_for(&prog, "Array");

    assert!(
        (sym.flags & symbol_flags::VALUE) != 0,
        "shadow symbol must keep the local VALUE flag; got flags={:08x}",
        sym.flags
    );
    assert!(
        (sym.flags & symbol_flags::TYPE) != 0,
        "shadow symbol must inherit the lib TYPE flag so `Array<...>` \
         still resolves as a type; got flags={:08x}",
        sym.flags
    );

    for &decl in &sym.declarations {
        let kind = declaration_kind(&prog, sym_id, decl)
            .expect("declaration must be resolvable in some arena");
        assert_ne!(
            kind,
            syntax_kind_ext::INTERFACE_DECLARATION,
            "shadow symbol must NOT carry the lib's INTERFACE_DECLARATION \
             onto its `declarations` (regression for issue #4687); decl={decl:?}"
        );
        assert_ne!(
            kind,
            syntax_kind_ext::TYPE_ALIAS_DECLARATION,
            "shadow symbol must NOT carry a TYPE_ALIAS_DECLARATION onto \
             `declarations` for a VALUE-only local; decl={decl:?}"
        );
    }
}

#[test]
fn unique_symbol_shadowing_lib_type_alias_preserves_type_flag_only() {
    // The exact scenario from issue #4687: a `unique symbol` const is a
    // VALUE-only declaration and shadows lib's `type Readonly<T>`. With
    // the buggy preservation, the lib's TYPE_ALIAS body (with its
    // `[P in keyof T]: T[P]` mapped type) would attach to the shadow,
    // and `Static<typeof X>`-style traversal walks that mapped body as
    // if it were part of the user's symbol — conflating Input/Output.
    let prog = bind_user_with_lib(
        "export {};\nexport declare const Readonly: unique symbol;\n",
        "type Readonly<T> = { readonly [P in keyof T]: T[P] };",
    );
    let (sym_id, sym) = shadow_symbol_for(&prog, "Readonly");

    assert!(
        (sym.flags & symbol_flags::TYPE) != 0,
        "shadow symbol must inherit the lib TYPE flag; got flags={:08x}",
        sym.flags
    );

    for &decl in &sym.declarations {
        let kind = declaration_kind(&prog, sym_id, decl)
            .expect("declaration must be resolvable in some arena");
        assert_ne!(
            kind,
            syntax_kind_ext::TYPE_ALIAS_DECLARATION,
            "unique-symbol shadow must NOT carry the lib's TYPE_ALIAS \
             body (regression for issue #4687); decl={decl:?}"
        );
    }
}

#[test]
fn type_only_interface_shadowing_lib_var_still_preserves_value_decl() {
    // Inverse direction: the local `interface Symbol {}` is TYPE-only.
    // The lib has `var Symbol: { iterator: symbol }` (VALUE). The
    // shadow must preserve the lib's VALUE declaration so
    // `Symbol.iterator` still resolves. `var` declarations do NOT
    // drive the computed-key / mapped-type traversal that motivated
    // the type-side fix, so this preservation remains safe.
    let prog = bind_user_with_lib(
        "export {};\ninterface Symbol {}\n",
        "interface Symbol { description: string }\nvar Symbol: { iterator: symbol };",
    );
    let (sym_id, sym) = shadow_symbol_for(&prog, "Symbol");

    assert!(
        (sym.flags & symbol_flags::TYPE) != 0,
        "shadow symbol must keep TYPE flag from local interface; got flags={:08x}",
        sym.flags
    );
    assert!(
        (sym.flags & symbol_flags::VALUE) != 0,
        "shadow symbol must inherit the lib VALUE flag so `Symbol.iterator` \
         still resolves; got flags={:08x}",
        sym.flags
    );

    let saw_variable_decl = sym.declarations.iter().any(|&decl| {
        declaration_kind(&prog, sym_id, decl)
            .is_some_and(|kind| kind == syntax_kind_ext::VARIABLE_DECLARATION)
    });
    assert!(
        saw_variable_decl,
        "TYPE-only local shadowing a lib `var` must still preserve the \
         lib's VARIABLE_DECLARATION on the shadow's declarations so \
         property access works"
    );
}
