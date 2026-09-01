/// Normalize output: strip ANSI codes, normalize line endings to \n.
fn normalize_output(s: &str) -> String {
    // Strip ANSI escape codes
    let stripped = strip_ansi(s);
    // Normalize Windows line endings to Unix
    stripped.replace("\r\n", "\n")
}

#[test]
fn generic_private_class_assignment_preserves_type_arguments_in_cli_output() {
    let temp = TempDir::new("generic_private_class_assignment").expect("temp dir");
    let source = r#"
class C<T> {
    #foo: T;
    #method(): T { return this.#foo; }
    get #prop(): T { return this.#foo; }
    set #prop(value: T) { this.#foo = value; }

    bar(x: C<T>) { return x.#foo; }
    bar2(x: C<T>) { return x.#method(); }
    bar3(x: C<T>) { return x.#prop; }

    baz(x: C<number>) { return x.#foo; }
    baz2(x: C<number>) { return x.#method; }
    baz3(x: C<number>) { return x.#prop; }

    quux(x: C<string>) { return x.#foo; }
    quux2(x: C<string>) { return x.#method; }
    quux3(x: C<string>) { return x.#prop; }
}

declare let a: C<number>;
declare let b: C<string>;
a.#foo;
a.#method;
a.#prop;
a = b;
b = a;
"#;
    write_file(&temp.path.join("test.ts"), source);

    let (_, output) = run_tsz_with_exit_code(
        &temp.path,
        &[
            "--pretty",
            "false",
            "--noEmit",
            "--strict",
            "--target",
            "es6",
            "--strictPropertyInitialization",
            "false",
            "test.ts",
        ],
    )
    .expect("tsz should run");

    assert!(
        output.contains("Type 'C<string>' is not assignable to type 'C<number>'."),
        "expected C<string> -> C<number> display in CLI output, got:\n{output}"
    );
    assert!(
        output.contains("Type 'C<number>' is not assignable to type 'C<string>'."),
        "expected C<number> -> C<string> display in CLI output, got:\n{output}"
    );
    assert!(
        !output.contains("Type 'C' is not assignable to type 'C'."),
        "generic class CLI diagnostic should not erase type arguments, got:\n{output}"
    );
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip escape sequence: ESC [ ... (letter)
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Compare two outputs line by line, returning a detailed diff description.
fn diff_outputs(tsc_output: &str, tsz_output: &str) -> Option<String> {
    let tsc_lines: Vec<&str> = tsc_output.lines().collect();
    let tsz_lines: Vec<&str> = tsz_output.lines().collect();

    let mut diffs = Vec::new();

    let max_lines = tsc_lines.len().max(tsz_lines.len());
    for i in 0..max_lines {
        let tsc_line = tsc_lines.get(i).unwrap_or(&"<missing>");
        let tsz_line = tsz_lines.get(i).unwrap_or(&"<missing>");
        if tsc_line != tsz_line {
            diffs.push(format!(
                "Line {} differs:\n  tsc: {:?}\n  tsz: {:?}",
                i + 1,
                tsc_line,
                tsz_line
            ));
        }
    }

    if tsc_lines.len() != tsz_lines.len() {
        diffs.push(format!(
            "Line count: tsc={}, tsz={}",
            tsc_lines.len(),
            tsz_lines.len()
        ));
    }

    if diffs.is_empty() {
        None
    } else {
        Some(diffs.join("\n"))
    }
}

/// Check that tsc is available on the system.
fn tsc_available() -> bool {
    tsc_command()
        .and_then(|mut cmd| cmd.arg("--version").output().ok())
        .is_some()
}

/// Create a command that runs the pinned repo TypeScript compiler when available.
/// Falls back to PATH `tsc` for environments without `scripts/node_modules` installed.
fn tsc_command() -> Option<Command> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent()?.parent()?;
    let local_tsc_js = workspace_root.join("scripts/node_modules/typescript/lib/tsc.js");

    if local_tsc_js.exists() {
        let mut cmd = Command::new("node");
        cmd.arg(local_tsc_js);
        return Some(cmd);
    }

    Some(Command::new("tsc"))
}

// ===========================================================================
// Integration tests: exact match (where checker positions agree)
// ===========================================================================
