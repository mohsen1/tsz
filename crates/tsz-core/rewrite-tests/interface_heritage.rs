use std::sync::Arc;

use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn options() -> CompilerOptions {
    CompilerOptions {
        no_emit: true,
        strict: false,
        ..CompilerOptions::default()
    }
}

fn compile(source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &options(),
    )
}

fn assert_complete(source: &str) {
    let output = compile(source);
    assert!(
        output.diagnostics.is_empty(),
        "{source}: {:?}",
        output.diagnostics
    );
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Complete,
        "{source}"
    );
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Complete,
        "{source}"
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
    assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
}

fn assert_deferred(source: &str) {
    let output = compile(source);
    assert!(
        output.diagnostics.is_empty(),
        "{source}: {:?}",
        output.diagnostics
    );
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Deferred,
        "{source}"
    );
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Deferred,
        "{source}"
    );
    assert_eq!(
        output.exit_status,
        CompileExitStatus::SemanticIncomplete,
        "{source}"
    );
    assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
}

#[test]
fn one_hop_property_heritage_assembles_single_and_multiple_bases() {
    // When a sole interface has one or more sole, non-heritage bases and all
    // involved members are required properties, pinned TS7 assembles base
    // properties in authored order and then applies the derived properties.
    for source in [
        r#"
            interface Foundation { base: string; count: number }
            interface Concrete extends Foundation { own: boolean }
            declare let concrete: Concrete;
            concrete.base; concrete.count; concrete.own;
        "#,
        r#"
            interface Parent<Payload> { base: Payload; list: Payload[] }
            interface Child<Subject> extends Parent<Subject> { own: Subject }
            declare let child: Child<string>;
            child.base; child.list; child.own;
        "#,
        r#"
            interface Left<Alpha> { left: Alpha }
            interface Right<Beta> { right: Beta[] }
            interface Pair<Subject> extends Left<Subject>, Right<Subject> {
                own: Subject
            }
            declare let pair: Pair<string>;
            pair.left; pair.right; pair.own;
        "#,
    ] {
        assert_complete(source);
    }
}

#[test]
fn identical_substituted_duplicates_are_idempotent() {
    for source in [
        r#"
            interface Left<Payload> { shared: Payload; left: Payload }
            interface Right<Element> { shared: Element; right: Element }
            interface Combined<Subject> extends Left<Subject>, Right<Subject> {
                shared: Subject;
                own: Subject;
            }
            declare let value: Combined<string>;
            value.shared; value.left; value.right; value.own;
        "#,
        // This is the pinned nonConflictingRecursiveBaseTypeMembers shape.
        r#"
            interface Alpha<Payload> { recursive: Combined<Payload> }
            interface Beta<Element> { recursive: Combined<Element> }
            interface Combined<Subject> extends Alpha<Subject>, Beta<Subject> {}
            declare let value: Combined<string>;
            value.recursive;
        "#,
    ] {
        assert_complete(source);
    }
}

