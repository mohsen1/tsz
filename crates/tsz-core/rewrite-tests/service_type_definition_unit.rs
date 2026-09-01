use super::*;

#[macro_use]
#[path = "fixtures/service_query_expect.rs"]
mod service_query_expect;
expect_claimed_extension!();

fn offset(source: &str, marker: &str) -> u32 {
    source.find(marker).unwrap() as u32
}

fn claimed(service: &LanguageService, path: &str, offset: u32) -> Vec<DefinitionInfo> {
    service
        .type_definition(path, offset)
        .expect_claimed("modeled type definition must be complete")
}

#[test]
fn type_definition_uses_checker_owned_type_declarations() {
    let source = "type Word = string;\n\
interface Container<T> { value: T }\n\
class Item { value: string = ''; }\n\
const primitive: Word = '';\n\
const concrete: Container<string> = { value: '' };\n\
const annotated: Item = new Item();\n\
const inferred = new Item();\n";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("case.ts", source);

    let alias = claimed(&service, "case.ts", offset(source, "Word = ''"));
    assert_eq!(alias.len(), 1);
    assert_eq!(
        (alias[0].name.as_str(), alias[0].kind.as_str()),
        ("Word", "type")
    );
    assert_eq!(alias[0].container_kind, "");

    for name in ["primitive:", "concrete:", "annotated:", "inferred ="] {
        let definitions = claimed(&service, "case.ts", offset(source, name));
        assert_eq!(definitions.len(), 1, "{name}");
        assert_eq!(definitions[0].file_name, "case.ts");
    }
    assert_eq!(
        claimed(&service, "case.ts", offset(source, "primitive:"))[0].name,
        "Word"
    );
    assert_eq!(
        claimed(&service, "case.ts", offset(source, "concrete:"))[0].name,
        "Container"
    );
    assert_eq!(
        claimed(&service, "case.ts", offset(source, "annotated:"))[0].name,
        "Item"
    );
    assert_eq!(
        claimed(&service, "case.ts", offset(source, "inferred ="))[0].name,
        "Item"
    );
}

#[test]
fn type_definition_uses_authored_anonymous_object_identity() {
    let model = "type User = { name: string };\n\
type Box<T> = { value: T };\n\
declare const boxedUser: Box<User>;\n\
boxedUser;\n\
type Either<T> = ({ left: T } | { right: T });\n\
declare const either: Either<User>;\n\
either;\n";
    let control = "export const control = 1;\n";
    for reversed in [false, true] {
        let mut roots = vec![
            SourceInput::new("model.ts", model),
            SourceInput::new("control.ts", control),
        ];
        if reversed {
            roots.reverse();
        }
        for _ in 0..2 {
            let output = Compiler::new().compile(roots.clone(), &CompilerOptions::default());
            let index = navigation::NavigationIndex::build(&output);
            let boxed = index.type_definition("model.ts", offset(model, "boxedUser;"));
            assert_eq!(boxed.len(), 1);
            let target = &boxed[0];
            let start = offset(model, "{ value: T }");
            assert_eq!(target.file_name, "model.ts");
            assert_eq!(
                target.text_span,
                TextSpan {
                    start,
                    length: "{ value: T }".len() as u32,
                }
            );
            assert_eq!((target.kind.as_str(), target.name.as_str()), ("", "__type"));
            assert_eq!(target.container_kind, "");
            assert_eq!(target.container_name, "");
            assert!(!target.is_local && !target.is_ambient);
            assert!(!target.unverified && !target.failed_alias_resolution);
            assert_eq!(target.context_span, None);

            let either = index.type_definition("model.ts", offset(model, "either;"));
            assert_eq!(
                either
                    .iter()
                    .map(|definition| (definition.name.as_str(), definition.text_span.start))
                    .collect::<Vec<_>>(),
                [
                    ("__type", offset(model, "{ left: T }")),
                    ("__type", offset(model, "{ right: T }")),
                ]
            );
        }
    }
}

