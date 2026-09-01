use std::sync::Arc;

use tsz::diagnostics::{DiagnosticCategory, RelatedInformation};
use tsz::service::LanguageService;
use tsz::{CompilerOptions, SemanticCompletion};

type Fingerprint = (
    String,
    u32,
    u32,
    u32,
    DiagnosticCategory,
    String,
    Vec<(String, u32, u32, u32, String, u32)>,
);

fn related_fingerprint(
    related: &[RelatedInformation],
) -> Vec<(String, u32, u32, u32, String, u32)> {
    related
        .iter()
        .map(|related| {
            (
                related.file.clone(),
                related.code,
                related.start,
                related.length,
                related.message_text.clone(),
                related.depth,
            )
        })
        .collect()
}

fn analyze(path: &str, source: &str) -> (SemanticCompletion, Vec<Fingerprint>) {
    analyze_files(&[(path, source)], path)
}

fn analyze_files(files: &[(&str, &str)], path: &str) -> (SemanticCompletion, Vec<Fingerprint>) {
    let mut service = LanguageService::new(CompilerOptions {
        no_emit: true,
        strict: true,
        target: "es2015".to_string(),
        ..CompilerOptions::default()
    });
    for (path, source) in files {
        service.open(*path, Arc::<str>::from(*source));
    }
    let result = service.semantic_diagnostics(path);
    (
        result.semantic_completion,
        result
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.file.clone(),
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.category,
                    diagnostic.message_text.clone(),
                    related_fingerprint(&diagnostic.related_information),
                )
            })
            .collect(),
    )
}

fn missing(path: &str, source: &str, name: &str) -> Fingerprint {
    (
        path.to_string(),
        2304,
        source.find(name).expect("missing-name occurrence") as u32,
        name.len() as u32,
        DiagnosticCategory::Error,
        format!("Cannot find name '{name}'."),
        Vec::new(),
    )
}

fn assignment(path: &str, source: &str, name: &str, message: &str) -> Fingerprint {
    (
        path.to_string(),
        2322,
        source.find(name).expect("assignment occurrence") as u32,
        name.len() as u32,
        DiagnosticCategory::Error,
        message.to_string(),
        Vec::new(),
    )
}

fn union_assignment(path: &str, source: &str, name: &str) -> Fingerprint {
    (
        path.to_string(),
        2322,
        source.find(name).expect("union assignment occurrence") as u32,
        name.len() as u32,
        DiagnosticCategory::Error,
        "Type 'string | number' is not assignable to type 'string'.".to_string(),
        vec![(
            String::new(),
            2322,
            0,
            0,
            "Type 'number' is not assignable to type 'string'.".to_string(),
            1,
        )],
    )
}

fn call_argument(
    path: &str,
    source: &str,
    call: &str,
    argument: &str,
    message: &str,
    related: &[&str],
) -> Fingerprint {
    let call_start = source.find(call).expect("call occurrence");
    (
        path.to_string(),
        2345,
        (call_start + call.find(argument).expect("argument in call")) as u32,
        argument.len() as u32,
        DiagnosticCategory::Error,
        message.to_string(),
        related
            .iter()
            .enumerate()
            .map(|(index, message)| {
                (
                    String::new(),
                    2345,
                    0,
                    0,
                    (*message).to_string(),
                    index as u32 + 1,
                )
            })
            .collect(),
    )
}

