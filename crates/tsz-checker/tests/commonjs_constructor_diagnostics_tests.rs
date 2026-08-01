//! Cross-file CommonJS `new` semantics for plain JS functions.
//!
//! TypeScript 7 dropped the `isJSConstructor` inference: a plain JS function
//! never acquires a construct signature from `this.x = ...` assignments,
//! `F.prototype` assignments, or a JSDoc `@constructor` tag. Under
//! `noImplicitAny`, `new F()` is TS7009 and the result is `any`, so element and
//! property reads off the result are silent. Only a real `class` declaration
//! keeps a construct signature.
//!
//! These tests pin that the CommonJS export surface agrees with the same-file
//! `new` path. Every expectation here was taken from `tsc` 7.0.2 with
//! `allowJs`/`checkJs`/`strict` before the fix was written.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check `b.js` against a two-file CommonJS program where `b.js` does
/// `require("./a.js")`. Returns every diagnostic reported for `b.js`.
fn check_commonjs_pair(
    a_source: &str,
    b_source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new("a.js".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.js".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    let file_a_exports = binder_a.module_exports.get("a.js").cloned();
    if let Some(exports) = &file_a_exports {
        Arc::make_mut(&mut binder_b.module_exports).insert("./a.js".to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_a_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "b.js".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./a.js".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./a.js".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn no_lib_options() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        no_lib: true,
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    }
}

/// The two-file harness runs without lib files, so every case reports the same
/// fixed set of TS2318 "Cannot find global type" diagnostics. They are a
/// property of the harness, not of the code under test, so they are dropped
/// before asserting.
fn codes(diagnostics: &[(u32, String)]) -> Vec<u32> {
    diagnostics
        .iter()
        .map(|(code, _)| *code)
        .filter(|code| *code != 2318)
        .collect()
}

/// `tsc` 7.0.2 on `conformance/salsa/lateBoundAssignmentDeclarationSupport4.ts`:
/// the `new x.F()` in the importing file is TS7009 and nothing else. The
/// late-bound `F.prototype[sym]` declaration is "currently unsupported", so the
/// instance is `any` and `inst[x.S]` is silent.
#[test]
fn test_commonjs_new_on_prototype_expando_function_is_ts7009_only() {
    let a_source = r#"
const s = Symbol();
function F() {}
F.prototype[s] = "ok";
module.exports.F = F;
module.exports.S = s;
"#;
    let b_source = r#"
const x = require("./a.js");
const inst = new x.F();
inst[x.S];
"#;

    let diagnostics = check_commonjs_pair(a_source, b_source, no_lib_options());

    assert_eq!(
        codes(&diagnostics),
        vec![7009],
        "Expected exactly TS7009 for the cross-file `new` on a plain JS function, got: {diagnostics:#?}"
    );
}

/// Renamed-binder control for the case above: nothing about the fix may depend
/// on the exported function being called `F`.
#[test]
fn test_commonjs_new_on_prototype_expando_function_is_ts7009_only_renamed_binder() {
    let a_source = r#"
const marker = Symbol();
function WidgetFactory() {}
WidgetFactory.prototype[marker] = "ok";
module.exports.WidgetFactory = WidgetFactory;
module.exports.Marker = marker;
"#;
    let b_source = r#"
const mod = require("./a.js");
const made = new mod.WidgetFactory();
made[mod.Marker];
"#;

    let diagnostics = check_commonjs_pair(a_source, b_source, no_lib_options());

    assert_eq!(
        codes(&diagnostics),
        vec![7009],
        "Renamed binders must behave identically, got: {diagnostics:#?}"
    );
}

/// The `this.prop = value` form is the original `isJSConstructor` shape and is
/// equally not a constructor in TypeScript 7: `new x.F()` is TS7009, and the
/// `any` result silences both the property read and the element read.
#[test]
fn test_commonjs_new_on_this_assignment_function_is_ts7009_only() {
    let a_source = r#"
function F() {
  this.a = 1;
}
module.exports.F = F;
"#;
    let b_source = r#"
const x = require("./a.js");
const inst = new x.F();
inst.a;
inst["nope"];
inst.alsoNope;
"#;

    let diagnostics = check_commonjs_pair(a_source, b_source, no_lib_options());

    assert_eq!(
        codes(&diagnostics),
        vec![7009],
        "An `any` result must silence every downstream read, got: {diagnostics:#?}"
    );
}

/// A JSDoc `@constructor` tag does not resurrect the dropped inference either —
/// `tsc` 7.0.2 still reports TS7009 for `new x.G()`.
#[test]
fn test_commonjs_new_on_jsdoc_constructor_function_is_ts7009_only() {
    let a_source = r#"
/** @constructor */
function G() {
  this.a = 1;
}
module.exports.G = G;
"#;
    let b_source = r#"
const x = require("./a.js");
const g = new x.G();
g["nope"];
"#;

    let diagnostics = check_commonjs_pair(a_source, b_source, no_lib_options());

    assert_eq!(
        codes(&diagnostics),
        vec![7009],
        "A JSDoc @constructor tag must not add a construct signature, got: {diagnostics:#?}"
    );
}

/// The bare `module.exports = function ...` export form takes the same path as
/// the named `module.exports.F = F` form and must reach the same answer.
#[test]
fn test_commonjs_bare_export_assignment_new_is_ts7009_only() {
    let a_source = r#"
module.exports = function Boxed() {
  this.a = 1;
};
"#;
    let b_source = r#"
const Boxed = require("./a.js");
const b = new Boxed();
b["nope"];
"#;

    let diagnostics = check_commonjs_pair(a_source, b_source, no_lib_options());

    assert_eq!(
        codes(&diagnostics),
        vec![7009],
        "The bare export-assignment form must agree with the named form, got: {diagnostics:#?}"
    );
}

/// Negative control: a real `class` still carries its construct signature
/// across the CommonJS export surface. `new x.K()` is clean, its declared
/// members resolve, and an undeclared member still errors — so the fix removes
/// the synthesized construct signature without weakening the genuine one.
#[test]
fn test_commonjs_new_on_exported_class_still_constructs() {
    let a_source = r#"
class K {
  constructor() {
    this.b = 2;
  }
}
module.exports.K = K;
"#;
    let b_source = r#"
const x = require("./a.js");
const k = new x.K();
k.b;
"#;

    let diagnostics = check_commonjs_pair(a_source, b_source, no_lib_options());

    assert!(
        codes(&diagnostics).is_empty(),
        "An exported class must keep its construct signature, got: {diagnostics:#?}"
    );
}

/// Negative control, error side: the exported class's instance type must still
/// be a real type, not `any` — an unknown member is TS2339.
#[test]
fn test_commonjs_new_on_exported_class_still_reports_unknown_members() {
    let a_source = r#"
class K {
  constructor() {
    this.b = 2;
  }
}
module.exports.K = K;
"#;
    let b_source = r#"
const x = require("./a.js");
const k = new x.K();
k.missingMember;
"#;

    let diagnostics = check_commonjs_pair(a_source, b_source, no_lib_options());

    assert_eq!(
        codes(&diagnostics),
        vec![2339],
        "An exported class instance must stay concrete, got: {diagnostics:#?}"
    );
}

/// Calling the exported plain function *without* `new` is unaffected: it still
/// has its call signature and reports nothing.
#[test]
fn test_commonjs_plain_call_of_exported_function_is_unaffected() {
    let a_source = r#"
function F() {}
F.prototype.m = function () {};
module.exports.F = F;
"#;
    let b_source = r#"
const x = require("./a.js");
x.F();
"#;

    let diagnostics = check_commonjs_pair(a_source, b_source, no_lib_options());

    assert!(
        codes(&diagnostics).is_empty(),
        "A plain call must keep working, got: {diagnostics:#?}"
    );
}

/// `tsc` 7.0.2 on `conformance/salsa/lateBoundAssignmentDeclarationSupport2.ts`:
/// indexing the imported CommonJS *namespace* with its own exported unique
/// symbol is genuinely TS7053. This is the positive control for the family —
/// the namespace index check must not be weakened by removing the synthesized
/// construct signature.
#[test]
fn test_commonjs_exported_unique_symbol_stays_concrete_for_namespace_index_errors() {
    let a_source = r#"
const s = Symbol();
const str = "my-fake-sym";
module.exports[s] = "ok";
module.exports[str] = "ok";
module.exports.S = s;
"#;
    let b_source = r#"
const x = require("./a.js");
x[x.S];
"#;

    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        target: tsz_common::common::ScriptTarget::ES2015,
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    };
    let diagnostics = check_commonjs_pair(a_source, b_source, options);

    let ts7053 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7053)
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ts7053.len(),
        1,
        "Expected one TS7053 for indexing the imported CommonJS namespace with its exported unique symbol, got: {ts7053:#?}"
    );
    assert!(
        ts7053[0].contains("expression of type 'unique symbol'"),
        "Expected the imported CommonJS export to preserve unique symbol identity, got: {ts7053:#?}"
    );
}