#[test]
fn type_definition_alias_bodies_are_dependency_closed_in_the_alias_scope() {
    let model = "export interface Owner { remote: true }\n\
export type Named = (((Owner)));\n\
export type Concrete = (({ inline: string } | Owner));\n\
export type Generic<T> = (({ value: T } | Owner));\n\
export type ArrayGap = ({ array: string } | Owner[]);\n\
export type IntersectionGap = ({ intersection: string } & Owner[]);\n\
export type MissingGap = (({ missing: string } | Missing));\n";
    let use_source = "import type { Named, Concrete, Generic, ArrayGap, IntersectionGap, MissingGap } from './model';\n\
interface Owner { local: false }\n\
declare const named: Named;\n\
named;\n\
declare const concrete: Concrete;\n\
concrete;\n\
declare const generic: Generic<number>;\n\
generic;\n\
declare const arrayGap: ArrayGap;\n\
arrayGap;\n\
declare const intersectionGap: IntersectionGap;\n\
intersectionGap;\n\
declare const missingGap: MissingGap;\n\
missingGap;\n";
    let mut stable_products = None;
    for reversed in [false, true] {
        let mut service = LanguageService::new(CompilerOptions::default());
        if reversed {
            service.open("use.ts", use_source);
            service.open("model.ts", model);
        } else {
            service.open("model.ts", model);
            service.open("use.ts", use_source);
        }
        for _ in 0..2 {
            let named = claimed(&service, "use.ts", offset(use_source, "named;"));
            assert_eq!(named.len(), 1);
            assert_eq!(
                (
                    named[0].file_name.as_str(),
                    named[0].name.as_str(),
                    named[0].text_span.start,
                ),
                ("model.ts", "Owner", offset(model, "Owner { remote"))
            );

            let concrete = claimed(&service, "use.ts", offset(use_source, "concrete;"));
            let generic = claimed(&service, "use.ts", offset(use_source, "generic;"));
            for (definitions, anonymous_marker) in [
                (&concrete, "{ inline: string }"),
                (&generic, "{ value: T }"),
            ] {
                assert_eq!(definitions.len(), 2);
                assert!(definitions.iter().any(|definition| {
                    definition.file_name == "model.ts"
                        && definition.name == "Owner"
                        && definition.text_span.start == offset(model, "Owner { remote")
                }));
                assert!(definitions.iter().any(|definition| {
                    definition.file_name == "model.ts"
                        && definition.name == "__type"
                        && definition.text_span.start == offset(model, anonymous_marker)
                }));
            }
            let products = [concrete, generic].map(|definitions| {
                definitions
                    .into_iter()
                    .map(|definition| (definition.file_name, definition.name, definition.text_span))
                    .collect::<Vec<_>>()
            });
            if let Some(expected) = &stable_products {
                assert_eq!(&products, expected);
            } else {
                stable_products = Some(products);
            }

            for marker in ["arrayGap;", "intersectionGap;", "missingGap;"] {
                assert!(matches!(
                    service.type_definition("use.ts", offset(use_source, marker)),
                    ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
                ));
            }
        }
    }
}