#[test]
fn flow_region_withholds_container_suffix_but_keeps_function_like_holes() {
    for (path, binder, discriminant) in [
        ("flow-region.ts", "value", "value."),
        ("renamed-flow-region.ts", "renamed", "((renamed))."),
    ] {
        let source = format!(
            concat!(
                "declare function takeText(input:string):void;\n",
                "function region({binder}:string|number){{\n",
                "  const before:string={binder};\n",
                "  switch({discriminant}){{case \"text\":\n",
                "    takeText({binder});\n",
                "    const hiddenCase:string=1; MissingCase;\n",
                "  }}\n",
                "  const hiddenAfter:string=1; MissingAfter;\n",
                "  const arrow=():string=>{{\n",
                "    const arrowWrong:string=1;\n",
                "    MissingArrow;\n",
                "    return 1;\n",
                "  }};\n",
                "  function nested(){{\n",
                "    const nestedWrong:string=1;\n",
                "    MissingNested;\n",
                "  }}\n",
                "  class Boundary{{renamed(callback=()=>{{\n",
                "    const classArrowWrong:string=1;\n",
                "    MissingClassArrow;\n",
                "  }}){{\n",
                "    const classBodyWrong:string=1;\n",
                "    MissingClassBody;\n",
                "  }}}}\n",
                "  const hiddenResume:string=1; MissingResume;\n",
                "}}\n",
                "function independent({binder}:string|number){{\n",
                "  const outside:string={binder};\n",
                "  MissingOutside;\n",
                "}}\n",
            ),
            binder = binder,
            discriminant = discriminant,
        );
        let expected = vec![
            union_assignment(path, &source, "before"),
            missing(path, &source, "MissingCase"),
            missing(path, &source, "MissingAfter"),
            assignment(
                path,
                &source,
                "arrowWrong",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(path, &source, "MissingArrow"),
            assignment(
                path,
                &source,
                "return",
                "Type 'number' is not assignable to type 'string'.",
            ),
            assignment(
                path,
                &source,
                "nestedWrong",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(path, &source, "MissingNested"),
            assignment(
                path,
                &source,
                "classArrowWrong",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(path, &source, "MissingClassArrow"),
            assignment(
                path,
                &source,
                "classBodyWrong",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(path, &source, "MissingClassBody"),
            missing(path, &source, "MissingResume"),
            union_assignment(path, &source, "outside"),
            missing(path, &source, "MissingOutside"),
        ];

        let first = analyze(path, &source);
        let repeated = analyze(path, &source);
        assert_eq!(first, repeated, "{path}");
        assert_eq!(first.0, SemanticCompletion::Deferred, "{path}");
        assert_eq!(first.1, expected, "{path}");
    }
}

#[test]
fn recovered_switch_region_withholds_only_its_flow_container() {
    let source = concat!(
        "function shell(value:{tag:string}){\n",
        "  const before:string=1;\n",
        "  switch(value.){default:\n",
        "    const hiddenInside:string=1; MissingInside;\n",
        "  }\n",
        "  const hiddenAfter:string=1; MissingAfter;\n",
        "  const boundary=()=>{MissingArrowBoundary;};\n",
        "}\n",
        "MissingOutsideBoundary;\n",
    );
    let path = "recovered-flow-region.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![
            assignment(
                path,
                source,
                "before",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(path, source, "MissingInside"),
            missing(path, source, "MissingAfter"),
            missing(path, source, "MissingArrowBoundary"),
            missing(path, source, "MissingOutsideBoundary"),
        ],
    );
}

#[test]
fn predicate_if_region_withholds_both_branches_and_container_suffix() {
    for (path, predicate, condition) in [
        (
            "predicate-region.ts",
            "isText",
            "MissingCondition&&(((isText)))(value)&&(null as any)`head${\"gap\"}tail`",
        ),
        (
            "renamed-predicate-region.ts",
            "renamedPredicate",
            "MissingCondition&&((renamedPredicate))((value))&&(null as any)`head${\"gap\"}tail`",
        ),
    ] {
        let source = format!(
            concat!(
                "declare function {predicate}(input:string|number):input is string;\n",
                "function shell(value:string|number){{\n",
                "  const before:string=value;\n",
                "  if({condition}){{\n",
                "    const hiddenThen:string=1; MissingThen;\n",
                "  }}else{{\n",
                "    const hiddenElse:string=1; MissingElse;\n",
                "  }}\n",
                "  const hiddenAfter:string=1; MissingAfterIf;\n",
                "  const boundary=()=>{{MissingIfArrow;}};\n",
                "  function independentBoundary(){{MissingIfFunction;}}\n",
                "  const hiddenResume:string=1; MissingIfResume;\n",
                "}}\n",
                "MissingIfOutside;\n",
            ),
            predicate = predicate,
            condition = condition,
        );
        let expected = vec![
            union_assignment(path, &source, "before"),
            missing(path, &source, "MissingCondition"),
            missing(path, &source, "MissingThen"),
            missing(path, &source, "MissingElse"),
            missing(path, &source, "MissingAfterIf"),
            missing(path, &source, "MissingIfArrow"),
            missing(path, &source, "MissingIfFunction"),
            missing(path, &source, "MissingIfResume"),
            missing(path, &source, "MissingIfOutside"),
        ];

        let first = analyze(path, &source);
        assert_eq!(first, analyze(path, &source), "{path}");
        assert_eq!(first.0, SemanticCompletion::Deferred, "{path}");
        assert_eq!(first.1, expected, "{path}");
    }
}

#[test]
fn valid_predicate_branches_keep_missing_name_diagnostics() {
    let source = concat!(
        "declare function isText(input:string|number):input is string;\n",
        "function inferred(value:string|number){\n",
        "  if(isText(value)){return MissingInside;}else{return MissingElse;}\n",
        "}\n",
        "MissingOutsideReturn;\n",
    );
    let path = "flow-return-inference.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![
            missing(path, source, "MissingInside"),
            missing(path, source, "MissingElse"),
            missing(path, source, "MissingOutsideReturn"),
        ],
    );
}

#[test]
fn flow_region_is_local_across_files_and_root_orders() {
    let region_source = concat!(
        "function region(value:string|number){\n",
        "  switch((value).){default:const hiddenInside:string=1;}\n",
        "  const hiddenSuffix:string=1; MissingSuffix;\n",
        "}\n",
    );
    let independent_source = concat!("const outsideWrong:string=1;\n", "MissingOtherFile;\n",);
    let region_path = "a-region.ts";
    let independent_path = "b-independent.ts";
    let expected_independent = vec![
        assignment(
            independent_path,
            independent_source,
            "outsideWrong",
            "Type 'number' is not assignable to type 'string'.",
        ),
        missing(independent_path, independent_source, "MissingOtherFile"),
    ];

    for files in [
        [
            (region_path, region_source),
            (independent_path, independent_source),
        ],
        [
            (independent_path, independent_source),
            (region_path, region_source),
        ],
    ] {
        assert_eq!(
            analyze_files(&files, region_path),
            (
                SemanticCompletion::Deferred,
                vec![missing(region_path, region_source, "MissingSuffix")],
            ),
        );
        assert_eq!(
            analyze_files(&files, independent_path),
            (SemanticCompletion::Complete, expected_independent.clone()),
        );
    }
}

#[test]
fn cross_file_declaration_values_close_dependent_expression_demands() {
    let producer_source = concat!(
        "const pivot:string|number=1;\n",
        "switch((pivot).){}\n",
        "const produced=pivot;\n",
    );
    let consumer_source = concat!(
        "const alias=produced+1;\n",
        "const wrong:string=alias;\n",
        "const kept:MissingCrossFileValue=1;\n",
    );
    let producer_path = "a-producer.ts";
    let consumer_path = "b-consumer.ts";
    let expected_consumer = vec![missing(
        consumer_path,
        consumer_source,
        "MissingCrossFileValue",
    )];

    for files in [
        [
            (producer_path, producer_source),
            (consumer_path, consumer_source),
        ],
        [
            (consumer_path, consumer_source),
            (producer_path, producer_source),
        ],
    ] {
        assert_eq!(
            analyze_files(&files, producer_path),
            (SemanticCompletion::Deferred, Vec::new()),
        );
        assert_eq!(
            analyze_files(&files, consumer_path),
            (SemanticCompletion::Deferred, expected_consumer.clone()),
        );
    }
}

#[test]
fn flow_aliases_and_assignments_propagate_without_losing_fixed_call_results() {
    let source = concat!(
        "declare function isText(value:string|number):value is string;\n",
        "declare function useText(value:string):void;\n",
        "declare function fixedReturn(value:string):number;\n",
        "declare function genericIdentity<T>(value:T):T;\n",
        "function shell(value:string|number){\n",
        "  let assigned:string|number=0;\n",
        "  let genericTarget:string|number=0;\n",
        "  let fixedTarget:number=0;\n",
        "  if(isText(value)){\n",
        "    const alias=value; useText(alias);\n",
        "    assigned=value; useText(assigned);\n",
        "    genericTarget=genericIdentity(value);\n",
        "    fixedTarget=fixedReturn(value);\n",
        "    const fixed=fixedReturn(value);\n",
        "    const wrongFixed:string=fixed;\n",
        "    const asserted=(fixedReturn(value) as number);\n",
        "    const wrongAsserted:string=asserted;\n",
        "    const shaped={count:fixedReturn(value)};\n",
        "    const wrongShaped:string=shaped.count;\n",
        "    const kept:MissingFlowSibling=1;\n",
        "  }\n",
        "  useText(assigned);\n",
        "  const genericBad:string=genericTarget;\n",
        "  const fixedBad:string=fixedTarget;\n",
        "}\n",
    );
    let path = "flow-alias.ts";
    let post_assigned =
        source.rfind("useText(assigned)").expect("post-if call") as u32 + "useText(".len() as u32;
    let expected = vec![
        assignment(
            path,
            source,
            "wrongFixed",
            "Type 'number' is not assignable to type 'string'.",
        ),
        assignment(
            path,
            source,
            "wrongAsserted",
            "Type 'number' is not assignable to type 'string'.",
        ),
        assignment(
            path,
            source,
            "wrongShaped",
            "Type 'number' is not assignable to type 'string'.",
        ),
        missing(path, source, "MissingFlowSibling"),
        (
            path.to_string(),
            2345,
            post_assigned,
            "assigned".len() as u32,
            DiagnosticCategory::Error,
            "Argument of type 'string | number' is not assignable to parameter of type 'string'."
                .to_string(),
            vec![(
                String::new(),
                2345,
                0,
                0,
                "Type 'number' is not assignable to type 'string'.".to_string(),
                1,
            )],
        ),
        union_assignment(path, source, "genericBad"),
        assignment(
            path,
            source,
            "fixedBad",
            "Type 'number' is not assignable to type 'string'.",
        ),
    ];
    for _ in 0..2 {
        let (completion, diagnostics) = analyze(path, source);
        assert_eq!(completion, SemanticCompletion::Complete);
        assert_eq!(diagnostics, expected);
    }
}

#[test]
fn direct_call_assignment_sources_defer_outside_the_exact_model() {
    for (path, declaration, call, missing_name) in [
        (
            "flow-call-explicit.ts",
            "declare function generic<T>(value:T):T;",
            "generic<string>(value)",
            "MissingExplicitCallFlow",
        ),
        (
            "flow-call-overload.ts",
            "declare function choose(value:string):string;declare function choose(value:number):number;",
            "choose(value)",
            "MissingOverloadCallFlow",
        ),
        (
            "flow-call-constrained.ts",
            "declare function constrained<T extends string|number>(value:T):T;",
            "constrained(value)",
            "MissingConstrainedCallFlow",
        ),
        (
            "flow-call-optional.ts",
            "declare function optional(value?:string):number;",
            "optional(value)",
            "MissingOptionalCallFlow",
        ),
        (
            "flow-call-rest.ts",
            "declare function rest(...values:string[]):number;",
            "rest(value)",
            "MissingRestCallFlow",
        ),
        (
            "flow-call-arity.ts",
            "declare function arity(value:string,suffix?:string):number;",
            "arity(value)",
            "MissingArityCallFlow",
        ),
    ] {
        let source = format!(
            concat!(
                "declare function isText(value:string|number):value is string;\n",
                "declare function takeText(value:string):void;{declaration}\n",
                "function shell(value:string|number){{let target:string|number=0;\n",
                "  if(isText(value)){{target={call};takeText(target);}}\n",
                "  const kept:{missing_name}=1;}}\n",
            ),
            declaration = declaration,
            call = call,
            missing_name = missing_name,
        );
        let first = analyze(path, &source);
        assert_eq!(first, analyze(path, &source), "{path}");
        assert_eq!(first.0, SemanticCompletion::Deferred, "{path}");
        assert_eq!(
            first.1,
            vec![missing(path, &source, missing_name)],
            "{path}"
        );
    }
}

#[test]
fn assignment_sources_defer_for_self_calls_and_inexact_union_members() {
    for (path, source, missing_name) in [
        (
            "flow-call-self.ts",
            concat!(
                "declare function isText(value:string|number):value is string;\n",
                "declare function fixed(value:string):number;declare function takeText(value:string):void;\n",
                "function shell(value:string|number){let target:string|number=value;\n",
                "if(isText(target)){target=fixed(target);takeText(target);}\n",
                "const kept:MissingSelfCallFlow=1;}\n",
            ),
            "MissingSelfCallFlow",
        ),
        (
            "flow-structural-source.ts",
            concat!(
                "interface A{a:string}declare let source:{a:string;b:number};\n",
                "function shell(tag:\"yes\"|\"no\"){let target:A|number=0;\n",
                "if(tag===\"yes\"){target=source;target.b;}const kept:MissingStructuralFlow=1;}\n",
            ),
            "MissingStructuralFlow",
        ),
        (
            "flow-union-source.ts",
            concat!(
                "declare let source:number|string;declare function takeText(value:string):void;\n",
                "function shell(tag:\"yes\"|\"no\"){let target:string|number|boolean=false;\n",
                "if(tag===\"yes\"){target=source;takeText(target);}const kept:MissingUnionFlow=1;}\n",
            ),
            "MissingUnionFlow",
        ),
        (
            "flow-boolean-source.ts",
            concat!(
                "function shell(tag:\"yes\"|\"no\"){let target:true|string=\"\";\n",
                "if(tag===\"yes\"){target=true;}const exact:true=target;const kept:MissingBooleanFlow=1;}\n",
            ),
            "MissingBooleanFlow",
        ),
    ] {
        let first = analyze(path, source);
        assert_eq!(first, analyze(path, source), "{path}");
        assert_eq!(first.0, SemanticCompletion::Deferred, "{path}");
        assert_eq!(first.1, vec![missing(path, source, missing_name)], "{path}");
    }
}

#[test]
fn predicate_subject_paths_cover_both_branches_but_not_sibling_properties() {
    for (path, binder, wrapper) in [
        ("member.ts", "box", "box.value"),
        ("renamed-member.ts", "crateValue", "((crateValue.value))"),
    ] {
        let source = format!(
            concat!(
                "interface Box{{value:string|number;other:string|number}}\n",
                "declare function hasText(value:string|number):value is string;\n",
                "function inspect({binder}:Box){{\n",
                "  if(hasText({wrapper})){{\n",
                "    const yes:string={binder}.value;\n",
                "    const otherThen:string={binder}.other;\n",
                "  }}else{{\n",
                "    const no:number={binder}.value;\n",
                "    const otherElse:string={binder}.other;\n",
                "  }}\n",
                "  const after:string={binder}.value;\n",
                "}}\n",
            ),
            binder = binder,
            wrapper = wrapper,
        );
        let (completion, diagnostics) = analyze(path, &source);
        assert_eq!(completion, SemanticCompletion::Complete, "{path}");
        assert_eq!(
            diagnostics,
            vec![
                union_assignment(path, &source, "otherThen"),
                union_assignment(path, &source, "otherElse"),
                union_assignment(path, &source, "after"),
            ],
            "{path}",
        );
    }
}

#[test]
fn predicate_member_flow_separates_matching_sibling_and_container_consumers() {
    let declarations = concat!(
        "interface Box{value:string|number;other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "declare function takeText(value:string):void;\n",
        "declare function takeStringValue(value:{value:string}):void;\n",
    );
    for (path, root, subject) in [
        ("predicate-consumers.ts", "box", "box.value"),
        (
            "predicate-consumers-renamed.ts",
            "holder",
            "((((holder.value))))",
        ),
    ] {
        let source = format!(
            concat!(
                "function inspect({root}:Box){{\n",
                "  if(isText({subject})){{\n",
                "    takeText({root}.value);\n",
                "    takeText({root}.other);\n",
                "    takeStringValue({root});\n",
                "  }}\n",
                "}}\n",
            ),
            root = root,
            subject = subject,
        );
        let sibling_call = format!("takeText({root}.other)");
        let sibling = format!("{root}.other");
        let container_call = format!("takeStringValue({root})");
        let expected = vec![
            call_argument(
                path,
                &source,
                &sibling_call,
                &sibling,
                "Argument of type 'string | number' is not assignable to parameter of type 'string'.",
                &["Type 'number' is not assignable to type 'string'."],
            ),
            call_argument(
                path,
                &source,
                &container_call,
                root,
                "Argument of type 'Box' is not assignable to parameter of type '{ value: string; }'.",
                &[
                    "Types of property 'value' are incompatible.",
                    "Type 'string | number' is not assignable to type 'string'.",
                    "Type 'number' is not assignable to type 'string'.",
                ],
            ),
        ];
        for reversed in [false, true] {
            let files = if reversed {
                [
                    (path, source.as_str()),
                    ("predicate-consumers.d.ts", declarations),
                ]
            } else {
                [
                    ("predicate-consumers.d.ts", declarations),
                    (path, source.as_str()),
                ]
            };
            let first = analyze_files(&files, path);
            assert_eq!(
                first,
                analyze_files(&files, path),
                "{path}; reversed={reversed}"
            );
            assert_eq!(
                first.0,
                SemanticCompletion::Complete,
                "{path}; reversed={reversed}"
            );
            assert_eq!(first.1, expected, "{path}; reversed={reversed}");
        }
    }
}

#[test]
fn sibling_predicates_preserve_prior_path_flow_in_regions_and_return_suffixes() {
    for (path, root, value, other) in [
        ("predicate-sibling-flow.ts", "box", "value", "other"),
        (
            "predicate-sibling-flow-renamed.ts",
            "holder",
            "payload",
            "spare",
        ),
    ] {
        let source = format!(
            concat!(
                "interface Box{{{value}:string|number;{other}:string|number}}\n",
                "declare function isText(value:string|number):value is string;\n",
                "function shell({root}:Box){{\n",
                "  if((isText)((({root}.{value})))){{}}else{{return;}}\n",
                "  if((isText)((({root}.{other})))){{\n",
                "    const regionKept:string={root}.{value};\n",
                "  }}\n",
                "  if((isText)((({root}.{other})))){{return;}}\n",
                "  const suffixKept:string={root}.{value};\n",
                "}}\n",
            ),
            root = root,
            value = value,
            other = other,
        );
        let (completion, diagnostics) = analyze(path, &source);
        assert_eq!(completion, SemanticCompletion::Complete, "{path}");
        assert_eq!(diagnostics, Vec::new(), "{path}");
    }
}

#[test]
fn later_non_target_predicate_arguments_rebase_on_the_first_argument_effect() {
    for (path, root, predicate) in [
        ("predicate-multiarg.ts", "box", "firstIsText"),
        (
            "predicate-multiarg-renamed.ts",
            "holder",
            "renamedFirstIsText",
        ),
    ] {
        let source = format!(
            concat!(
                "interface Box{{value:string|number}}\n",
                "declare function {predicate}(target:string|number,ignored:string|number):target is string;\n",
                "function shell({root}:Box){{\n",
                "  if(({predicate})((({root}.value)),(({root}.value)))){{\n",
                "    const kept:string={root}.value;\n",
                "  }}\n",
                "}}\n",
            ),
            root = root,
            predicate = predicate,
        );
        let (completion, diagnostics) = analyze(path, &source);
        assert_eq!(completion, SemanticCompletion::Complete, "{path}");
        assert_eq!(diagnostics, Vec::new(), "{path}");
    }
}

#[test]
fn generic_predicate_nonclaims_do_not_erase_prior_disjoint_path_flow() {
    let source = concat!(
        "interface Box{value:string|number;other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "declare function genericIs<T>(target:T,ignored:T):target is T;\n",
        "function shell(box:Box){\n",
        "  if(isText(box.value)){}else{return;}\n",
        "  if(genericIs(box.other,box.other)){\n",
        "    const kept:string=box.value;\n",
        "    const dependent:string=box.other;\n",
        "    const independent:string=1;\n",
        "  }\n",
        "}\n",
    );
    let path = "predicate-nested-generic.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![assignment(
            path,
            source,
            "independent",
            "Type 'number' is not assignable to type 'string'.",
        )],
    );
}

