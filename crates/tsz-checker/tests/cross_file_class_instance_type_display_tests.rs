//! Structural rule for issue #8720.
//!
//! For every class symbol, the owning checker must publish the class
//! instance TypeId into the shared `DefinitionStore::class_to_instance`
//! slot once the class type is built. Cross-file consumers resolve
//! `Lazy(class_def_id)` (type position) by checking
//! `symbol_instance_types`, then the local `TypeEnvironment` class
//! instance cache, then `DefinitionStore::get_class_instance_type` (the
//! shared slot), and only as a last resort fall through to
//! `DefinitionStore::get_body` — which still holds the *constructor*
//! TypeId for value-position lookups (`typeof C`, bare `C` value
//! references). Without the shared instance-type slot, a consumer whose
//! local caches are empty cascades to the constructor TypeId (which is
//! registered against the `ClassConstructor` companion `DefId`) and the
//! diagnostic formatter renders the parameter as `typeof ClassName`
//! instead of `ClassName` — the visible symptom in the reported repro
//! (`module.exports = LazySet`, `const LazySet = require(...)`,
//! `x.addAll(x)` → `parameter of type 'typeof LazySet'`).
//!
//! This file pins the structural invariant directly: after
//! `get_type_of_symbol` runs for a class symbol, the
//! `class_to_instance` slot must be populated with the instance TypeId.
//! The rule is verified for several adjacent name choices and class
//! shapes so the fix cannot regress silently when a single fixture's
//! spelling changes.

use tsz_binder::{BinderState, symbol_flags};
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn check_single_file(file_name: &str, source: &str) -> (Vec<(u32, String)>, Vec<ClassInvariant>) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_lib: true,
            ..Default::default()
        },
    );
    checker.check_source_file(root);

    // Collect every class symbol that was promoted to a DefId during checking
    // and snapshot the invariant.
    let class_invariants = checker
        .ctx
        .binder
        .file_locals
        .iter()
        .filter_map(|(name, sym_id)| {
            let symbol = checker.ctx.binder.get_symbol(*sym_id)?;
            if !symbol.has_any_flags(symbol_flags::CLASS) {
                return None;
            }
            let class_def_id = checker.ctx.get_existing_def_id(*sym_id)?;
            let class_instance = checker
                .ctx
                .definition_store
                .get_class_instance_type(class_def_id);
            let class_body = checker.ctx.definition_store.get_body(class_def_id);
            let ctor_def_id = checker
                .ctx
                .definition_store
                .get_constructor_def(class_def_id);
            let ctor_body = ctor_def_id.and_then(|d| checker.ctx.definition_store.get_body(d));
            Some(ClassInvariant {
                name: name.to_string(),
                class_def_id: class_def_id.0,
                class_instance_type_id: class_instance.map(|t| t.0),
                class_body_type_id: class_body.map(|t| t.0),
                constructor_def_id: ctor_def_id.map(|d| d.0),
                constructor_body_type_id: ctor_body.map(|t| t.0),
            })
        })
        .collect::<Vec<_>>();

    let diagnostics = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    (diagnostics, class_invariants)
}

#[derive(Debug)]
#[allow(dead_code)]
struct ClassInvariant {
    name: String,
    class_def_id: u32,
    class_instance_type_id: Option<u32>,
    class_body_type_id: Option<u32>,
    constructor_def_id: Option<u32>,
    constructor_body_type_id: Option<u32>,
}

/// The shared `class_to_instance` slot must be populated with a TypeId
/// distinct from the constructor body. Cross-file consumers rely on this
/// slot to resolve `Lazy(class_def_id)` in type position without falling
/// through to the constructor (which would render as `typeof ClassName`).
fn assert_class_instance_type_is_published(invariants: &[ClassInvariant]) {
    for invariant in invariants {
        let class_instance = invariant.class_instance_type_id.unwrap_or_else(|| {
            panic!(
                "[{name}] expected `DefinitionStore::get_class_instance_type(DefId({def}))` to be \
                 populated after checking, got None. Without this slot, cross-file \
                 `Lazy(class_def_id)` resolutions cascade to the constructor TypeId and render as \
                 `typeof {name}` in diagnostics. invariants={invariants:#?}",
                name = invariant.name,
                def = invariant.class_def_id,
            )
        });
        let ctor_def = invariant.constructor_def_id.unwrap_or_else(|| {
            panic!(
                "[{name}] expected ClassConstructor companion DefId for class DefId({def}), got None. invariants={invariants:#?}",
                name = invariant.name,
                def = invariant.class_def_id,
            )
        });
        let ctor_body = invariant.constructor_body_type_id.unwrap_or_else(|| {
            panic!(
                "[{name}] expected constructor body for ClassConstructor DefId({ctor_def}), got None. invariants={invariants:#?}",
                name = invariant.name,
            )
        });
        assert_ne!(
            class_instance,
            ctor_body,
            "[{name}] `class_to_instance[DefId({class_def})]` must differ from \
             ClassConstructor body TypeId({ctor_body}). Both collapsed to TypeId({class_instance}) — \
             the instance type slot was populated with the constructor TypeId, which is the \
             precise bug from issue #8720 (`typeof {name}` rendered as a parameter type display).",
            name = invariant.name,
            class_def = invariant.class_def_id,
        );
    }
}

