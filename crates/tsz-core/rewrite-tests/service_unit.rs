use super::*;

#[test]
fn compiled_snapshot_is_reused_and_invalidated_by_every_revision_owner() {
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open(
        "case.ts",
        Arc::<str>::from("const gap = `plain`; const value: string = missing;"),
    );
    assert!(service.compiled_snapshot.get_mut().is_none());

    let first = service.semantic_diagnostics("case.ts");
    assert_eq!(first.diagnostics.len(), 1);
    assert_eq!(first.semantic_completion, SemanticCompletion::Complete);
    assert!(service.compiled_snapshot.get_mut().is_some());
    let uncached = service.compile();
    let cached = service.compiled_snapshot.get_mut().as_ref().unwrap();
    assert_eq!(cached.semantic_completion, uncached.semantic_completion);
    assert_eq!(cached.diagnostics, uncached.diagnostics);

    service.configure(CompilerOptions {
        no_check: true,
        ..CompilerOptions::default()
    });
    assert!(service.compiled_snapshot.get_mut().is_none());
    let _ = service.semantic_diagnostics("case.ts");
    assert!(service.compiled_snapshot.get_mut().is_some());

    service.open("other.ts", Arc::<str>::from("const other = 1;"));
    assert!(service.compiled_snapshot.get_mut().is_none());
    service.quick_info("other.ts", 7);
    assert!(service.compiled_snapshot.get_mut().is_some());

    assert!(service.change("other.ts", Arc::<str>::from("const renamed = 1;")));
    assert!(service.compiled_snapshot.get_mut().is_none());
    service.quick_info("other.ts", 7);
    assert!(service.compiled_snapshot.get_mut().is_some());

    assert!(service.close("other.ts"));
    assert!(service.compiled_snapshot.get_mut().is_none());
    let _ = service.semantic_diagnostics("case.ts");
    assert!(service.compiled_snapshot.get_mut().is_some());

    service.reset();
    assert!(service.compiled_snapshot.get_mut().is_none());
}

#[test]
fn capability_scope_prefers_adjacent_starts_and_nested_right_edges() {
    let adjacent = "const g = `plain`;veryLongSiblingName;const veryLongSiblingName = 1;";
    let adjacent_reference = adjacent.find("veryLongSiblingName").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("adjacent.ts", Arc::<str>::from(adjacent));

    let definition = service
        .definition_and_bound_span("adjacent.ts", adjacent_reference)
        .expect("the adjacent statement start must not inherit the prior nonclaim");
    assert_eq!(definition.definitions.len(), 1);
    assert_eq!(definition.definitions[0].name, "veryLongSiblingName");

    let nested = "function shell(bad: ){const sibling:string='x';sibling}";
    let nested_reference = nested.rfind("sibling").unwrap() as u32;
    let mut service = LanguageService::new(CompilerOptions::default());
    service.open("nested.ts", Arc::<str>::from(nested));
    for offset in [nested_reference, nested_reference + "sibling".len() as u32] {
        let definition = service
            .definition_and_bound_span("nested.ts", offset)
            .expect("a nested statement owns both its token and right-edge query");
        assert_eq!(definition.definitions.len(), 1);
        assert_eq!(definition.definitions[0].name, "sibling");
    }
}

#[test]
fn quick_info_keeps_same_offset_merged_interfaces_across_root_orders() {
    let name_start = "interface ".len() as u32;
    for paths in [["alpha.ts", "omega.ts"], ["omega.ts", "alpha.ts"]] {
        let roots = paths
            .into_iter()
            .map(|path| {
                let source = match path {
                    "alpha.ts" => "interface Shared { alpha: number; }",
                    "omega.ts" => "interface Shared { omega: string; }",
                    _ => unreachable!(),
                };
                SourceInput::new(path, Arc::<str>::from(source))
            })
            .collect();
        let output = Compiler::new().compile(roots, &CompilerOptions::default());
        let index = navigation::NavigationIndex::build(&output);

        for path in paths {
            let info = index
                .quick_info(path, name_start)
                .expect("each merged declaration keeps its file-local quick info");
            assert_eq!(info.kind, "interface");
            assert_eq!(
                info.text_span,
                TextSpan {
                    start: name_start,
                    length: "Shared".len() as u32,
                }
            );
            assert_eq!(info.display, "interface Shared");
        }
    }
}

