use super::{ProgramHost, SystemHost};
use crate::{Compiler, CompilerOptions, SemanticCompletion, SourceInput};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

#[test]
fn native_utf8_reads_strip_exactly_one_leading_byte_order_mark() {
    let fixture = TempDir::new().expect("tempdir");
    let path = fixture.path().join("bom.ts");
    fs::write(&path, "\u{feff}\u{feff}let value = 1;\u{feff}").expect("write source");

    let host = SystemHost::new(fixture.path());
    assert_eq!(
        host.read_file(&path).expect("read source"),
        "\u{feff}let value = 1;\u{feff}"
    );
}

#[test]
fn native_source_diagnostics_exclude_the_decoded_byte_order_mark() {
    let fixture = TempDir::new().expect("tempdir");
    let path = fixture.path().join("diagnostic.ts");
    fs::write(&path, "\u{feff}const count: number = \"wrong\";").expect("write source");
    let host = SystemHost::new(fixture.path());
    let text = host.read_file(&path).expect("read source");

    let output = Compiler::new().compile(
        vec![SourceInput::with_host_path(
            "diagnostic.ts",
            path,
            Arc::<str>::from(text),
        )],
        &CompilerOptions {
            no_emit: true,
            ..CompilerOptions::default()
        },
    );

    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
            .collect::<Vec<_>>(),
        [(2322, 6, 5)]
    );
}