#[test]
fn nested_switch_effects_rebase_on_prior_member_predicate_flow() {
    let source = concat!(
        "interface Box{value:string|number;kind:\"hit\"}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box){\n",
        "  if(isText(box.value)){}else{return;}\n",
        "  switch(box.kind){case \"hit\":\n",
        "    const kept:string=box.value;\n",
        "    break;\n",
        "  }\n",
        "}\n",
    );
    let path = "predicate-nested-switch.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(diagnostics, Vec::new());
}

#[test]
fn numeric_predicate_paths_keep_sibling_elements_distinct() {
    let source = concat!(
        "declare function isText(value:string|number):value is string;\n",
        "declare function takeText(value:string):void;\n",
        "function shell(pair:[string|number,number]){\n",
        "  if(isText(pair[0])){takeText(pair[1]);}\n",
        "}\n",
    );
    let path = "predicate-numeric-path.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Complete);
    let sibling = source.find("pair[1]").expect("sibling element") as u32;
    assert_eq!(
        diagnostics,
        vec![(
            path.to_string(),
            2345,
            sibling,
            "pair[1]".len() as u32,
            DiagnosticCategory::Error,
            "Argument of type 'number' is not assignable to parameter of type 'string'."
                .to_string(),
            Vec::new(),
        )],
    );
}

