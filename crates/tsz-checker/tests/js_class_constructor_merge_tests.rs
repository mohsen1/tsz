use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_project(files: &[(&str, &str)]) -> Vec<(String, Vec<u32>)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        no_lib: true,
        strict: true,
        target: ScriptTarget::ES2015,
        ..Default::default()
    };
    let types = TypeInterner::new();

    file_names
        .iter()
        .enumerate()
        .map(|(file_idx, file_name)| {
            let mut checker = CheckerState::new(
                all_arenas[file_idx].as_ref(),
                all_binders[file_idx].as_ref(),
                &types,
                file_name.clone(),
                options.clone(),
            );
            checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
            checker.ctx.set_all_binders(Arc::clone(&all_binders));
            checker.ctx.set_current_file_idx(file_idx);
            checker.ctx.set_lib_contexts(Vec::new());
            checker.check_source_file(roots[file_idx]);

            (
                file_name.clone(),
                checker.ctx.diagnostics.iter().map(|d| d.code).collect(),
            )
        })
        .collect()
}

#[test]
fn checked_js_constructor_var_merges_with_class_without_false_duplicates_or_new_errors() {
    let diagnostics = check_project(&[
        (
            "file1.js",
            r#"
var SomeClass = function () {
    this.otherProp = 0;
};

new SomeClass();
"#,
        ),
        (
            "file2.js",
            r#"
class SomeClass { }
SomeClass.prop = 0;
"#,
        ),
    ]);

    let mut offenders = Vec::new();
    for (file_name, codes) in &diagnostics {
        for &code in codes {
            if code == 2300 || code == 2339 || code == 7009 {
                offenders.push((file_name.clone(), code));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "Expected no TS2300/TS2339/TS7009 for checked-JS constructor/class merge, got: {diagnostics:#?}"
    );
}

/// Regression test: when a constructor function in file1.js merges with a class
/// in file2.js, accessing static properties on the class (e.g. `SomeClass.prop = 0`)
/// should NOT produce TS18046 (`'SomeClass' is of type 'unknown'`).
///
/// Root cause: `compute_class_symbol_type` only searched the current file's arena
/// for the class declaration. When the `CLASS` declaration was in a different file's
/// arena, the function returned `TypeId::UNKNOWN`, triggering false TS18046 errors
/// on any property access or constructor call on the class.
#[test]
fn cross_file_class_merge_no_false_ts18046() {
    let diagnostics = check_project(&[
        (
            "file1.js",
            r#"
var SomeClass = function () {
    this.otherProp = 0;
};

new SomeClass();
"#,
        ),
        (
            "file2.js",
            r#"
class SomeClass { }
SomeClass.prop = 0;
"#,
        ),
    ]);

    let mut offenders = Vec::new();
    for (file_name, codes) in &diagnostics {
        for &code in codes {
            if code == 18046 || code == 2339 {
                offenders.push((file_name.clone(), code));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "Expected no TS18046/TS2339 for cross-file class/constructor merge, but got: {offenders:?}\nAll diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn cross_file_script_constructor_base_keeps_instance_surface_for_override_check() {
    let diagnostics = check_project(&[
        (
            "first.js",
            r#"
/**
 * @constructor
 * @param {number} numberOxen
 */
function Wagon(numberOxen) {
    this.numberOxen = numberOxen;
}
class Drakkhen extends Dragon {}
"#,
        ),
        (
            "second.ts",
            r#"
function Dragon(numberEaten: number) {
    this.numberEaten = numberEaten;
}
class Conestoga extends Wagon {
    numberOxen: string = "";
    constructor() { super(4); }
}
"#,
        ),
    ]);

    let mut second_ts2416_count = 0;
    let mut false_cross_file_errors = Vec::new();
    for (file_name, codes) in &diagnostics {
        for &code in codes {
            if file_name == "second.ts" && code == 2416 {
                second_ts2416_count += 1;
            }
            if code == 2304 || code == 2339 || (file_name == "second.ts" && code == 2507) {
                false_cross_file_errors.push((file_name.clone(), code));
            }
        }
    }

    assert_eq!(
        second_ts2416_count, 1,
        "Expected exactly one TS2416 in second.ts for Conestoga.numberOxen overriding Wagon.numberOxen, got: {diagnostics:#?}"
    );
    assert!(
        false_cross_file_errors.is_empty(),
        "Expected no false TS2304/TS2339 or second.ts TS2507 for script-global constructor bases, got: {false_cross_file_errors:?}\nAll diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn cross_file_namespace_prototype_object_literals_preserve_constructor_identity() {
    let diagnostics = check_project(&[
        (
            "prototypePropertyAssignmentMergeAcrossFiles2.js",
            r#"
var Ns = {};
Ns.One = function() {};
Ns.Two = function() {};

Ns.One.prototype = {
    ok() {},
};
Ns.Two.prototype = {};
"#,
        ),
        (
            "other.js",
            r#"
/**
 * @type {Ns.One}
 */
var one = undefined;
one.wat;
/**
 * @type {Ns.Two}
 */
var two = undefined;
two.wat;
"#,
        ),
    ]);

    let other_codes = diagnostics
        .iter()
        .find(|(file_name, _)| file_name == "other.js")
        .map(|(_, codes)| codes.as_slice())
        .expect("other.js diagnostics");

    assert_eq!(
        other_codes.iter().filter(|&&code| code == 2322).count(),
        2,
        "Expected two TS2322 diagnostics for assigning undefined to the cross-file prototype constructors, got: {diagnostics:#?}"
    );
    assert_eq!(
        other_codes.iter().filter(|&&code| code == 2339).count(),
        2,
        "Expected two TS2339 diagnostics for missing properties on the preserved constructor identities, got: {diagnostics:#?}"
    );
}