#[test]
fn type_definition_preserves_type_meaning_imports_and_constituent_order() {
    let model = "export interface Left { left: string }\n\
export class Right { right: string = ''; }\n";
    let use_source = "import { Left as L, Right as R } from './model';\n\
let joined: (L | R) & L;\n\
joined;\n";
    let expected = ["Left", "Right"];
    for reversed in [false, true] {
        let mut roots = vec![
            SourceInput::new("model.ts", model),
            SourceInput::new("use.ts", use_source),
        ];
        if reversed {
            roots.reverse();
        }
        for _ in 0..2 {
            let output = Compiler::new().compile(roots.clone(), &CompilerOptions::default());
            let index = navigation::NavigationIndex::build(&output);
            let definitions = index.type_definition("use.ts", offset(use_source, "joined;"));
            assert_eq!(
                definitions
                    .iter()
                    .map(|definition| definition.name.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(
                index
                    .query_completion(
                        Target::TypeDefinition,
                        "use.ts",
                        offset(use_source, "joined;")
                    )
                    .is_complete()
            );
        }
    }
}

#[test]
fn type_definition_does_not_filter_definition_or_claim_anonymous_types() {
    let source = "interface Dual { value: string }\n\
const Dual = 1;\n\
const typed: Dual = { value: '' };\n\
typed;\n\
Dual;\n\
const anonymous = { value: '' };\n\
anonymous;\n";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("meaning.ts", source);

    let typed = claimed(&service, "meaning.ts", offset(source, "typed;"));
    assert_eq!(
        (typed[0].name.as_str(), typed[0].kind.as_str()),
        ("Dual", "interface")
    );
    assert!(claimed(&service, "meaning.ts", offset(source, "Dual;\nconst")).is_empty());
    assert!(matches!(
        service.type_definition("meaning.ts", offset(source, "anonymous;")),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
}

#[test]
fn type_definition_uses_the_sole_function_return_and_fences_partial_products() {
    let source = "interface Result {}\n\
interface Merged { left: string }\n\
interface Merged { right: string }\n\
function make(): Result { return {}; }\n\
make();\n\
let merged: Merged;\n\
merged;\n\
let merged_partial: Merged | Merged[];\n\
merged_partial;\n\
let partial: Result | Result[];\n\
partial;\n\
let array: Result[];\n\
array;\n";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("return.ts", source);

    let result = claimed(&service, "return.ts", offset(source, "make();"));
    assert_eq!(
        (result[0].name.as_str(), result[0].kind.as_str()),
        ("Result", "interface")
    );
    let merged = claimed(&service, "return.ts", offset(source, "merged;"));
    assert_eq!(merged.len(), 2);
    assert!(merged.iter().all(|definition| definition.name == "Merged"));
    for marker in ["merged_partial;", "partial;", "array;"] {
        assert!(matches!(
            service.type_definition("return.ts", offset(source, marker)),
            ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
        ));
    }

    let changed = format!("{source}let generic_array: Array<Result>;\ngeneric_array;\n");
    service.change("return.ts", changed.clone());
    assert!(matches!(
        service.type_definition("return.ts", offset(&changed, "generic_array;")),
        ServiceQuery::Nonclaimed(NavigationNonclaim::Deferred)
    ));
}

#[test]
fn import_alias_definition_and_type_definition_keep_value_and_type_products_distinct() {
    let model = "export type T = string;\nexport const T = '';\n";
    let use_source = "import { T } from './model';\nlet value: T;\nvalue;\n";
    for reversed in [false, true] {
        let mut roots = vec![
            SourceInput::new("model.ts", model),
            SourceInput::new("use.ts", use_source),
        ];
        if reversed {
            roots.reverse();
        }
        let output = Compiler::new().compile(roots, &CompilerOptions::default());
        let index = navigation::NavigationIndex::build(&output);
        let reference = offset(use_source, "T;\nvalue");
        assert!(index.type_definition("use.ts", reference).is_empty());
        assert!(
            index
                .query_completion(Target::TypeDefinition, "use.ts", reference)
                .is_complete()
        );
        let definition = index.definition("use.ts", reference).unwrap();
        assert_eq!(definition.text_span.start, reference);
        assert_eq!(
            definition
                .definitions
                .iter()
                .map(|definition| (
                    definition.text_span.start,
                    definition.kind.as_str(),
                    definition.name.as_str(),
                    definition.container_name.as_str(),
                ))
                .collect::<Vec<_>>(),
            [
                (
                    model.find("T = string").unwrap() as u32,
                    "const",
                    "T",
                    "\"./model\""
                ),
                (
                    model.find("T = ''").unwrap() as u32,
                    "const",
                    "T",
                    "\"./model\""
                ),
            ]
        );
        assert!(
            definition
                .definitions
                .iter()
                .all(|definition| definition.container_kind.is_empty())
        );
    }

    let type_only = "import type { T } from './model';\nlet value: T;\nvalue;\n";
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("model.ts", model);
    service.open("type-only.ts", type_only);
    let reference = offset(type_only, "T;\nvalue");
    let definitions = claimed(&service, "type-only.ts", reference);
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        (definitions[0].name.as_str(), definitions[0].kind.as_str()),
        ("T", "type")
    );
}