#[test]
fn matching_path_writes_preserve_rhs_flow_and_defer_only_later_dependents() {
    for (path, root, access) in [
        ("predicate-member-write.ts", "box", "box.value"),
        (
            "predicate-element-write-renamed.ts",
            "holder",
            "holder[\"value\"]",
        ),
    ] {
        let source = format!(
            concat!(
                "interface Box{{value:string|number;other:string|number}}\n",
                "declare function isText(value:string|number):value is string;\n",
                "function shell({root}:Box){{if(isText({access})){{\n",
                "  const before:string={access};\n",
                "  {access}={access}.length;\n",
                "  const after:string={access};\n",
                "  const sibling:string={root}.other;\n",
                "}}}}\n",
            ),
            root = root,
            access = access,
        );
        let (completion, diagnostics) = analyze(path, &source);
        assert_eq!(completion, SemanticCompletion::Deferred, "{path}");
        assert_eq!(
            diagnostics,
            vec![union_assignment(path, &source, "sibling")],
            "{path}",
        );
    }
}

#[test]
fn disjoint_writes_preserve_while_incomplete_paths_defer_fixed_narrowing() {
    for (path, write, expected) in [
        (
            "predicate-disjoint-write.ts",
            "box.other=1",
            SemanticCompletion::Complete,
        ),
        (
            "predicate-unrelated-write.ts",
            "other.value=1",
            SemanticCompletion::Complete,
        ),
        (
            "predicate-dynamic-write.ts",
            "box[key]=1",
            SemanticCompletion::Deferred,
        ),
    ] {
        let source = format!(
            concat!(
                "interface Box{{value:string|number;other:string|number}}\n",
                "declare function isText(value:string|number):value is string;\n",
                "function shell(box:Box,other:Box,key:any){{if(isText(box.value)){{\n",
                "  {write};\n",
                "  const kept:string=box.value;\n",
                "}}}}\n",
            ),
            write = write,
        );
        let (completion, diagnostics) = analyze(path, &source);
        assert_eq!(completion, expected, "{path}");
        assert_eq!(diagnostics, Vec::new(), "{path}");
    }

    let stable = concat!(
        "interface Box{nested:{value:string|number};other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box){const key='nested';if(isText(box.nested.value)){\n",
        "  box[key]={value:1};\n",
        "  const after:string=box.nested.value;\n",
        "  const sibling:string=box.other;\n",
        "}}\n",
    );
    let path = "predicate-stable-key-write.ts";
    let (completion, diagnostics) = analyze(path, stable);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(diagnostics, vec![union_assignment(path, stable, "sibling")]);
}

#[test]
fn ancestor_writes_invalidate_but_descendant_writes_preserve_predicate_paths() {
    let ancestor = concat!(
        "interface Box{nested:{value:string|number};other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box){if(isText(box.nested.value)){\n",
        "  const before:string=box.nested.value;\n",
        "  box.nested={value:1};\n",
        "  const after:string=box.nested.value;\n",
        "  const sibling:string=box.other;\n",
        "}}\n",
    );
    let ancestor_path = "predicate-ancestor-write.ts";
    let (completion, diagnostics) = analyze(ancestor_path, ancestor);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![union_assignment(ancestor_path, ancestor, "sibling")],
    );

    let descendant = concat!(
        "type Item={payload:string|number}|number;\n",
        "interface Box{value:Item}\n",
        "declare function isItem(value:Item):value is {payload:string|number};\n",
        "function shell(box:Box){if(isItem(box.value)){\n",
        "  box.value.payload=1;\n",
        "  const kept:{payload:string|number}=box.value;\n",
        "}}\n",
    );
    let descendant_path = "predicate-descendant-write.ts";
    let (completion, diagnostics) = analyze(descendant_path, descendant);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(diagnostics, Vec::new());
}

