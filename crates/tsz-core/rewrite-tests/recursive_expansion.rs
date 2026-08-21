use std::sync::Arc;

use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{ExpressionKind, StatementKind, parse_source};
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

fn compile_with_strictness(source: &str, strict: bool) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            no_emit: true,
            strict,
            ..CompilerOptions::default()
        },
    )
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn codes_from_diagnostics(diagnostics: &[tsz::diagnostics::Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_complete(output: &tsz::CompileOutput) {
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Complete,
        "unexpected completion for diagnostics {:?}",
        output.diagnostics
    );
    assert_eq!(
        output.stats.semantic_completion,
        SemanticCompletion::Complete
    );
}

#[test]
fn expanding_generic_relations_match_the_pinned_ts7_coinductive_result() {
    // When both related sides repeatedly instantiate the same generic origin
    // through growing arguments, pinned TS7 returns a provisional recursive
    // result. TSZ does so through the relation's query-local reference stacks.
    let cases = [
        r#"
            interface Stream<Item> { next: Stream<Item[]> }
            interface Cedar { cedar: unknown }
            interface Birch { birch: unknown }
            declare let cedar: Stream<Cedar>;
            declare let birch: Stream<Birch>;
            cedar = birch;
        "#,
        r#"
            interface Link<Item> { next: Link<Item> }
            interface Cedar { next: Cedar }
            interface Birch { next: Birch }
            declare let link: Link<Cedar>;
            declare let peer: Link<Birch>;
            link = peer;
        "#,
        r#"
            interface Stream<Item> { next: Stream<Item[]> }
            type Wrapped<Item> = Stream<Item>;
            interface Cedar { cedar: unknown }
            interface Birch { birch: unknown }
            declare let cedar: Wrapped<Cedar>;
            declare let birch: Wrapped<Birch>;
            cedar = birch;
        "#,
    ];

    for source in cases {
        let output = compile(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
        assert_eq!(
            output.stats.semantic_completion,
            SemanticCompletion::Complete
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success);
        assert!(
            output.stats.types < 512,
            "unbounded type growth: {:?}",
            output.stats
        );
    }
}

#[test]
fn distinct_or_asymmetric_generative_relations_fail_closed() {
    let cases = [
        // The structural relation is TS7-clean, but this checkpoint only
        // admits a provisional edge for one shared recursion origin.
        r#"
            interface Alpha<Value> { next: Alpha<Value[]> }
            interface Beta<Subject> { next: Beta<Subject[]> }
            declare let alpha: Alpha<string>;
            declare let beta: Beta<number>;
            alpha = beta;
        "#,
        // Different wrapper transforms diverge below the first growing edge.
        // Returning success there would hide TS7's deeper TS2322 path.
        r#"
            interface Left<Value> { next: Left<Value[]>; value: Value }
            interface Right<Value> { next: Right<{ value: Value }>; value: Value }
            declare let left: Left<number>;
            declare let right: Right<number>;
            left = right;
        "#,
        // Mutual recursive origins need a transform proof spanning both
        // declarations, which is outside this single-origin checkpoint.
        r#"
            interface Left<Item> { right: Right<Item[]> }
            interface Right<Item> { left: Left<Item[]> }
            interface Cedar { cedar: unknown }
            interface Birch { birch: unknown }
            declare let cedar: Left<Cedar>;
            declare let birch: Left<Birch>;
            cedar = birch;
        "#,
    ];

    for source in cases {
        let output = compile(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types < 512, "{:?}", output.stats);
    }
}

#[test]
fn recursive_assumption_does_not_hide_an_exposed_sibling_mismatch() {
    // The recursively expanding `next` property is authored first on purpose:
    // a provisional edge must not skip the later concrete `value` property.
    let output = compile(
        r#"
            interface Stream<Item> {
                next: Stream<Item[]>;
                value: Item;
            }
            interface Cedar { cedar: unknown }
            interface Birch { birch: unknown }
            declare let cedar: Stream<Cedar>;
            declare let birch: Stream<Birch>;
            cedar = birch;
        "#,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn relation_cutoff_does_not_skip_a_later_expansion_sibling() {
    // `value` is compatible at the root and incompatible only after `next`
    // expands. A cutoff for the recursive edge therefore cannot assume that
    // the finite sibling remains related.
    let output = compile(
        r#"
            interface Loop<Value> {
                next: Loop<Value[]>;
                value: Value | string;
            }
            declare let source: Loop<string>;
            declare let target: Loop<number>;
            target = source;
        "#,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(output.stats.types < 512, "{:?}", output.stats);
}

#[test]
fn projection_and_missing_property_display_are_bounded_demands() {
    let output = compile(
        r#"
            interface Stream<Item> { next: Stream<Item[]> }
            interface Cedar { cedar: unknown }
            declare let stream: Stream<Cedar>;
            stream.next.next;
            stream.missing;
        "#,
    );
    assert_eq!(codes(&output), vec![2339], "{:?}", output.diagnostics);
    assert_complete(&output);
    assert!(
        output.stats.types < 512,
        "unbounded type growth: {:?}",
        output.stats
    );
}

#[test]
fn generative_cutoff_validates_arguments_and_every_shape_sibling() {
    // Finite shrinking applications must reach the callable alias. Likewise,
    // a growing edge cannot conceal an unsupported callable sibling in either
    // authored order.
    let cases = [
        r#"
            type Callback = (value: number) => string;
            interface Box<Value> { value: Value }
            interface Host { value: Box<Box<Box<Callback>>> }
            declare let host: Host;
        "#,
        r#"
            type Callback = (value: number) => string;
            interface Loop<Value> {
                next: Loop<Value[]>;
                invoke: Callback;
            }
            declare let loop: Loop<string>;
        "#,
        r#"
            type Renamed = (seed: boolean) => number;
            interface Recurrence<Subject> {
                invoke: Renamed;
                next: Recurrence<Subject[]>;
            }
            declare let recurrence: Recurrence<number>;
        "#,
    ];

    for source in cases {
        let output = compile(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(
            output.stats.types < 512,
            "unbounded type growth: {:?}",
            output.stats
        );
    }
}

#[test]
fn generative_admission_validates_the_origin_and_every_active_frame() {
    let cases = [
        // The root carries an extra argument even though recursive children
        // normalize to the declaration's authored arity.
        r#"
            interface Loop<Value> { next: Loop<Value[]> }
            declare let left: Loop<number, string>;
            declare let right: Loop<boolean, string>;
            left = right;
        "#,
        // The recursive child itself carries the wrong arity.
        r#"
            interface Loop<Value> { next: Loop<Value[], string> }
            declare let left: Loop<number>;
            declare let right: Loop<boolean>;
            left = right;
        "#,
        // Every mutual frame is validated with its own declaration and args.
        r#"
            interface Left<Value> { right: Right<Value[], string> }
            interface Right<Value> { left: Left<Value[]> }
            declare let value: Left<number>;
        "#,
        r#"
            interface Left<Value> { right: Right<Value[]> }
            interface Right<Value extends string> { left: Left<Value[]> }
            declare let value: Left<number>;
        "#,
        // A merged origin cannot provisionally close before an unmodeled
        // sibling declaration has been assembled.
        r#"
            interface Stream<Value> { next: Stream<Value[]> }
            interface Stream<Value> { value: Value }
            declare let left: Stream<number>;
            declare let right: Stream<string>;
            left = right;
        "#,
        // Exact recursive alias/class owners also reject root arity before
        // substitution can discard an argument.
        "type Ring<T>={next:Ring<T>};declare let ring:Ring<string,number>;",
        "class Chain<T>{next:Chain<T>;}declare let chain:Chain<string,number>;",
    ];

    for source in cases {
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
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }
}

#[test]
fn reference_evaluation_never_elects_one_merged_type_declaration() {
    let cases = [
        r#"
            interface Mixed<Value> { value: Value }
            class Mixed<Value> { value: Value; }
            declare let mixed: Mixed<string>;
            mixed.value;
        "#,
        r#"
            class Mixed<Value> { value: Value; }
            interface Mixed<Value> { value: Value }
            declare let mixed: Mixed<string>;
            mixed.value;
        "#,
        r#"
            type Choice<Value> = { value: Value };
            type Choice<Value> = { other: Value };
            declare let choice: Choice<string>;
            choice.value;
        "#,
        // A script-global declaration colliding with an ambient library type
        // is not a sole recursion origin, even when only its authored member
        // is visible in this file.
        r#"
            interface Array<Value> { next: Array<Value[]> }
            declare let left: Array<string>;
            declare let right: Array<number>;
            left = right;
        "#,
        "function Mixed(){} class Mixed { value:string; } new Mixed();",
        "class Mixed { value:string; } function Mixed(){} new Mixed();",
    ];

    for source in cases {
        let output = compile(source);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}: {:?}",
            output.diagnostics
        );
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }
}

#[test]
fn circular_indexed_access_stays_outside_the_generative_shape_assumption() {
    let cases = [
        r#"
            type Direct = { x: Direct["x"] };
            declare let direct: Direct;
        "#,
        r#"
            type Project<Key extends "x" | "y"> = {
                x: Project<Key>[Key];
                y: number;
            };
            declare let demanded: Project<"x">;
            demanded.x;
        "#,
    ];
    for source in cases {
        let output = compile(source);
        assert_ne!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}: {:?}",
            output.diagnostics
        );
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }
}

#[test]
fn issue_1002_sources_terminate_without_turning_exact_cycles_into_success() {
    let first = compile(
        r#"
            interface IObservable<T> { n: IObservable<T[]>; }
            interface ISubject<T> extends IObservable<T> {}
            interface Foo { x }
            interface Bar { y }
            var values: IObservable<Foo>;
            var values2: ISubject<Bar>;
            values = values2;
        "#,
    );
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_complete(&first);
    assert_eq!(first.exit_status, CompileExitStatus::Success);
    assert!(
        first.stats.types < 512,
        "unbounded type growth: {:?}",
        first.stats
    );

    let second = compile(
        r#"
            interface IObservable<T> { n: IObservable<T[]>; }
            interface ISubject<T> extends IObservable<T> {}
            declare function combineLatest<TOther>(x: IObservable<TOther>[]): void;
            declare function combineLatest(): void;
            function fn<T>() {
                var values: ISubject<any>[] = [];
                combineLatest<T>(values);
            }
        "#,
    );
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(second.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(
        second.stats.types < 512,
        "unbounded type growth: {:?}",
        second.stats
    );

    let exact = compile("type Loop = Loop; declare let loop: Loop;");
    assert_eq!(codes(&exact), vec![2456]);
    assert_eq!(exact.semantic_completion, SemanticCompletion::Cycle);
    assert_eq!(exact.exit_status, CompileExitStatus::SemanticIncomplete);

    let productive = compile("type Ring = { next: Ring }; declare let ring: Ring;");
    assert!(
        productive.diagnostics.is_empty(),
        "{:?}",
        productive.diagnostics
    );
    assert_complete(&productive);
    assert_eq!(productive.exit_status, CompileExitStatus::Success);
}

#[test]
fn alias_recursion_requires_an_authored_productive_boundary() {
    let invalid = [
        "type Bad = Bad | string; declare let bad: Bad;",
        "type Bad = (Bad); declare let bad: Bad;",
        "type Bad = Bad & {}; declare let bad: Bad;",
        "type Bad = Bad | any; declare let bad: Bad;",
        "type Bad = Bad | unknown; declare let bad: Bad;",
        "type Bad = Bad | never; declare let bad: Bad;",
        "type Bad = Bad & any; declare let bad: Bad;",
        "type Bad = Bad & unknown; declare let bad: Bad;",
        "type Bad = Bad & never; declare let bad: Bad;",
        "type Identity<Value> = Value; type Bad = Identity<Bad>; declare let bad: Bad;",
        "type ArrayWrap<Value> = Value[]; type Bad = ArrayWrap<Bad>; declare let bad: Bad;",
        "type Left = Right; type Right = Left | string; declare let left: Left;",
        "type Bad<Value> = Bad<Value> | Value; declare let bad: Bad<string>;",
    ];
    for source in invalid {
        let output = compile(source);
        assert!(
            codes(&output).contains(&2456),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Cycle,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }

    let productive = [
        "type Good = string | Good[]; declare let good: Good;",
        "type Good = [Good]; declare let good: Good;",
        "type Good = { next: Good }; declare let good: Good;",
        "type Good = () => Good; declare let good: Good;",
        "type Good = new () => Good; declare let good: Good;",
        "type Identity<Value> = Value; type Good = Identity<Good>[]; declare let good: Good;",
        "type Good<Value> = Value | Good<Value>[]; declare let good: Good<string>;",
    ];
    for source in productive {
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
            SemanticCompletion::Complete
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }

    // The authored object boundary makes this cycle legal. Transparent alias
    // instantiation after that boundary remains symbolic until its own
    // instantiation query is modeled, but it must never become TS2456.
    for source in [
        "type Identity<Value> = Value; type Good = { next: Identity<Good> }; declare let good: Good;",
        "type Left = Right; type Right = { next: Left }; declare let left: Left;",
        "interface Box<Value> { value: Value } type Good = Box<Good>; declare let good: Good;",
        "type Identity<Value> = Value; interface Box<Value> { value: Value } type Good = Box<Identity<Good>>; declare let good: Good;",
    ] {
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
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }
}

#[test]
fn alias_cycle_classification_is_root_owned_and_lexically_scoped() {
    let descendant = "type Broken=Broken;type ObjectRoot={bad:Broken};type AliasRoot=Broken;type ArrayRoot=Broken[];declare let object:ObjectRoot;declare let aliasValue:AliasRoot;declare let arrayValue:ArrayRoot;";
    let output = compile(descendant);
    assert_eq!(codes(&output), vec![2456], "{:?}", output.diagnostics);
    assert_eq!(
        output.diagnostics[0].start,
        descendant.find("Broken=Broken").unwrap() as u32
    );
    assert_eq!(output.diagnostics[0].length, "Broken".len() as u32);
    assert_eq!(output.semantic_completion, SemanticCompletion::Cycle);

    // The queried root's cycle crosses an object boundary, even though a
    // descendant alias pair also contains its own neutral subcycle. Only the
    // declarations in that neutral pair own TS2456.
    let mixed = "type Outer={edge:Middle};type Middle=Inner;type Inner=Middle|Outer[];declare let outer:Outer;";
    let mixed_output = compile(mixed);
    assert_eq!(codes(&mixed_output), vec![2456, 2456]);
    let starts = mixed_output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.start)
        .collect::<Vec<_>>();
    assert_eq!(
        starts,
        vec![
            mixed.find("Middle=Inner").unwrap() as u32,
            mixed.find("Inner=Middle").unwrap() as u32,
        ]
    );

    let shadowed = "type Shadow<Shadow>=Shadow|string;type Global=Global;type Uses<Global>=Global|number;type FunctionShadow=<Global>(value:Global)=>Global;type MappedShadow={[Global in 'x']:Global};type InferShadow<Value>=Value extends infer Global?Global:never;declare let shadow:Shadow<string>;declare let uses:Uses<boolean>;declare let callable:FunctionShadow;declare let mapped:MappedShadow;declare let inferred:InferShadow<string>;";
    let shadowed_output = compile(shadowed);
    assert_eq!(codes(&shadowed_output), vec![2456]);
    assert_eq!(
        shadowed_output.diagnostics[0].start,
        shadowed.find("Global=Global").unwrap() as u32
    );
    assert_eq!(shadowed_output.diagnostics[0].length, "Global".len() as u32);
    assert!(
        shadowed_output.stats.types < 512,
        "{:?}",
        shadowed_output.stats
    );
}

#[test]
fn standard_library_alias_productivity_uses_typed_owner_capability() {
    // The generated library index does not carry the declaration kind for
    // mapped utility aliases. They therefore remain typed nonclaims instead
    // of borrowing Array's productive-boundary rule or caching an absorbed
    // recursive result.
    for source in [
        "type Cycle=Readonly<Cycle>;declare let value:Cycle;",
        "type Cycle=Partial<Cycle>;declare let value:Cycle;",
        "type Cycle=Required<Cycle>;declare let value:Cycle;",
        "type Cycle=Pick<Cycle,keyof Cycle>;declare let value:Cycle;",
        "type Cycle=Record<string,Cycle>;declare let value:Cycle;",
        "type Cycle=Record<string|number|symbol,Cycle>;declare let value:Cycle;",
    ] {
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
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }

    let array = compile("type Cycle=Array<Cycle>;declare let value:Cycle;");
    assert!(array.diagnostics.is_empty(), "{:?}", array.diagnostics);
    assert_complete(&array);
    assert!(array.stats.types < 512, "{:?}", array.stats);

    for source in [
        "type Cycle=ReadonlyArray<Cycle>;declare let value:Cycle;",
        "type Cycle=Promise<Cycle>;declare let value:Cycle;",
    ] {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }
}

#[test]
fn nested_recursive_shapes_keep_the_reference_path_bounded() {
    let cases = [
        r#"
            interface Stream<Value> { next: Stream<Value[]> }
            interface Outer { stream: Stream<string> }
            declare let outer: Outer;
            outer.stream;
        "#,
        r#"
            interface Stream<Value> { next: Stream<Value[]> }
            type Wrapped<Value> = { stream: Stream<Value> };
            declare let wrapped: Wrapped<string>;
            wrapped.stream;
        "#,
    ];
    for source in cases {
        let compiler = Compiler::new();
        let cold = compiler.compile(
            vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
            &options(),
        );
        let warm = compiler.compile(
            vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
            &options(),
        );
        assert!(
            cold.diagnostics.is_empty(),
            "{source}: {:?}",
            cold.diagnostics
        );
        assert_complete(&cold);
        assert_eq!(cold.exit_status, CompileExitStatus::Success);
        assert_eq!(
            cold.semantic_completion, warm.semantic_completion,
            "{source}"
        );
        assert_eq!(codes(&cold), codes(&warm), "{source}");
        assert_eq!(cold.stats.types, warm.stats.types, "{source}");
        assert!(cold.stats.types < 512, "{source}: {:?}", cold.stats);
    }
}

#[test]
fn recursive_results_are_cold_warm_and_root_order_independent() {
    let declarations = SourceInput::new(
        "models.ts",
        Arc::<str>::from(
            r#"
                interface Stream<Item> { next: Stream<Item[]> }
                interface Exposed<Item> { next: Exposed<Item[]>; value: Item }
                interface Cedar { cedar: unknown }
                interface Birch { birch: unknown }
                type CedarStream = Stream<Cedar>;
                type BirchStream = Stream<Birch>;
            "#,
        ),
    );
    let hidden = SourceInput::new(
        "hidden.ts",
        Arc::<str>::from(
            r#"
                declare let hiddenTarget: CedarStream;
                declare let hiddenSource: BirchStream;
                hiddenTarget = hiddenSource;
            "#,
        ),
    );
    let exposed = SourceInput::new(
        "exposed.ts",
        Arc::<str>::from(
            r#"
                declare let exposedTarget: Exposed<Cedar>;
                declare let exposedSource: Exposed<Birch>;
                exposedTarget = exposedSource;
            "#,
        ),
    );
    let compiler = Compiler::new();
    let run = |inputs| compiler.compile(inputs, &options());
    // The same checker first sees the hidden assumption and then the exposed
    // mismatch, and the second root order reverses those semantic demands.
    let forward_inputs = vec![declarations.clone(), hidden.clone(), exposed.clone()];
    let reverse_inputs = vec![exposed, hidden, declarations];
    let cold = run(forward_inputs.clone());
    let warm = run(forward_inputs);
    let reversed = run(reverse_inputs);
    let fingerprint = |output: &tsz::CompileOutput| {
        (
            serde_json::to_vec(&output.diagnostics).unwrap(),
            output.semantic_completion,
            output.exit_status,
            output.stats.types,
        )
    };

    assert!(cold.diagnostics.is_empty(), "{:?}", cold.diagnostics);
    assert_eq!(cold.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(cold.exit_status, CompileExitStatus::SemanticIncomplete);
    assert_eq!(fingerprint(&cold), fingerprint(&warm));
    assert_eq!(fingerprint(&cold), fingerprint(&reversed));
    assert!(
        cold.stats.types < 768,
        "unbounded type growth: {:?}",
        cold.stats
    );
}

#[test]
fn interface_heritage_rejects_shapes_outside_the_property_only_boundary() {
    let supported = compile(
        r#"
            interface Parent<Payload> { entry: Payload }
            interface Child<Subject> extends Parent<Subject> {}
            declare let child: Child<string>;
            child.entry;
        "#,
    );
    assert!(
        supported.diagnostics.is_empty(),
        "{:?}",
        supported.diagnostics
    );
    assert_complete(&supported);
    assert_eq!(supported.exit_status, CompileExitStatus::Success);

    let wrong_root_arity = compile(
        "interface B<T>{value:T} interface D<T> extends B<T>{} declare let d:D<string,number>; d.value;",
    );
    assert!(wrong_root_arity.diagnostics.is_empty());
    assert_eq!(
        wrong_root_arity.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        wrong_root_arity.exit_status,
        CompileExitStatus::SemanticIncomplete
    );

    let deferred = [
        // Transitive inheritance needs a recursive base-list query.
        "interface A<T>{value:T} interface B<T> extends A<T>{} interface D<T> extends B<T>{} declare let d:D<string>;",
        // Callable, index, and alias bases have separate relation owners.
        "interface B<T>{(value:T):T} interface D<T> extends B<T>{} declare let d:D<string>;",
        "interface B<T>{[key:string]:T} interface D<T> extends B<T>{} declare let d:D<string>;",
        "type B<T>={value:T}; interface D<T> extends B<T>{} declare let d:D<string>;",
        // Only exact, unconstrained positional generic pass-through is owned.
        "interface B<T>{value:T} interface D<T> extends B<T[]>{} declare let d:D<string>;",
        "interface B<T extends unknown>{value:T} interface D<T extends unknown> extends B<T>{} declare let d:D<string>;",
        // Pinned expanding-inheritance control: bounded, but override checking
        // is not claimed by this narrow heritage checkpoint.
        "interface A<T>{x:A<B<T>>} interface B<T> extends A<T>{x:B<A<T>>} declare let b:B<string>;",
        // Same-file interface merging must not masquerade as one declaration.
        "interface B<T>{value:T} interface D<T> extends B<T>{} interface D<T> extends B<T>{} declare let d:D<string>;",
    ];
    for source in deferred {
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
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }

    let cross_file_merge = Compiler::new().compile(
        vec![
            SourceInput::new("base.ts", Arc::<str>::from("interface B<T>{value:T}")),
            SourceInput::new(
                "first.ts",
                Arc::<str>::from("interface D<T> extends B<T>{}"),
            ),
            SourceInput::new(
                "second.ts",
                Arc::<str>::from("interface D<T> extends B<T>{}"),
            ),
            SourceInput::new("use.ts", Arc::<str>::from("declare let d:D<string>;")),
        ],
        &options(),
    );
    assert!(cross_file_merge.diagnostics.is_empty());
    assert_eq!(
        cross_file_merge.semantic_completion,
        SemanticCompletion::Deferred
    );

    for inputs in [
        vec![
            SourceInput::new(
                "cycle.ts",
                Arc::<str>::from("interface D<T> extends D<T>{}"),
            ),
            SourceInput::new(
                "base.ts",
                Arc::<str>::from("interface B<T>{value:T} interface D<T> extends B<T>{}"),
            ),
            SourceInput::new("use.ts", Arc::<str>::from("declare let d:D<string>;")),
        ],
        vec![
            SourceInput::new(
                "base.ts",
                Arc::<str>::from("interface B<T>{value:T} interface D<T> extends B<T>{}"),
            ),
            SourceInput::new(
                "cycle.ts",
                Arc::<str>::from("interface D<T> extends D<T>{}"),
            ),
            SourceInput::new("use.ts", Arc::<str>::from("declare let d:D<string>;")),
        ],
    ] {
        let output = Compiler::new().compile(inputs, &options());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }

    for source in [
        "interface B<T,T>{value:T} interface D<U> extends B<U>{} declare let d:D<string>;",
        "interface B<T>{value:T} interface D<U,U> extends B<U,U>{} declare let d:D<string,string>;",
    ] {
        let output = compile(source);
        assert_ne!(output.semantic_completion, SemanticCompletion::Complete);
        assert!(output.stats.types < 512, "{source}: {:?}", output.stats);
    }

    for source in [
        "interface Direct extends Direct {} declare let direct:Direct;",
        "interface Left extends Right {} interface Right extends Left {} declare let left:Left;",
    ] {
        let output = compile(source);
        assert!(
            output.diagnostics.is_empty(),
            "{source}: {:?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Cycle,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }

    let contextual_argument = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from(
                "declare function map<T>(cb:(x:T)=>void):void;map<number>(renamed=>renamed.toFixed());",
            ),
        )],
        &CompilerOptions {
            no_emit: true,
            strict: true,
            ..CompilerOptions::default()
        },
    );
    assert!(
        contextual_argument.diagnostics.is_empty(),
        "{:?}",
        contextual_argument.diagnostics
    );
    assert_eq!(
        contextual_argument.semantic_completion,
        SemanticCompletion::Deferred
    );
}

#[test]
fn generic_call_type_arguments_keep_type_scope_and_relational_grammar() {
    let nested = compile(
        r#"
            type Envelope<Value> = { value: Value };
            declare function invoke<Input>(value: Input): void;
            declare function invoke(): void;
            function outer<Subject>(value: Subject) {
                invoke<Envelope<Subject[]>>(value);
            }
        "#,
    );
    assert!(nested.diagnostics.is_empty(), "{:?}", nested.diagnostics);
    assert_eq!(nested.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(nested.exit_status, CompileExitStatus::SemanticIncomplete);

    // TS7 owns TS2558, TS2347, and instantiated TS2345 here. Until the call
    // query carries generic signatures, all explicit type-argument calls are
    // honest nonclaims rather than calls with silently ignored arguments.
    for source in [
        "function plain(x:number):void{} plain<string>(1);",
        "declare let anyFn:any; anyFn<string>();",
        "function id<T>(x:T):T{return x} id<string>('ok');",
        "function id<T>(x:T):T{return x} id<string>(1);",
        // Value-only names require TS2749 rather than the resolver's TS2304.
        // Until that diagnostic is owned, the explicit call is a nonclaim.
        "declare let callable:any;const valueOnly=1;callable < valueOnly > (1);",
        "type Middle=number;declare let callable:any;callable < Middle > (1);",
    ] {
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
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }

    let relational =
        compile("const left=1;const middle=2;const right=3;left < middle && middle > right;");
    assert!(
        relational.diagnostics.is_empty(),
        "{:?}",
        relational.diagnostics
    );
    assert_complete(&relational);

    let source = "declare function invoke<X>(value:X):void;function outer<Subject>(value:Subject){invoke<Subject>(value);}";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", Arc::<str>::from(source));
    let positions = source
        .match_indices("Subject")
        .map(|(position, _)| position as u32)
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 3);
    for reference in &positions[1..] {
        let definition = service
            .definition_and_bound_span("case.ts", *reference + 1)
            .expect("function type-parameter definition");
        assert_eq!(definition.definitions[0].text_span.start, positions[0]);
    }
    assert_eq!(
        service.references("case.ts", positions[0] + 1)[0]
            .references
            .len(),
        3
    );
    assert_eq!(
        service.rename("case.ts", positions[0] + 1).locations.len(),
        3
    );

    let class_source = "declare function invoke<X>(value:X):void;class Container<Element>{run(value:Element):void{invoke<Element>(value);}}";
    let mut class_service = LanguageService::new(CompilerOptions::default());
    class_service.open("case.ts", Arc::<str>::from(class_source));
    let class_positions = class_source
        .match_indices("Element")
        .map(|(position, _)| position as u32)
        .collect::<Vec<_>>();
    assert_eq!(class_positions.len(), 3);
    for reference in &class_positions[1..] {
        let definition = class_service
            .definition_and_bound_span("case.ts", *reference + 1)
            .expect("class type-parameter definition");
        assert_eq!(
            definition.definitions[0].text_span.start,
            class_positions[0]
        );
    }
    assert_eq!(
        class_service.references("case.ts", class_positions[0] + 1)[0]
            .references
            .len(),
        3
    );
    assert_eq!(
        class_service
            .rename("case.ts", class_positions[0] + 1)
            .locations
            .len(),
        3
    );

    let factory_source = r#"
        declare function factory<T>(): { value: T };
        const made: { value: string } = factory<string>();
        factory<string>().value;
    "#;
    let factory = compile(factory_source);
    assert!(factory.diagnostics.is_empty(), "{:?}", factory.diagnostics);
    assert_eq!(factory.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(factory.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(factory.stats.types < 512, "{:?}", factory.stats);
}

#[test]
fn generic_call_recovery_keeps_presence_and_close_commit_facts() {
    let parse = |text: &str| {
        let source = SourceText::new(
            FileId(0),
            "case.ts".into(),
            Arc::<str>::from(text.to_string()),
        );
        parse_source(&source)
    };

    let empty = parse("foo<>();");
    assert_eq!(codes_from_diagnostics(&empty.diagnostics), vec![1099]);
    let StatementKind::Expression(empty_call) = &empty.unit.statements[0].kind else {
        panic!("empty type arguments did not remain an expression");
    };
    assert!(matches!(
        &empty_call.kind,
        ExpressionKind::Call {
            type_arguments: Some(arguments),
            ..
        } if arguments.is_empty()
    ));

    let malformed = parse("Foo<a,,b>();");
    assert!(
        codes_from_diagnostics(&malformed.diagnostics).contains(&1110),
        "{:?}",
        malformed.diagnostics
    );
    let StatementKind::Expression(malformed_call) = &malformed.unit.statements[0].kind else {
        panic!("malformed type arguments did not remain an expression");
    };
    assert!(matches!(
        malformed_call.kind,
        ExpressionKind::Call {
            type_arguments: Some(_),
            ..
        }
    ));

    let missing_close = parse("f<T(x);");
    let StatementKind::Expression(relational) = &missing_close.unit.statements[0].kind else {
        panic!("missing type close did not remain an expression");
    };
    assert!(matches!(relational.kind, ExpressionKind::Binary { .. }));

    let missing_paren = parse("f<T>(x;");
    assert!(
        codes_from_diagnostics(&missing_paren.diagnostics).contains(&1005),
        "{:?}",
        missing_paren.diagnostics
    );
    let StatementKind::Expression(committed_call) = &missing_paren.unit.statements[0].kind else {
        panic!("closed type arguments did not commit the call");
    };
    assert!(matches!(
        committed_call.kind,
        ExpressionKind::Call {
            type_arguments: Some(_),
            ..
        }
    ));

    let emitted = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("function foo<T>(){}foo<>();"),
        )],
        &CompilerOptions {
            no_check: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert_eq!(codes(&emitted), vec![1099]);
    assert!(emitted.emitted_files[0].text.contains("foo();"));
}

#[test]
fn generic_call_emit_erases_types_without_reclassifying_relational_js() {
    let typescript = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from(
                "type Box<T>={value:T};declare function generic<A,B>(value:number):void;declare function outer(value:unknown):void;declare function takes<T>(value:unknown):void;declare const fn:unknown;const left=1;const middle=2;generic<string,number>(7);outer(generic<string,number>(7));takes<(x:string)=>Box<Box<string>>>(fn);outer(generic < left, middle > 7);outer(generic < left, middle > +(7));",
            ),
        )],
        &CompilerOptions {
            no_check: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(
        typescript.diagnostics.is_empty(),
        "{:?}",
        typescript.diagnostics
    );
    let javascript = &typescript.emitted_files[0].text;
    assert!(javascript.contains("generic(7);"), "{javascript}");
    assert!(javascript.contains("outer(generic(7));"), "{javascript}");
    assert!(javascript.contains("takes(fn);"), "{javascript}");
    assert!(
        javascript.contains("outer(generic < left, middle > 7);"),
        "{javascript}"
    );
    assert!(
        javascript.contains("outer(generic < left, middle > +(7));"),
        "{javascript}"
    );
    assert!(!javascript.contains("generic<string"), "{javascript}");

    let javascript_source = "const Foo=1;const middle=2;Foo < middle > (1);";
    let javascript_output = Compiler::new().compile(
        vec![SourceInput::new(
            "case.jsx",
            Arc::<str>::from(javascript_source),
        )],
        &CompilerOptions {
            allow_js: true,
            no_check: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(
        javascript_output.diagnostics.is_empty(),
        "{:?}",
        javascript_output.diagnostics
    );
    let emitted = &javascript_output.emitted_files[0].text;
    assert!(emitted.contains("Foo < middle > (1);"), "{emitted}");
    assert!(!emitted.contains("Foo(1)"), "{emitted}");
}

#[test]
fn explicit_generic_calls_block_only_the_unowned_declaration_product() {
    let source = "declare function generic<T>():T;export function outer<T>():void{generic<T>();}export const result=generic<string>();";
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
            &CompilerOptions {
                no_check,
                declaration: true,
                target: "esnext".to_string(),
                module: "esnext".to_string(),
                ..CompilerOptions::default()
            },
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.emitted_files.iter().any(|file| !file.declaration));
        assert!(!output.emitted_files.iter().any(|file| file.declaration));
        let javascript = output
            .emitted_files
            .iter()
            .find(|file| !file.declaration)
            .unwrap();
        assert!(
            javascript.text.contains("generic();"),
            "{}",
            javascript.text
        );
        assert!(!javascript.text.contains("generic<"), "{}", javascript.text);
    }

    let owned = Compiler::new().compile(
        vec![SourceInput::new(
            "owned.ts",
            Arc::<str>::from("export function outer<T>():void{}"),
        )],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(owned.emitted_files.iter().any(|file| file.declaration));

    let runtime_source = "declare function f<T>():void;f<string>();";
    for (path, module, expected) in [
        ("runtime.ts", "esnext", "\"use strict\";\nf();\n"),
        ("runtime.ts", "commonjs", "\"use strict\";\nf();\n"),
        (
            "runtime.cts",
            "nodenext",
            "\"use strict\";\nObject.defineProperty(exports, \"__esModule\", { value: true });\nf();\n",
        ),
    ] {
        for no_check in [false, true] {
            let output = Compiler::new().compile(
                vec![SourceInput::new(path, Arc::<str>::from(runtime_source))],
                &CompilerOptions {
                    no_check,
                    declaration: true,
                    target: "esnext".to_string(),
                    module: module.to_string(),
                    ..CompilerOptions::default()
                },
            );
            assert!(output.diagnostics.is_empty(), "{path}/{module}/{no_check}");
            assert_eq!(output.emitted_files.len(), 1, "{path}/{module}/{no_check}");
            assert!(!output.emitted_files[0].declaration);
            assert_eq!(
                output.emitted_files[0].text, expected,
                "{path}/{module}/{no_check}"
            );
        }
    }
}

#[test]
fn productive_object_alias_completion_is_independent_of_strictness_and_declaration_form() {
    let cases = [
        ("alias-only", "type Link={next:Link};"),
        ("ordinary-let", "type Link={next:Link}; let link:Link;"),
        (
            "ambient-let",
            "type Link={next:Link}; declare let link:Link;",
        ),
        ("renamed", "type Branch={child:Branch}; let branch:Branch;"),
        (
            "nested-boundary",
            "type Branch={edge:{child:Branch}}; let branch:Branch;",
        ),
    ];
    let mut incomplete = Vec::new();
    for strict in [false, true] {
        for (name, source) in cases {
            let output = compile_with_strictness(source, strict);
            if !output.diagnostics.is_empty()
                || output.semantic_completion != SemanticCompletion::Complete
            {
                incomplete.push((
                    strict,
                    name,
                    codes(&output),
                    output.semantic_completion,
                    output.exit_status,
                ));
            }
        }
    }
    assert!(incomplete.is_empty(), "incomplete cases: {incomplete:?}");
}

#[test]
fn productive_aliases_do_not_bypass_ambient_merge_or_invalid_root_gates() {
    // `Node` is already owned by the selected default DOM library. Pinned TS7
    // reports TS2300 for this collision; duplicate-symbol diagnostics are not
    // modeled yet, so the rewrite must stay Deferred instead of treating the
    // program alias as a sole definitive recursion root.
    for strict in [false, true] {
        for source in [
            "type Node={next:Node};",
            "type Node={next:Node}; let node:Node;",
            "type Node={next:Node}; declare let node:Node;",
        ] {
            let output = compile_with_strictness(source, strict);
            assert!(output.diagnostics.is_empty(), "{strict}/{source}");
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "{strict}/{source}"
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        }

        // Transparent self-cycles remain invalid independently of declaration
        // form, binder spelling, and a nested alias-instantiation wrapper.
        for source in [
            "type Loop=Loop; let loop:Loop;",
            "type Loop=Loop; declare let loop:Loop;",
            "type Recurrence=Recurrence; let recurrence:Recurrence;",
            "type Identity<Value>=Value; type Loop=Identity<Loop>; declare let loop:Loop;",
        ] {
            let output = compile_with_strictness(source, strict);
            assert_eq!(codes(&output), vec![2456], "{strict}/{source}");
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Cycle,
                "{strict}/{source}"
            );
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        }
    }
}
