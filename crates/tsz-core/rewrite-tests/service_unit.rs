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
    assert_eq!(first.semantic_completion, SemanticCompletion::Deferred);
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
        let index = navigation::NavigationIndex::build(&output.program);

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
