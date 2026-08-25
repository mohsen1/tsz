use std::sync::Arc;

use crate::bind::bind_source;
use crate::program::{CapabilityContext, CompilerOptions};
use crate::source::{FileId, SourceText};
use crate::syntax::parse_source;

use super::*;

fn file(id: u32, path: &str) -> ProgramFile {
    file_with_text(id, path, "export {};\n")
}

fn file_with_text(id: u32, path: &str, text: &str) -> ProgramFile {
    let source = SourceText::new(FileId(id), PathBuf::from(path), Arc::from(text));
    let parsed = parse_source(&source);
    ProgramFile {
        bindings: bind_source(source.id, &parsed.unit),
        source,
        syntax: parsed.unit,
    }
}

fn plan(files: &[ProgramFile], options: &CompilerOptions) -> EmitPlan {
    let capabilities = CapabilityAnalysis::derive(files, options, CapabilityContext::default());
    EmitPlan::for_program(files, options, &ProjectProvenance::default(), &capabilities)
}

#[test]
fn command_line_programs_map_from_the_common_source_directory() {
    let files = [file(0, "src/one/index.ts"), file(1, "src/two/index.ts")];
    let paths = EmitPaths::for_program(&files, None, None);

    assert_eq!(
        paths
            .output_target(&files[0].source, Some(Path::new("dist")), false)
            .path,
        Path::new("dist/one/index.js")
    );
    assert_eq!(
        paths
            .output_target(&files[1].source, Some(Path::new("types")), true)
            .path,
        Path::new("types/two/index.d.ts")
    );
}

#[test]
fn configured_programs_map_from_the_entry_project_directory() {
    let files = [
        file(0, "/project/src/one/same.mts"),
        file(1, "/project/src/two/same.cts"),
    ];
    let paths = EmitPaths::for_program(&files, Some(Path::new("/project")), None);

    assert_eq!(
        paths
            .output_target(&files[0].source, Some(Path::new("/project/dist")), false,)
            .path,
        Path::new("/project/dist/src/one/same.mjs")
    );
    assert_eq!(
        paths
            .output_target(&files[1].source, Some(Path::new("/project/types")), true,)
            .path,
        Path::new("/project/types/src/two/same.d.cts")
    );
}

#[test]
fn configured_sources_outside_the_emit_root_stay_beside_the_source() {
    let files = [
        file(0, "/workspace/one/index.ts"),
        file(1, "/workspace/two/index.ts"),
    ];
    let paths = EmitPaths::for_program(&files, Some(Path::new("/workspace/project")), None);

    assert_eq!(
        paths
            .output_target(
                &files[0].source,
                Some(Path::new("/workspace/project/dist")),
                false,
            )
            .path,
        Path::new("/workspace/one/index.js")
    );
    assert_eq!(
        paths
            .output_target(
                &files[1].source,
                Some(Path::new("/workspace/project/dist")),
                false,
            )
            .path,
        Path::new("/workspace/two/index.js")
    );
}

#[test]
fn product_planning_is_root_order_independent_with_both_optional_maps() {
    let options = CompilerOptions {
        declaration: true,
        source_map: true,
        declaration_map: true,
        root_dir: Some(PathBuf::from("src")),
        out_dir: Some(PathBuf::from("dist")),
        declaration_dir: Some(PathBuf::from("types")),
        ..CompilerOptions::default()
    };
    let forward_files = [file(0, "src/one.ts"), file(1, "src/nested/two.ts")];
    let reverse_files = [file(1, "src/nested/two.ts"), file(0, "src/one.ts")];
    let forward = plan(&forward_files, &options);
    let reverse = plan(&reverse_files, &options);

    for id in [FileId(0), FileId(1)] {
        assert_eq!(
            forward.for_file(id).javascript,
            reverse.for_file(id).javascript
        );
        assert_eq!(
            forward.for_file(id).declaration,
            reverse.for_file(id).declaration
        );
    }
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(
        forward.has_blocked_products(),
        reverse.has_blocked_products()
    );
    assert_eq!(
        forward.for_file(FileId(0)).javascript.as_deref(),
        Some(Path::new("dist/one.js"))
    );
    assert_eq!(
        forward.for_file(FileId(1)).declaration.as_deref(),
        Some(Path::new("types/nested/two.d.ts"))
    );
}

#[test]
fn capability_and_collision_gates_leave_independent_primary_products() {
    let options = CompilerOptions {
        declaration: true,
        root_dir: Some(PathBuf::from("src")),
        out_dir: Some(PathBuf::from("dist")),
        declaration_dir: Some(PathBuf::from("types")),
        ..CompilerOptions::default()
    };

    let js_only_files = [file_with_text(0, "src/call.ts", "callee<string>();")];
    let js_only = plan(&js_only_files, &options);
    assert_eq!(
        js_only.for_file(FileId(0)).javascript.as_deref(),
        Some(Path::new("dist/call.js"))
    );
    assert!(js_only.for_file(FileId(0)).declaration.is_none());

    let declaration_only_files = [file(0, "src/value.ts"), file(1, "dist/value.js")];
    let declaration_only = plan(&declaration_only_files, &options);
    assert!(declaration_only.for_file(FileId(0)).javascript.is_none());
    assert_eq!(
        declaration_only.for_file(FileId(0)).declaration.as_deref(),
        Some(Path::new("types/value.d.ts"))
    );
    assert!(declaration_only.has_blocked_products());
    assert!(declaration_only.diagnostics().iter().any(|diagnostic| {
        diagnostic.code == 5055 && diagnostic.message_text.contains("dist/value.js")
    }));
}

#[test]
fn map_collisions_block_only_the_auxiliary_products_in_either_root_order() {
    let options = CompilerOptions {
        declaration: true,
        source_map: true,
        declaration_map: true,
        root_dir: Some(PathBuf::from("src")),
        out_dir: Some(PathBuf::from("dist")),
        declaration_dir: Some(PathBuf::from("types")),
        ..CompilerOptions::default()
    };
    for files in [
        vec![
            file(0, "src/value.ts"),
            file(1, "dist/value.js.map"),
            file(2, "types/value.d.ts.map"),
        ],
        vec![
            file(2, "types/value.d.ts.map"),
            file(1, "dist/value.js.map"),
            file(0, "src/value.ts"),
        ],
    ] {
        let plan = plan(&files, &options);
        assert_eq!(
            plan.for_file(FileId(0)).javascript.as_deref(),
            Some(Path::new("dist/value.js"))
        );
        assert_eq!(
            plan.for_file(FileId(0)).declaration.as_deref(),
            Some(Path::new("types/value.d.ts"))
        );
        assert!(plan.has_blocked_products());
        for target in ["dist/value.js.map", "types/value.d.ts.map"] {
            assert!(
                plan.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code == 5055 && diagnostic.message_text.contains(target)
                }),
                "missing map collision for {target}: {:#?}",
                plan.diagnostics()
            );
        }
    }
}