#[test]
fn path_write_targets_arms_and_captures_keep_their_local_completion() {
    let invalid = concat!(
        "interface Box{value:string|number;other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box){if(isText(box.value)){\n",
        "  box.value=true;\n",
        "  const sibling:string=box.other;\n",
        "}}\n",
    );
    let invalid_path = "predicate-invalid-write.ts";
    let (completion, diagnostics) = analyze(invalid_path, invalid);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostics,
        vec![
            assignment(
                invalid_path,
                invalid,
                "true",
                "Type 'boolean' is not assignable to type 'string | number'.",
            ),
            union_assignment(invalid_path, invalid, "sibling"),
        ],
    );

    let dirty_survivor = concat!(
        "interface Box{value:string|number;other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box){if(isText(box.value)){box.value=1;}else{return;}\n",
        "  const after:string=box.value;const sibling:string=box.other;\n",
        "}\n",
    );
    let dirty_path = "predicate-dirty-survivor.ts";
    let (completion, diagnostics) = analyze(dirty_path, dirty_survivor);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![union_assignment(dirty_path, dirty_survivor, "sibling")],
    );

    let clean_survivor = concat!(
        "interface Box{value:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box){if(isText(box.value)){box.value=1;return;}\n",
        "  const kept:number=box.value;\n",
        "}\n",
    );
    let (completion, diagnostics) = analyze("predicate-clean-survivor.ts", clean_survivor);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(diagnostics, Vec::new());

    let capture = concat!(
        "interface Box{value:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box){if(isText(box.value)){\n",
        "  const callback=()=>{box.value=1;const captured:string=box.value;};\n",
        "  const kept:string=box.value;\n",
        "}}\n",
    );
    let (completion, diagnostics) = analyze("predicate-captured-write.ts", capture);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(diagnostics, Vec::new());
}

#[test]
fn composed_path_write_targets_defer_instead_of_falling_back_to_declared_flow() {
    let source = concat!(
        "interface Box{value:string|number;other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box){\n",
        "  if(isText(box.value)){}else{return;}\n",
        "  if(isText(box.value)){\n",
        "    box.value=true;\n",
        "    const sibling:string=box.other;\n",
        "  }\n",
        "}\n",
    );
    let path = "predicate-composed-write-target.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(diagnostics, vec![union_assignment(path, source, "sibling")]);
}

#[test]
fn dependent_conditions_still_collect_independent_nested_predicates() {
    let direct = concat!(
        "declare function isText(value:string|number):value is string;\n",
        "function shell(value:string|number,other:string|number){\n",
        "  if(isText(value)){\n",
        "    if(value&&isText(other)){\n",
        "      const narrowed:string=other;\n",
        "      const kept:MissingNestedPredicate=1;\n",
        "    }\n",
        "  }\n",
        "}\n",
    );
    let direct_path = "nested-predicate.ts";
    let (completion, diagnostics) = analyze(direct_path, direct);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostics,
        vec![missing(direct_path, direct, "MissingNestedPredicate",)],
    );

    let member = concat!(
        "interface Box{value:string|number;other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(box:Box,renamed:string|number){\n",
        "  if(isText((box.value))){\n",
        "    if(box.value&&isText(((renamed)))){\n",
        "      const narrowed:string=renamed;\n",
        "      const independent:string=box.other;\n",
        "    }\n",
        "  }\n",
        "}\n",
    );
    let member_path = "nested-member-predicate.ts";
    let (completion, diagnostics) = analyze(member_path, member);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostics,
        vec![union_assignment(member_path, member, "independent")],
    );
}

#[test]
fn logical_and_predicate_effects_follow_only_the_executed_true_edge() {
    let source = concat!(
        "declare function isText(value:string|number):value is string;\n",
        "function falseEdge(flag:boolean,value:string|number){\n",
        "  if(flag&&isText(value)){const narrowed:string=value;}\n",
        "  else{const false_arm:string=value;}\n",
        "}\n",
        "function falseReturns(flag:boolean,value:string|number){\n",
        "  if(flag&&isText(value)){}else{return;}\n",
        "  const after_false_return:string=value;\n",
        "}\n",
        "function trueReturns(flag:boolean,value:string|number){\n",
        "  if(flag&&isText(value)){return;}\n",
        "  const after_true_return:string=value;\n",
        "}\n",
    );
    let path = "logical-predicate-edges.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostics,
        vec![
            union_assignment(path, source, "false_arm"),
            union_assignment(path, source, "after_true_return"),
        ],
    );
}

#[test]
fn logical_and_predicate_path_writes_invalidate_only_later_dependents() {
    let source = concat!(
        "interface Box{value:string|number;other:string|number}\n",
        "declare function isText(value:string|number):value is string;\n",
        "function shell(flag:boolean,box:Box){\n",
        "  if(flag&&isText(box.value)){\n",
        "    const before:string=box.value;\n",
        "    box.value=1;\n",
        "    const dependent:string=box.value;\n",
        "    const sibling:string=box.other;\n",
        "  }\n",
        "}\n",
    );
    let path = "logical-predicate-path-write.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(diagnostics, vec![union_assignment(path, source, "sibling")]);
}

#[test]
fn logical_or_negation_and_nested_conjunctions_do_not_gain_a_true_predicate_edge() {
    let source = concat!(
        "declare function isText(value:string|number):value is string;\n",
        "function shell(flag:boolean,first:string|number,second:string|number){\n",
        "  if(flag||isText(first)){const or_arm:string=first;}\n",
        "  if(!(flag&&isText(second))){const negated_arm:string=second;}\n",
        "  if(flag&&first&&isText(second)){}else{const nested_else:string=second;}\n",
        "}\n",
    );
    let path = "unsupported-logical-predicate-shapes.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostics,
        vec![
            union_assignment(path, source, "or_arm"),
            union_assignment(path, source, "negated_arm"),
            union_assignment(path, source, "nested_else"),
        ],
    );
}

#[test]
fn nonclaimed_control_flow_keeps_independent_host_expression_names() {
    for (switch_path, binder, discriminant, case, callee, argument) in [
        (
            "host-switch.ts",
            "value",
            "value.",
            "MissingCase",
            "MissingCall",
            "MissingArg",
        ),
        (
            "wrapped-host-switch.ts",
            "renamed",
            "((renamed)).",
            "((MissingCase))",
            "((MissingCall))",
            "((MissingArg))",
        ),
    ] {
        let switch_source = format!(
            concat!(
                "function shell({binder}:{{tag:string}}){{\n",
                "  const resolved=true;\n",
                "  switch({discriminant}){{case {case}:{callee}({binder},{argument},resolved);break;}}\n",
                "  const kept:MissingAfterSwitch=1;\n",
                "}}\n",
            ),
            binder = binder,
            discriminant = discriminant,
            case = case,
            callee = callee,
            argument = argument,
        );
        let (switch_completion, switch_diagnostics) = analyze(switch_path, &switch_source);
        assert_eq!(
            switch_completion,
            SemanticCompletion::Deferred,
            "{switch_path}"
        );
        assert_eq!(
            switch_diagnostics,
            vec![
                missing(switch_path, &switch_source, "MissingCase"),
                missing(switch_path, &switch_source, "MissingCall"),
                missing(switch_path, &switch_source, "MissingArg"),
                missing(switch_path, &switch_source, "MissingAfterSwitch"),
            ],
            "{switch_path}",
        );
    }

    for (if_path, predicate, condition) in [
        (
            "host-if.ts",
            "choose",
            "resolvedBefore&&MissingBefore&&choose(value,`head${\"mode\"}tail`)&&MissingAfter&&resolvedAfter",
        ),
        (
            "wrapped-host-if.ts",
            "renamedChoose",
            "((resolvedBefore))&&((MissingBefore))&&((renamedChoose))((value),`head${\"mode\"}tail`)&&((MissingAfter))&&((resolvedAfter))",
        ),
    ] {
        let if_source = format!(
            concat!(
                "declare function {predicate}(value:string|number,mode:unknown):value is string;\n",
                "function shell(value:string|number){{\n",
                "  const resolvedBefore=true; const resolvedAfter=true;\n",
                "  if({condition}){{\n",
                "    const kept:MissingInsideIf=1;\n",
                "  }}\n",
                "}}\n",
            ),
            predicate = predicate,
            condition = condition,
        );
        let (if_completion, if_diagnostics) = analyze(if_path, &if_source);
        assert_eq!(if_completion, SemanticCompletion::Deferred, "{if_path}");
        assert_eq!(
            if_diagnostics,
            vec![
                missing(if_path, &if_source, "MissingBefore"),
                missing(if_path, &if_source, "MissingAfter"),
                missing(if_path, &if_source, "MissingInsideIf"),
            ],
            "{if_path}",
        );
    }
}