#[test]
fn heritage_property_order_matches_the_pinned_stable_declaration_order() {
    // With stable type ordering, pinned TS7 emits own members first and then
    // bases in declaration order. Reordering the extends clause does not
    // reorder the inherited diagnostic provenance.
    let source = r#"
        interface First { baseB: string; baseA: string }
        interface Second { second: string }
        interface Forward extends First, Second { own: string }
        interface Reverse extends Second, First { own: string }
        declare const missing: { present: number }[];
        const forward: Forward[] = missing;
        const reverse: Reverse[] = missing;
    "#;
    let output = compile(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![2322, 2322]
    );
    let missing_order = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            diagnostic
                .related_information
                .last()
                .and_then(|related| related.message_text.rsplit_once(": "))
                .map(|(_, properties)| properties)
                .expect("multi-missing continuation")
        })
        .collect::<Vec<_>>();
    assert_eq!(missing_order, ["own, baseB, baseA, second"; 2]);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Complete
    );
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn heritage_property_order_deduplicates_shadows_and_preserves_nested_order() {
    let shadowed = compile(
        r#"
            interface First { shared: string; first: string }
            interface Second { shared: string; second: string }
            interface Derived extends Second, First { shared: string; own: string }
            declare const missing: { present: number }[];
            const target: Derived[] = missing;
        "#,
    );
    assert_eq!(shadowed.diagnostics.len(), 1, "{:?}", shadowed.diagnostics);
    assert_eq!(shadowed.diagnostics[0].code, 2322);
    let shadowed_order = shadowed.diagnostics[0]
        .related_information
        .last()
        .and_then(|related| related.message_text.rsplit_once(": "))
        .map(|(_, properties)| properties);
    assert_eq!(shadowed_order, Some("shared, own, first, second"));
    assert_eq!(shadowed.semantic_completion, SemanticCompletion::Complete);

    // Renaming each generic binder does not change the nested authored order
    // inherited from the base declaration.
    let nested = compile(
        r#"
            interface Base<Payload> {
                nested: { zeta: Payload; alpha: Payload }
            }
            interface Derived<Subject> extends Base<Subject> {}
            declare const source: { nested: { present: number } }[];
            const target: Derived<string>[] = source;
        "#,
    );
    assert_eq!(nested.diagnostics.len(), 1, "{:?}", nested.diagnostics);
    assert_eq!(nested.diagnostics[0].code, 2322);
    assert_eq!(
        nested.diagnostics[0]
            .related_information
            .last()
            .map(|related| related.message_text.as_str()),
        Some(
            "Type '{ present: number; }' is missing the following properties from type '{ zeta: string; alpha: string; }': zeta, alpha"
        )
    );
    assert_eq!(nested.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn substituted_property_conflicts_remain_typed_nonclaims() {
    for source in [
        "interface A{value:string} interface B{value:number} interface D extends A,B{} declare let d:D;",
        "interface B<T>{value:T} interface D<T> extends B<T>{value:T[]} declare let d:D<string>;",
        "interface A<T>{value:T} interface B<T>{value:T[]} interface D<T> extends A<T>,B<T>{} declare let d:D<string>;",
    ] {
        assert_deferred(source);
    }
}

#[test]
fn unsupported_heritage_facts_do_not_enter_the_property_merge() {
    let deferred = [
        // Optional and readonly require override/relation diagnostics.
        "interface B<T>{value?:T} interface D<T> extends B<T>{} declare let d:D<string>;",
        "interface B<T>{readonly value:T} interface D<T> extends B<T>{} declare let d:D<string>;",
        // Methods, call/index signatures, and accessors have distinct owners.
        "interface B<T>{value(input:T):T} interface D<T> extends B<T>{} declare let d:D<string>;",
        "interface B<T>{(input:T):T} interface D<T> extends B<T>{} declare let d:D<string>;",
        "interface B<T>{[key:string]:T} interface D<T> extends B<T>{} declare let d:D<string>;",
        "interface B<T>{get value():T} interface D<T> extends B<T>{} declare let d:D<string>;",
        // Own non-property members cannot bypass the same declaration gate.
        "interface B<T>{value:T} interface D<T> extends B<T>{own?:T} declare let d:D<string>;",
        "interface B<T>{value:T} interface D<T> extends B<T>{readonly own:T} declare let d:D<string>;",
        "interface B<T>{value:T} interface D<T> extends B<T>{own(input:T):T} declare let d:D<string>;",
        // Merged and transitive declarations need provenance-bearing queries.
        "interface B<T>{left:T} interface B<T>{right:T} interface D<T> extends B<T>{} declare let d:D<string>;",
        "interface A<T>{value:T} interface B<T> extends A<T>{} interface D<T> extends B<T>{} declare let d:D<string>;",
        // Constraints/defaults and non-exact arity are owned by reference diagnostics.
        "interface B<T extends unknown>{value:T} interface D<T extends unknown> extends B<T>{} declare let d:D<string>;",
        "interface B<T=string>{value:T} interface D<T=string> extends B<T>{} declare let d:D;",
        "interface B<T>{value:T} interface D<T> extends B<T>{} declare let d:D<string,number>;",
        "interface B<T,U>{value:T} interface D<T> extends B<T>{} declare let d:D<string>;",
        "interface B<T>{value:T} interface D<T,U> extends B<T,U>{} declare let d:D<string,number>;",
        // Heritage substitution is deliberately positional and untransformed.
        "interface B<T>{value:T} interface D<T> extends B<T[]>{} declare let d:D<string>;",
    ];
    for source in deferred {
        assert_deferred(source);
    }
}

#[test]
fn property_heritage_is_cold_warm_root_order_stable_and_bounded() {
    let declarations = SourceInput::new(
        "models.ts",
        Arc::<str>::from(
            r#"
                interface Left<Payload> { shared: Payload; left: Payload }
                interface Right<Element> { shared: Element; right: Element[] }
                interface Combined<Subject> extends Left<Subject>, Right<Subject> {
                    own: Subject
                }
            "#,
        ),
    );
    let first = SourceInput::new(
        "first.ts",
        Arc::<str>::from(
            "declare let first:Combined<string>; first.shared; first.left; first.right; first.own;",
        ),
    );
    let second = SourceInput::new(
        "second.ts",
        Arc::<str>::from(
            "declare let second:Combined<string>; second.own; second.right; second.left; second.shared;",
        ),
    );
    let compiler = Compiler::new();
    let run = |inputs| compiler.compile(inputs, &options());
    let forward = vec![declarations.clone(), first.clone(), second.clone()];
    let reverse = vec![second, first, declarations];
    let cold = run(forward.clone());
    let warm = run(forward);
    let reversed = run(reverse);
    let fingerprint = |output: &tsz::CompileOutput| {
        (
            serde_json::to_vec(&output.diagnostics).unwrap(),
            output.semantic_completion,
            output.stats.semantic_completion,
            output.exit_status,
            output.stats.types,
        )
    };

    assert!(cold.diagnostics.is_empty(), "{:?}", cold.diagnostics);
    assert_eq!(cold.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(cold.exit_status, CompileExitStatus::Success);
    assert_eq!(fingerprint(&cold), fingerprint(&warm));
    assert_eq!(fingerprint(&cold), fingerprint(&reversed));
    assert!(cold.stats.types < 768, "unbounded growth: {:?}", cold.stats);
}
