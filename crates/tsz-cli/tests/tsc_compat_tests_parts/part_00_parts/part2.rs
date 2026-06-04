#[test]
fn batch_mode_uses_project_cwd_for_jsdoc_required_constructor_types() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("batch_jsdoc_required_constructor").expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node.d.ts"),
        "declare function require(id: string): any;\ndeclare var module: any, exports: any;\n",
    );
    write_file(
        &base.join("a-ext.js"),
        "exports.A = function () {\n    this.x = 1;\n};\n",
    );
    write_file(
        &base.join("a.js"),
        "const { A } = require(\"./a-ext\");\n\n/** @param {A} p */\nfunction a(p) { p.x; }\n",
    );
    write_file(
        &base.join("b-ext.js"),
        "exports.B = class {\n    constructor() {\n        this.x = 1;\n    }\n};\n",
    );
    write_file(
        &base.join("b.js"),
        "const { B } = require(\"./b-ext\");\n\n/** @param {B} p */\nfunction b(p) { p.x; }\n",
    );
    write_file(
        &base.join("c-ext.js"),
        "export function C() {\n    this.x = 1;\n}\n",
    );
    write_file(
        &base.join("c.js"),
        "const { C } = require(\"./c-ext\");\n\n/** @param {C} p */\nfunction c(p) { p.x; }\n",
    );
    write_file(
        &base.join("d-ext.js"),
        "export var D = function() {\n    this.x = 1;\n};\n",
    );
    write_file(
        &base.join("d.js"),
        "const { D } = require(\"./d-ext\");\n\n/** @param {D} p */\nfunction d(p) { p.x; }\n",
    );
    write_file(
        &base.join("e-ext.js"),
        "export class E {\n    constructor() {\n        this.x = 1;\n    }\n}\n",
    );
    write_file(
        &base.join("e.js"),
        "const { E } = require(\"./e-ext\");\n\n/** @param {E} p */\nfunction e(p) { p.x; }\n",
    );
    write_file(
        &base.join("f.js"),
        "var F = function () {\n    this.x = 1;\n};\n\n/** @param {F} p */\nfunction f(p) { p.x; }\n",
    );
    write_file(
        &base.join("g.js"),
        "function G() {\n    this.x = 1;\n}\n\n/** @param {G} p */\nfunction g(p) { p.x; }\n",
    );
    write_file(
        &base.join("h.js"),
        "class H {\n    constructor() {\n        this.x = 1;\n    }\n}\n\n/** @param {H} p */\nfunction h(p) { p.x; }\n",
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "allowJs": true,
    "checkJs": true,
    "noEmit": true,
    "module": "commonjs"
  },
  "include": ["*.ts", "*.tsx", "*.js", "*.jsx", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx"],
  "exclude": ["node_modules"]
}"#,
    );

    let mut child = Command::new(tsz_bin)
        .arg("--batch")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tsz --batch");

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("batch stdin");
        writeln!(stdin, "{}", base.display()).expect("write batch project");
    }

    let output = child.wait_with_output().expect("wait for tsz --batch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "batch worker failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("TS2339"),
        "expected no TS2339 from JSDoc constructor param in batch mode, got:\n{stdout}\n{stderr}"
    );
}

#[test]
fn declaration_emit_keyword_destructuring_rest_omits_keyword_key() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("keyword_destructuring_rest_dts").expect("temp dir");
    let base = temp.path.as_path();
    let out_dir = base.join("out");

    write_file(
        &base.join("input.ts"),
        r#"
type P = {
    enum: boolean;
    function: boolean;
    abstract: boolean;
    async: boolean;
    await: boolean;
    one: boolean;
};

function f1({ enum: _enum, ...rest }: P) {
    return rest;
}

function f2({ function: _function, ...rest }: P) {
    return rest;
}

function f3({ abstract: _abstract, ...rest }: P) {
    return rest;
}

function f4({ async: _async, ...rest }: P) {
    return rest;
}

function f5({ await: _await, ...rest }: P) {
    return rest;
}
"#,
    );

    let output = Command::new(&tsz_bin)
        .args([
            "--ignoreConfig",
            "--declaration",
            "--emitDeclarationOnly",
            "--target",
            "es2015",
            "--outDir",
            out_dir.to_str().expect("utf-8 temp path"),
            "input.ts",
        ])
        .current_dir(base)
        .output()
        .expect("failed to run tsz");
    assert!(
        output.status.success(),
        "tsz failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let dts = std::fs::read_to_string(out_dir.join("input.d.ts")).expect("declaration output");
    for (function_name, omitted_key) in [
        ("f1", "enum"),
        ("f2", "function"),
        ("f3", "abstract"),
        ("f4", "async"),
        ("f5", "await"),
    ] {
        let signature = format!("declare function {function_name}");
        let start = dts
            .find(&signature)
            .unwrap_or_else(|| panic!("Expected {signature} in declaration output: {dts}"));
        let end = dts[start..]
            .find("};")
            .map_or(dts.len(), |offset| start + offset);
        let emitted_function = &dts[start..end];

        assert!(
            !emitted_function.contains(&format!("    {omitted_key}: boolean;")),
            "Expected `{omitted_key}` to be omitted from {function_name} rest return type: {dts}"
        );
    }
}

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