#[test]
fn local_literal_assignment_kills_prior_incomplete_flow_without_poisoning_siblings() {
    let source = concat!(
        "declare function isText(value:string|number):value is string;\n",
        "declare function takeText(value:string):void;\n",
        "type Tagged={tag:\"text\";payload:string}|{tag:\"number\";payload:number};\n",
        "function shell(value:Tagged){\n",
        "  let combined:string|number=0;\n",
        "  switch(value.tag){case \"text\":combined=value.payload||\"fallback\";takeText(combined);}\n",
        "  let killed:string|number=0;\n",
        "  switch(value.tag){case \"text\":killed=value.payload;killed=1;takeText(killed);}\n",
        "  let joined:string|number=0;\n",
        "  switch(value.tag){case \"text\":if(isText(joined)){joined=value.payload;}else{joined=value.payload;}takeText(joined);}\n",
        "  const kept:MissingTransferSibling=1;\n",
        "}\n",
    );
    let path = "flow-transfer.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    let killed =
        source.find("takeText(killed)").expect("killed call") as u32 + "takeText(".len() as u32;
    assert_eq!(
        diagnostics,
        vec![
            (
                path.to_string(),
                2345,
                killed,
                "killed".len() as u32,
                DiagnosticCategory::Error,
                "Argument of type 'number' is not assignable to parameter of type 'string'."
                    .to_string(),
                Vec::new(),
            ),
            missing(path, source, "MissingTransferSibling"),
        ],
    );
}

#[test]
fn local_reference_and_literal_assignments_follow_decl_ids_and_parentheses() {
    for (path, value, target) in [
        ("flow-local-assignment.ts", "value", "target"),
        ("flow-local-assignment-renamed.ts", "input", "result"),
    ] {
        let source = format!(
            concat!(
                "declare function isText(value:string|number):value is string;\n",
                "declare function takeText(value:string):void;\n",
                "function shell({value}:string|number,other:string|number){{\n",
                "  let {target}:string|number=0;\n",
                "  let selfed:string|number=0;\n",
                "  if(isText((({value})))){{\n",
                "    (({target}))=((({value})));takeText({target});\n",
                "    {{let {target}:string|number=0;(({target}))=1;takeText({target});}}\n",
                "    takeText({target});\n",
                "    (({target}))=1;takeText({target});\n",
                "    selfed=selfed;takeText(selfed);\n",
                "  }}\n",
                "  takeText({target});\n",
                "  const sibling:string=other;\n",
                "}}\n",
            ),
            value = value,
            target = target,
        );
        let killed_call = format!("takeText({target})");
        let target_calls = source
            .match_indices(&killed_call)
            .map(|(start, _)| start as u32 + "takeText(".len() as u32)
            .collect::<Vec<_>>();
        let selfed =
            source.find("takeText(selfed)").expect("self call") as u32 + "takeText(".len() as u32;
        let (completion, diagnostics) = analyze(path, &source);
        assert_eq!(completion, SemanticCompletion::Deferred, "{path}");
        assert_eq!(
            diagnostics,
            vec![
                (
                    path.to_string(),
                    2345,
                    target_calls[1],
                    target.len() as u32,
                    DiagnosticCategory::Error,
                    "Argument of type 'number' is not assignable to parameter of type 'string'."
                        .to_string(),
                    Vec::new(),
                ),
                (
                    path.to_string(),
                    2345,
                    target_calls[3],
                    target.len() as u32,
                    DiagnosticCategory::Error,
                    "Argument of type 'number' is not assignable to parameter of type 'string'."
                        .to_string(),
                    Vec::new(),
                ),
                (
                    path.to_string(),
                    2345,
                    selfed,
                    "selfed".len() as u32,
                    DiagnosticCategory::Error,
                    "Argument of type 'string | number' is not assignable to parameter of type 'string'."
                        .to_string(),
                    vec![(
                        String::new(),
                        2345,
                        0,
                        0,
                        "Type 'number' is not assignable to type 'string'.".to_string(),
                        1,
                    )],
                ),
                union_assignment(path, &source, "sibling"),
            ],
            "{path}",
        );
    }
}

#[test]
fn local_literal_assignments_preserve_admitted_union_members_and_defer_rejected_flow() {
    for (path, tag, target, invalid, broad, accepted, alternate, expected, missing_name) in [
        (
            "flow-literal-target.ts",
            "tag",
            "choice",
            "invalid",
            "broad",
            "left",
            "right",
            "\"left\" | \"right\"",
            "MissingLiteralSibling",
        ),
        (
            "flow-literal-target-renamed.ts",
            "mode",
            "selected",
            "rejected",
            "wide",
            "open",
            "closed",
            "\"closed\" | \"open\"",
            "MissingRenamedLiteralSibling",
        ),
    ] {
        let source = format!(
            concat!(
                "declare function takeExact(value:\"{accepted}\"):void;\n",
                "declare function takeText(value:string):void;\n",
                "function shell({tag}:\"yes\"|\"no\"){{\n",
                "  let {target}:\"{accepted}\"|\"{alternate}\"=\"{alternate}\";\n",
                "  if({tag}===\"yes\"){{(({target}))=\"{accepted}\";takeExact({target});}}\n",
                "  let {invalid}:\"{accepted}\"|\"{alternate}\"=\"{alternate}\";\n",
                "  if({tag}===\"yes\"){{{invalid}=\"outside\";takeExact({invalid});}}\n",
                "  let {broad}:string|number=0;\n",
                "  if({tag}===\"yes\"){{{broad}=1;takeText({broad});}}\n",
                "  const sibling:{missing_name}=1;\n",
                "}}\n",
            ),
            tag = tag,
            target = target,
            invalid = invalid,
            broad = broad,
            accepted = accepted,
            alternate = alternate,
            missing_name = missing_name,
        );
        let broad_call = source
            .find(&format!("takeText({broad})"))
            .expect("broad call") as u32
            + "takeText(".len() as u32;
        let invalid_call = source
            .find(&format!("takeExact({invalid})"))
            .expect("rejected assignment call") as u32
            + "takeExact(".len() as u32;
        let (completion, diagnostics) = analyze(path, &source);
        assert_eq!(completion, SemanticCompletion::Complete, "{path}");
        assert_eq!(
            diagnostics,
            vec![
                assignment(
                    path,
                    &source,
                    "\"outside\"",
                    &format!("Type '\"outside\"' is not assignable to type '{expected}'."),
                ),
                (
                    path.to_string(),
                    2345,
                    invalid_call,
                    invalid.len() as u32,
                    DiagnosticCategory::Error,
                    format!(
                        "Argument of type '{expected}' is not assignable to parameter of type '\"{accepted}\"'."
                    ),
                    vec![(
                        String::new(),
                        2345,
                        0,
                        0,
                        format!(
                            "Type '\"{alternate}\"' is not assignable to type '\"{accepted}\"'."
                        ),
                        1,
                    )],
                ),
                (
                    path.to_string(),
                    2345,
                    broad_call,
                    broad.len() as u32,
                    DiagnosticCategory::Error,
                    "Argument of type 'number' is not assignable to parameter of type 'string'."
                        .to_string(),
                    Vec::new(),
                ),
                missing(path, &source, missing_name),
            ],
            "{path}",
        );
    }
}