#[test]
fn class_def_body_is_instance_type_not_constructor_simple() {
    let (_diagnostics, invariants) = check_single_file(
        "test.ts",
        r#"
class LazySet {
    addAll(iterable: LazySet): void {}
}
const x = new LazySet();
x.addAll(x);
"#,
    );
    let lazy_set: Vec<_> = invariants.iter().filter(|i| i.name == "LazySet").collect();
    assert_eq!(
        lazy_set.len(),
        1,
        "Expected exactly one LazySet class invariant, got: {invariants:#?}"
    );
    assert_class_instance_type_is_published(&lazy_set.into_iter().cloned().collect::<Vec<_>>());
}

#[test]
fn class_def_body_is_instance_type_not_constructor_renamed() {
    // Rename the class to prove the rule isn't keyed on a specific identifier
    // (CLAUDE.md §25 anti-hardcoding).
    let (_diagnostics, invariants) = check_single_file(
        "test.ts",
        r#"
class Tree {
    merge(other: Tree): void {}
}
const t = new Tree();
t.merge(t);
"#,
    );
    assert_class_instance_type_is_published(&invariants);
}

#[test]
fn class_def_body_is_instance_type_for_multiple_classes() {
    // Multiple distinct classes in the same file — each Class DefId carries
    // its own instance type, none collapse to a constructor.
    let (_diagnostics, invariants) = check_single_file(
        "test.ts",
        r#"
class Alpha {
    touch(other: Alpha): void {}
}
class Beta {
    touch(other: Beta): void {}
}
class Gamma {
    consume(a: Alpha, b: Beta): void {}
}
const a = new Alpha();
const b = new Beta();
const g = new Gamma();
g.consume(a, b);
a.touch(a);
b.touch(b);
"#,
    );
    let named_classes: Vec<_> = invariants
        .iter()
        .filter(|i| matches!(i.name.as_str(), "Alpha" | "Beta" | "Gamma"))
        .cloned()
        .collect();
    assert_eq!(
        named_classes.len(),
        3,
        "Expected three named classes in invariants, got: {invariants:#?}"
    );
    assert_class_instance_type_is_published(&named_classes);
}

#[test]
fn class_def_body_is_instance_type_with_js_class_jsdoc() {
    // JS class with JSDoc @param — same code path that the cross-file
    // `module.exports = LazySet` repro exercised in issue #8720. The
    // structural rule must hold whether the class is annotated via TS
    // syntax or JSDoc.
    let (_diagnostics, invariants) = check_single_file(
        "test.js",
        r#"
class Bucket {
    /** @param {Bucket} other */
    take(other) {}
}
const x = new Bucket();
x.take(x);
"#,
    );
    let buckets: Vec<_> = invariants
        .iter()
        .filter(|i| i.name == "Bucket")
        .cloned()
        .collect();
    assert!(
        !buckets.is_empty(),
        "Expected Bucket class invariant, got: {invariants:#?}"
    );
    assert_class_instance_type_is_published(&buckets);
}

#[test]
fn no_typeof_param_diagnostic_for_self_method_param() {
    // End-to-end check: with the class def body correctly carrying the
    // instance type, `x.addAll(x)` must not produce a TS2345 whose target
    // display starts with `typeof`. tsc shows the parameter as the class
    // instance name (`LazySet`).
    let (diagnostics, _invariants) = check_single_file(
        "test.ts",
        r#"
class LazySet {
    addAll(iterable: LazySet): void {}
}
const x = new LazySet();
x.addAll(x);
"#,
    );
    let typeof_offenders: Vec<_> = diagnostics
        .iter()
        .filter(|(_, msg)| {
            msg.contains("parameter of type 'typeof ") || msg.contains("required in type 'typeof ")
        })
        .collect();
    assert!(
        typeof_offenders.is_empty(),
        "Expected no `typeof X` parameter diagnostics, got:\n{typeof_offenders:#?}\nAll diagnostics:\n{diagnostics:#?}"
    );
}

impl Clone for ClassInvariant {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            class_def_id: self.class_def_id,
            class_instance_type_id: self.class_instance_type_id,
            class_body_type_id: self.class_body_type_id,
            constructor_def_id: self.constructor_def_id,
            constructor_body_type_id: self.constructor_body_type_id,
        }
    }
}
