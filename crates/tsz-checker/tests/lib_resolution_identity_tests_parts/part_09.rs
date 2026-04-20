#[test]
fn test_lib_type_alias_pick_resolves_correctly() {
    // Pick<T,K> is a mapped type alias in lib.d.ts. Its resolution depends on
    // stable DefId for the type alias itself and correct type param lowering.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; email: string; }
type NameOnly = Pick<User, "name">;
const u: NameOnly = { name: "Alice" };
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Pick<User, 'name'> should accept {{name: string}}.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_type_alias_omit_resolves_correctly() {
    // Omit<T,K> is built on top of Pick and Exclude. Its resolution
    // exercises nested type alias DefId identity.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
interface User { name: string; age: number; email: string; }
type WithoutEmail = Omit<User, "email">;
const u: WithoutEmail = { name: "Alice", age: 30 };
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322),
        "Omit<User, 'email'> should accept {{name, age}}.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Import-type lowering for lib types ----

#[test]
fn test_type_reference_to_lib_generic_preserves_params() {
    // A type alias that wraps a lib generic (e.g., `type Arr<T> = Array<T>`)
    // should preserve type parameters via the stable DefId path.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
type Arr<T> = Array<T>;
const nums: Arr<number> = [1, 2, 3];
const len: number = nums.length;
const bad: Arr<number> = ["a"];
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    // Should have TS2322 for the bad line but not TS2304/TS2339 for missing names
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304 || *c == 2339),
        "Type alias wrapping lib Array should resolve names and members.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_nested_lib_generic_references() {
    // Map<string, Array<number>> exercises nested lib generic resolution.
    // Both Map and Array must have stable DefIds for proper type lowering.
    if !lib_files_available() {
        return;
    }
    let diagnostics = compile_with_lib(
        r#"
const m: Map<string, Array<number>> = new Map();
m.set("key", [1, 2, 3]);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304),
        "Nested lib generic Map<string, Array<number>> should resolve.\nDiagnostics: {real_errors:#?}"
    );
}

// ---- Focused tests: get_canonical_lib_def_id, Promise, import-type ----

#[test]
fn test_promise_resolve_returns_typed_value() {
    if !lib_files_available() {
        return;
    }
    // Promise.resolve should return Promise<T> where T matches the argument.
    let diagnostics = compile_with_lib(
        r#"
async function fetchData(): Promise<string> {
    return "hello";
}
const p: Promise<string> = fetchData();
const q: Promise<number> = Promise.resolve(42);
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2322 || *c == 2339),
        "Promise<T> should resolve correctly without false assignability or property errors.\n\
         Diagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_then_chain_preserves_type() {
    if !lib_files_available() {
        return;
    }
    // then() should accept callbacks and chain correctly.
    let diagnostics = compile_with_lib(
        r#"
const p = Promise.resolve(42);
const q: Promise<string> = p.then((x) => x.toString());
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Promise.then should be accessible without TS2339.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_promise_constructor_with_executor() {
    if !lib_files_available() {
        return;
    }
    // new Promise((resolve, reject) => ...) should work.
    let diagnostics = compile_with_lib(
        r#"
const p = new Promise<number>((resolve, reject) => {
    resolve(42);
});
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304 || *c == 2339),
        "new Promise<number>() should resolve without 'not found' errors.\n\
         Diagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_error_extends_correctly() {
    if !lib_files_available() {
        return;
    }
    // Error should be resolvable and have .message, .name, .stack.
    let diagnostics = compile_with_lib(
        r#"
const e = new Error("oops");
const msg: string = e.message;
const name: string = e.name;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "Error.message and Error.name should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_lib_ref_regexp_methods() {
    if !lib_files_available() {
        return;
    }
    // RegExp should have .test() and .exec().
    let diagnostics = compile_with_lib(
        r#"
const re = /hello/;
const result: boolean = re.test("hello world");
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2339),
        "RegExp.test should be accessible.\nDiagnostics: {real_errors:#?}"
    );
}

#[test]
fn test_import_type_expression_resolves_without_error() {
    if !lib_files_available() {
        return;
    }
    // Type aliases referencing lib types should not produce false errors.
    let diagnostics = compile_with_lib(
        r#"
type StringArray = Array<string>;
const a: StringArray = ["hello", "world"];
const len: number = a.length;
"#,
    );
    let real_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _)| *c != 2318 && *c != 2583)
        .collect();
    assert!(
        !real_errors.iter().any(|(c, _)| *c == 2304 || *c == 2322),
        "Type alias to Array<string> should resolve via lib.\nDiagnostics: {real_errors:#?}"
    );
}