#[test]
fn nested_unmodeled_control_writes_defer_instead_of_becoming_lexically_definite() {
    for (path, tag, flag, value, one, two, returning, missing_name) in [
        (
            "flow-nested-control-assignment.ts",
            "tag",
            "flag",
            "value",
            "one",
            "two",
            "returning",
            "MissingControlSibling",
        ),
        (
            "flow-nested-control-assignment-renamed.ts",
            "mode",
            "enabled",
            "text",
            "single",
            "paired",
            "exiting",
            "MissingRenamedControlSibling",
        ),
    ] {
        let source = format!(
            concat!(
                "declare function takeText(value:string):void;\n",
                "function shell({tag}:\"yes\"|\"no\",{flag}:boolean,{value}:string){{\n",
                "  let {one}:string|number=0;\n",
                "  if({tag}===\"yes\"){{if({flag}){{{one}={value};}}takeText({one});}}\n",
                "  let {two}:string|number=0;\n",
                "  if({tag}===\"yes\"){{if({flag}){{{two}={value};}}else{{{two}=1;}}takeText({two});}}\n",
                "  let {returning}:string|number=0;\n",
                "  if({tag}===\"yes\"){{if({flag}){{{returning}={value};}}else{{{returning}=1;return;}}takeText({returning});}}\n",
                "  const sibling:{missing_name}=1;\n",
                "}}\n",
            ),
            tag = tag,
            flag = flag,
            value = value,
            one = one,
            two = two,
            returning = returning,
            missing_name = missing_name,
        );
        let (completion, diagnostics) = analyze(path, &source);
        assert_eq!(completion, SemanticCompletion::Deferred, "{path}");
        assert_eq!(
            diagnostics,
            vec![missing(path, &source, missing_name)],
            "{path}",
        );
    }
}

#[test]
fn fixed_call_results_reach_annotated_return_and_nested_call_relations() {
    let source = concat!(
        "declare function size(value:{tag:string}):number;\n",
        "declare function takeText(value:string):void;\n",
        "function shell(value:{tag:string}):string{\n",
        "  switch(value.tag){default:\n",
        "    const bad:string=size(value);\n",
        "    takeText(size(value));\n",
        "    return size(value);\n",
        "  }\n",
        "}\n",
    );
    let path = "fixed-result-consumers.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Complete);
    let nested_call = source.find("size(value));").expect("nested call") as u32;
    assert_eq!(
        diagnostics,
        vec![
            assignment(
                path,
                source,
                "bad",
                "Type 'number' is not assignable to type 'string'.",
            ),
            (
                path.to_string(),
                2345,
                nested_call,
                "size(value)".len() as u32,
                DiagnosticCategory::Error,
                "Argument of type 'number' is not assignable to parameter of type 'string'."
                    .to_string(),
                Vec::new(),
            ),
            (
                path.to_string(),
                2322,
                source.find("return").expect("return relation") as u32,
                "return".len() as u32,
                DiagnosticCategory::Error,
                "Type 'number' is not assignable to type 'string'.".to_string(),
                Vec::new(),
            ),
        ],
    );
}

#[test]
fn wrapped_predicate_callee_keeps_exact_member_subject() {
    let source = concat!(
        "declare function renamedPredicate(value:string|number):value is string;\n",
        "declare function takeText(value:string):void;\n",
        "function shell(box:{value:string|number;other:number}){\n",
        "  if((renamedPredicate)(box.value)){\n",
        "    takeText(box.value);\n",
        "    takeText(box.other);\n",
        "  }\n",
        "}\n",
    );
    let path = "wrapped-predicate.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostics,
        vec![(
            path.to_string(),
            2345,
            source.find("box.other").expect("independent member") as u32,
            "box.other".len() as u32,
            DiagnosticCategory::Error,
            "Argument of type 'number' is not assignable to parameter of type 'string'."
                .to_string(),
            Vec::new(),
        )],
    );
}