#[test]
fn navigation_keys_follow_bound_declaration_groups_across_root_orders() {
    let sources = [
        (
            "class.ts",
            "class Dual {}\nnew Dual();\nlet instance: Dual;",
        ),
        (
            "interface.ts",
            "interface Dual { value: number }\nlet other: Dual;",
        ),
        (
            "enum.ts",
            "enum Recovered { Value }\nRecovered;\nlet recovered: Recovered;",
        ),
        (
            "meanings.ts",
            "interface Separate {}\nconst Separate = 1;\nlet typed: Separate;\nSeparate;",
        ),
        (
            "dep.ts",
            "export const Both = 1; export interface OnlyType {}",
        ),
        (
            "import.ts",
            concat!(
                "import { Both } from './dep';\n",
                "import type { OnlyType } from './dep';\n",
                "Both; let imported: Both; let typeOnly: OnlyType;",
            ),
        ),
        ("module-a.ts", "export const Local = 1; Local;"),
        ("module-b.ts", "export const Local = 2; Local;"),
        ("script-a.ts", "const Shared = 1; Shared;"),
        ("script-b.ts", "Shared;"),
    ];

    for reversed in [false, true] {
        let mut roots = sources
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect::<Vec<_>>();
        if reversed {
            roots.reverse();
        }
        let output = Compiler::new().compile(roots, &CompilerOptions::default());
        let index = navigation::NavigationIndex::build(&output);

        let class_source = sources[0].1;
        let class_reference = class_source.find("new Dual").unwrap() as u32 + 4;
        let class_definition = index
            .definition("class.ts", class_reference)
            .expect("the value side of a class keeps the merged declaration group");
        assert_eq!(class_definition.definitions.len(), 2);
        assert_eq!(
            class_definition
                .definitions
                .iter()
                .map(|definition| (definition.file_name.as_str(), definition.kind.as_str()))
                .collect::<Vec<_>>(),
            [("class.ts", "class"), ("interface.ts", "interface")]
        );
        let class_type_reference = class_source.rfind("Dual").unwrap() as u32 + 1;
        assert_eq!(
            index.references("class.ts", class_type_reference)[0]
                .references
                .len(),
            5
        );

        let enum_source = sources[2].1;
        let enum_type_reference = enum_source.rfind("Recovered").unwrap() as u32 + 1;
        let enum_definition = index
            .definition("enum.ts", enum_type_reference)
            .expect("the recovered enum type side shares its value-side authored span");
        assert_eq!(enum_definition.definitions.len(), 1);
        assert_eq!(enum_definition.definitions[0].kind, "module");
        assert_eq!(
            index.references("enum.ts", enum_type_reference)[0]
                .references
                .len(),
            3
        );

        let meanings_source = sources[3].1;
        let type_reference = meanings_source.find("typed: Separate").unwrap() as u32 + 8;
        let value_reference = meanings_source.rfind("Separate").unwrap() as u32 + 1;
        let type_definition = index.definition("meanings.ts", type_reference).unwrap();
        let value_definition = index.definition("meanings.ts", value_reference).unwrap();
        assert_eq!(type_definition.definitions.len(), 1);
        assert_eq!(type_definition.definitions[0].kind, "interface");
        assert_eq!(value_definition.definitions.len(), 1);
        assert_eq!(value_definition.definitions[0].kind, "const");
        assert_eq!(
            index.references("meanings.ts", type_reference)[0]
                .references
                .len(),
            2
        );
        assert_eq!(
            index.references("meanings.ts", value_reference)[0]
                .references
                .len(),
            2
        );

        let import_source = sources[5].1;
        let imported_type = import_source.find("imported: Both").unwrap() as u32 + 10;
        let type_only = import_source.rfind("OnlyType").unwrap() as u32 + 1;
        assert_eq!(
            index.references("import.ts", imported_type)[0]
                .references
                .len(),
            3
        );
        assert_eq!(
            index.references("import.ts", type_only)[0].references.len(),
            2
        );

        for (path, source) in [sources[6], sources[7]] {
            let reference = source.rfind("Local").unwrap() as u32 + 1;
            let references = &index.references(path, reference)[0].references;
            assert_eq!(references.len(), 2);
            assert!(
                references
                    .iter()
                    .all(|reference| reference.file_name == path)
            );
        }

        let shared_reference = sources[9].1.find("Shared").unwrap() as u32 + 1;
        let shared = &index.references("script-b.ts", shared_reference)[0].references;
        assert_eq!(shared.len(), 3);
        assert_eq!(
            shared
                .iter()
                .map(|reference| reference.file_name.as_str())
                .collect::<Vec<_>>(),
            ["script-a.ts", "script-a.ts", "script-b.ts"]
        );
    }
}
