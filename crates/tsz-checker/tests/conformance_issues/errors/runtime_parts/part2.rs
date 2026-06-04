/// TS18013 must use the actual class name for generic class instances
/// (`C<number>`). The display-type-to-class resolver previously only handled
/// uninstantiated instance types, brand-bearing object shapes, and lazy
/// references — so a generic instance stored in its application form fell
/// through to the `"the class"` fallback whenever the class references itself
/// under multiple instantiations like `bar(x: C<T>)`, `baz(x: C<number>)`,
/// which forces the instance type to stay in application form. Source:
/// conformance test `privateNamesInGenericClasses.ts`.
#[test]
fn test_ts18013_uses_class_name_for_generic_class_instance() {
    assert_ts18013_uses_class_name(
        r#"
class C<T> {
    #foo: T = undefined as any;
    #method(): T { return this.#foo; }
    bar(x: C<T>) { return x.#foo; }
    baz(x: C<number>) { return x.#foo; }
    quux(x: C<string>) { return x.#foo; }
}
declare let a: C<number>;
a.#foo;
a.#method;
        "#,
        "C",
        2,
    );
}

/// Same rule with a renamed type parameter (`U` instead of `T`) and class
/// (`Q` instead of `C`). If the fix is hardcoded to a specific identifier
/// name, this test will fail.
#[test]
fn test_ts18013_uses_class_name_for_generic_class_instance_renamed_type_param() {
    assert_ts18013_uses_class_name(
        r#"
class Q<U> {
    #x: U = undefined as any;
    #m(): U { return this.#x; }
    self(p: Q<U>) { return p.#x; }
    numericInst(p: Q<number>) { return p.#x; }
    stringInst(p: Q<string>) { return p.#x; }
}
declare let qq: Q<number>;
qq.#x;
qq.#m;
        "#,
        "Q",
        2,
    );
}

/// When `#x` is declared in generic `Base<T>` and accessed via an instance of
/// generic `Derived<number>`, TS18013 must report the *declaring* class name
/// (`Base`), not the receiver's display name. This combines the
/// declaring-class-name rule (`test_ts18013_reports_declaring_class_name`) with
/// the Application-unwrap fix: the receiver is an instance of `Derived<number>`
/// (which is `Application(Derived, [number])`), and the brand of `#x` points
/// to `Base`. The resolver must unwrap the application and walk the class
/// hierarchy to find `Base` as the declaring class.
#[test]
fn test_ts18013_declaring_class_name_for_generic_inheritance() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base<T> {
    #x: T = undefined as any;
    self(p: Base<T>) { return p.#x; }
    numericInst(p: Base<number>) { return p.#x; }
}
class Derived<T> extends Base<T> {}
declare let d: Derived<number>;
d.#x;
        "#,
    );

    let ts18013_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(c, _)| *c == 18013)
        .map(|(_, m)| m.as_str())
        .collect();

    assert_eq!(
        ts18013_messages.len(),
        1,
        "Should emit exactly one TS18013.\nActual errors: {diagnostics:?}"
    );
    assert!(
        ts18013_messages[0].contains("'Base'"),
        "TS18013 should reference the declaring class 'Base', not 'Derived' \
         and not 'the class'.\nActual message: {}",
        ts18013_messages[0]
    );
}

/// TS2344 false positive: a generic indexed-access type argument `T[K]` whose
/// resolved property values structurally satisfy a non-callable interface
/// constraint should not emit TS2344. The conformance test
/// `inferenceDoesNotAddUndefinedOrNull` triggers this when a user file
/// declaration-merges with a lib interface (e.g. `interface Node`):
/// `lib.dom.d.ts`'s `getElementsByTagName<K extends keyof HTMLElementTagNameMap>(...)
/// : HTMLCollectionOf<HTMLElementTagNameMap[K]>` is re-checked, and the
/// constraint `<T extends Element>` of `HTMLCollectionOf` fails for
/// `HTMLElementTagNameMap[K]` even though every value extends `Element`.
///
/// Root cause (under investigation, 2026-05): the constraint validator at
/// `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
/// resolves `Map[K]`'s base constraint to the union of property values
/// (correct), but the subsequent `is_assignable_to(base, constraint)` check
/// returns false when `base` is a lib-arena `TypeId` and `constraint` is the
/// user-arena (declaration-merged) `TypeId` for the same nominal interface.
/// `base_union_members_satisfy_constraint` masks this for union bases
/// because each member's `is_assignable_to` runs in the same arena context
/// during the lib re-check, but the user-file constraint check exposes the
/// cross-arena divergence.
///
/// This test is `#[ignore]`'d because the unit-test harness does not
/// reproduce the binary's lib re-check pathway (`check_source_file_interfaces_only_filtered_post_merge`).
/// The corresponding conformance test
/// `compiler/inferenceDoesNotAddUndefinedOrNull.ts` is the primary
/// integration-level reproducer.
#[test]
fn test_no_false_ts2344_for_indexed_access_value_subtype_of_constraint() {
    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        r#"
interface NodeArray<T extends Node> extends ReadonlyArray<T> {}

interface Node {
    forEachChild<T>(cbNode: (node: Node) => T | undefined, cbNodeArray?: (nodes: NodeArray<Node>) => T | undefined): T | undefined;
}
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2344),
        "Should not emit TS2344 for `HTMLElementTagNameMap[K]` against an `extends Element` constraint when the user redeclares a lib interface (declaration-merge). Actual: {diagnostics:?}"
    );
}