#[test]
fn complete_binary_operands_require_their_typescript_operator_semantics() {
    let source = concat!(
        "declare let large:bigint;\n",
        "declare let text:string;\n",
        "const bigintResult=large-large;\n",
        "const wrongNumber:number=bigintResult;\n",
        "const invalidLeft=text-1;\n",
        "const invalidRight=1-text;\n",
        "const mixed=large+1;\n",
    );
    let path = "complete-binary-operators.ts";
    let (completion, diagnostics) = analyze(path, source);
    assert_eq!(completion, SemanticCompletion::Complete);
    assert_eq!(
        diagnostics,
        vec![
            assignment(
                path,
                source,
                "wrongNumber",
                "Type 'bigint' is not assignable to type 'number'.",
            ),
            (
                path.to_string(),
                2362,
                source.find("text-1").expect("left operand") as u32,
                "text".len() as u32,
                DiagnosticCategory::Error,
                "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
                Vec::new(),
            ),
            (
                path.to_string(),
                2363,
                source.find("1-text").expect("right expression") as u32 + 2,
                "text".len() as u32,
                DiagnosticCategory::Error,
                "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
                Vec::new(),
            ),
            (
                path.to_string(),
                2365,
                source.find("large+1").expect("mixed expression") as u32,
                "large+1".len() as u32,
                DiagnosticCategory::Error,
                "Operator '+' cannot be applied to types 'bigint' and '1'.".to_string(),
                Vec::new(),
            ),
        ],
    );

    let producer = concat!(
        "type Wide=bigint;type Words=string;\n",
        "declare let wide:Wide;declare let words:Words;\n",
        "declare let count:number;declare let dynamic:any;declare let bottom:never;\n",
    );
    let consumer = concat!(
        "const validBigint:bigint=((wide))-wide;\n",
        "const validNumber:number=(count)+1;\n",
        "const validString:string=('prefix')+count;\n",
        "const validAny:any=dynamic+wide;\n",
        "const validNever:number=bottom|bottom;\n",
        "const invalidLiteral='left'-1;\n",
        "const invalidAlias=1-((words));\n",
        "const mixedError:string=(wide)-1;\n",
        "const mixedReverse:string=(1)+(wide);\n",
        "const leftMixed:string='x'-1n;\n",
        "const rightMixed:string=1n-'x';\n",
        "const bothInvalid:string='x'-true;\n",
    );
    let producer_path = "binary-producer.ts";
    let consumer_path = "binary-consumer.ts";
    let expected = vec![
        (
            consumer_path.to_string(),
            2362,
            consumer.find("'left'-1").expect("literal left operand") as u32,
            "'left'".len() as u32,
            DiagnosticCategory::Error,
            "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
            Vec::new(),
        ),
        (
            consumer_path.to_string(),
            2363,
            consumer.find("((words))").expect("wrapped alias operand") as u32,
            "((words))".len() as u32,
            DiagnosticCategory::Error,
            "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
            Vec::new(),
        ),
        (
            consumer_path.to_string(),
            2365,
            consumer.find("(wide)-1").expect("mixed error expression") as u32,
            "(wide)-1".len() as u32,
            DiagnosticCategory::Error,
            "Operator '-' cannot be applied to types 'bigint' and 'number'.".to_string(),
            Vec::new(),
        ),
        (
            consumer_path.to_string(),
            2365,
            consumer.find("(1)+(wide)").expect("reversed mixed expression") as u32,
            "(1)+(wide)".len() as u32,
            DiagnosticCategory::Error,
            "Operator '+' cannot be applied to types '1' and 'bigint'.".to_string(),
            Vec::new(),
        ),
        (
            consumer_path.to_string(),
            2362,
            consumer.find("'x'-1n").expect("invalid left with bigint") as u32,
            "'x'".len() as u32,
            DiagnosticCategory::Error,
            "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
            Vec::new(),
        ),
        (
            consumer_path.to_string(),
            2365,
            consumer.find("'x'-1n").expect("invalid left mixed expression") as u32,
            "'x'-1n".len() as u32,
            DiagnosticCategory::Error,
            "Operator '-' cannot be applied to types 'string' and 'bigint'.".to_string(),
            Vec::new(),
        ),
        (
            consumer_path.to_string(),
            2365,
            consumer.find("1n-'x'").expect("invalid right mixed expression") as u32,
            "1n-'x'".len() as u32,
            DiagnosticCategory::Error,
            "Operator '-' cannot be applied to types 'bigint' and 'string'.".to_string(),
            Vec::new(),
        ),
        (
            consumer_path.to_string(),
            2363,
            consumer.find("1n-'x'").expect("invalid right with bigint") as u32 + 3,
            "'x'".len() as u32,
            DiagnosticCategory::Error,
            "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
            Vec::new(),
        ),
        assignment(
            consumer_path,
            consumer,
            "bothInvalid",
            "Type 'number' is not assignable to type 'string'.",
        ),
        (
            consumer_path.to_string(),
            2362,
            consumer.find("'x'-true").expect("both invalid left") as u32,
            "'x'".len() as u32,
            DiagnosticCategory::Error,
            "The left-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
            Vec::new(),
        ),
        (
            consumer_path.to_string(),
            2363,
            consumer.find("'x'-true").expect("both invalid right") as u32 + 4,
            "true".len() as u32,
            DiagnosticCategory::Error,
            "The right-hand side of an arithmetic operation must be of type 'any', 'number', 'bigint' or an enum type.".to_string(),
            Vec::new(),
        ),
    ];
    for reversed in [false, true] {
        let files = if reversed {
            [(consumer_path, consumer), (producer_path, producer)]
        } else {
            [(producer_path, producer), (consumer_path, consumer)]
        };
        let mut service = LanguageService::new(CompilerOptions {
            no_emit: true,
            strict: true,
            target: "es2020".to_string(),
            ..CompilerOptions::default()
        });
        for (path, source) in files {
            service.open(path, Arc::<str>::from(source));
        }
        for _ in 0..2 {
            let result = service.semantic_diagnostics(consumer_path);
            assert_eq!(result.semantic_completion, SemanticCompletion::Complete);
            assert_eq!(
                result
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (
                        diagnostic.file.clone(),
                        diagnostic.code,
                        diagnostic.start,
                        diagnostic.length,
                        diagnostic.category,
                        diagnostic.message_text.clone(),
                        related_fingerprint(&diagnostic.related_information),
                    ))
                    .collect::<Vec<_>>(),
                expected,
            );
        }
    }
}

#[test]
fn claimed_parameter_default_arrows_keep_nested_statement_diagnostics() {
    let function_source = concat!(
        "function outer(callback=(()=>{\n",
        "  const wrongDefault:string=1;\n",
        "  const kept:MissingDefaultInside=1;\n",
        "})){}\n",
    );
    let function_path = "function-default.ts";
    let (completion, diagnostics) = analyze(function_path, function_source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![
            assignment(
                function_path,
                function_source,
                "wrongDefault",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(function_path, function_source, "MissingDefaultInside"),
        ],
    );

    let class_source = concat!(
        "class Holder{\n",
        "  constructor(callback=()=>{const wrongCtor:string=1;const kept:MissingCtor=1;}){}\n",
        "  renamed(callback=(()=>{const wrongMethod:string=1;const kept:MissingMethod=1;})){}\n",
        "}\n",
    );
    let class_path = "class-default.ts";
    let (completion, diagnostics) = analyze(class_path, class_source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![
            assignment(
                class_path,
                class_source,
                "wrongCtor",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(class_path, class_source, "MissingCtor"),
            assignment(
                class_path,
                class_source,
                "wrongMethod",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(class_path, class_source, "MissingMethod"),
        ],
    );

    let arrow_source = concat!(
        "const claimed=(callback=(()=>{\n",
        "  const wrongArrowParameter:string=1;\n",
        "  MissingArrowParameter;\n",
        "}))=>{};\n",
    );
    let arrow_path = "arrow-parameter-default.ts";
    let (completion, diagnostics) = analyze(arrow_path, arrow_source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![
            assignment(
                arrow_path,
                arrow_source,
                "wrongArrowParameter",
                "Type 'number' is not assignable to type 'string'.",
            ),
            missing(arrow_path, arrow_source, "MissingArrowParameter"),
        ],
    );
}

#[test]
fn required_types_descend_claimed_parameter_default_arrows() {
    let function_source = concat!(
        "function outer<T>(callback=((value:MissingFunctionParameter):MissingFunctionReturn=>{\n",
        "  const gap=`plain`;\n",
        "  type Kept=T|MissingRequiredDefault;\n",
        "})){}\n",
    );
    let function_path = "required-function-default.ts";
    let (completion, diagnostics) = analyze(function_path, function_source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![
            missing(function_path, function_source, "MissingFunctionParameter",),
            missing(function_path, function_source, "MissingFunctionReturn",),
            missing(function_path, function_source, "MissingRequiredDefault",),
        ],
    );

    let method_source = concat!(
        "class Holder<T>{renamed<U>(callback=((value:MissingMethodParameter):MissingMethodReturn=>{\n",
        "  const gap=`plain`;\n",
        "  type Kept=T|U|MissingMethodRequired;\n",
        "})){} }\n",
    );
    let method_path = "required-method-default.ts";
    let (completion, diagnostics) = analyze(method_path, method_source);
    assert_eq!(completion, SemanticCompletion::Deferred);
    assert_eq!(
        diagnostics,
        vec![
            missing(method_path, method_source, "MissingMethodParameter",),
            missing(method_path, method_source, "MissingMethodReturn"),
            missing(method_path, method_source, "MissingMethodRequired"),
        ],
    );
}
