use std::sync::Arc;

use crate::bind::bind_source;
use crate::source::{FileId, SourceText};
use crate::syntax::parse_source;

use super::*;

fn file(id: u32, path: &str) -> ProgramFile {
    let source = SourceText::new(FileId(id), PathBuf::from(path), Arc::from("export {};\n"));
    let parsed = parse_source(&source);
    ProgramFile {
        bindings: bind_source(source.id, &parsed.unit),
        source,
        syntax: parsed.unit,
    }
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
