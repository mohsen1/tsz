//! A nameless JSDoc `@typedef` still declares a type named after its host.
//!
//! `/** @typedef {string} */ A.B.C;` (or `A.B.C = { ... }`) carries no name in
//! the tag itself. That is a grammar error — tsc reports `TS1003` at the type
//! expression's closing brace, and tsz matches it. But tsc *also* declares the
//! type alias, naming it after the declaration the comment annotates, so `A.B.C`
//! is a real type-space name and `A` is a legitimate namespace qualifier.
//!
//! tsz recognised only the *named* dotted form (`@typedef {string} A.B.C`) and
//! so reported a spurious `TS2503` ("Cannot find namespace 'A'") for the
//! nameless spelling, on top of the correct `TS1003`.
//!
//! The negative controls matter as much as the positives here: the exemption is
//! for a host declaration that actually matches the referenced qualified name.
//! A plain value expando with no typedef anywhere must still report `TS2503`.
//!
//! Binder and file names are varied across cases so the behaviour follows
//! structure, not identifier text.

use std::path::PathBuf;
use std::process::Command;

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

/// Run `tsz -p tsconfig.json` over `files`, returning combined stdout+stderr.
/// `allowJs`/`checkJs` are on because every case here is a `.js` JSDoc shape.
fn run_tsz(files: &[(&str, &str)]) -> String {
    let Some(tsz_bin) = find_tsz_binary() else {
        return String::from("__SKIP__");
    };
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(path, contents).expect("write file");
    }
    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{ "compilerOptions": { "noEmit": true, "allowJs": true, "checkJs": true, "target": "es2015", "skipLibCheck": true } }"#,
    )
    .expect("write tsconfig.json");

    let output = Command::new(tsz_bin)
        .args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

/// The nameless tag itself is still a grammar error in tsc, so every positive
/// case here asserts `TS1003` is *kept* while `TS2503` disappears. Asserting
/// only the absence of `TS2503` would pass just as well if the whole JSDoc
/// comment stopped being scanned.
fn assert_ts1003_without_ts2503(out: &str, case: &str) {
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503"),
        "{case}: a nameless @typedef names its host declaration, so its root \
         keeps namespace meaning (no TS2503), got:\n{out}"
    );
    assert!(
        out.contains("TS1003"),
        "{case}: the nameless tag is still a grammar error and TS1003 must \
         survive the fix, got:\n{out}"
    );
}

#[test]
fn nameless_typedef_on_qualified_expression_statement_host_declares_its_name() {
    let out = run_tsz(&[(
        "alpha.js",
        r#"
var Zeta = {};
Zeta.Inner = {};
/**
 * @typedef {string}
 */
Zeta.Inner.Alpha;
/** @type {Zeta.Inner.Alpha} */
var probe = "s";
"#,
    )]);
    assert_ts1003_without_ts2503(&out, "expression-statement host");
}

#[test]
fn nameless_typedef_on_qualified_assignment_host_declares_its_name() {
    let out = run_tsz(&[(
        "beta.js",
        r#"
var Qq = {};
/**
 * @typedef {number}
 */
Qq.Beta = { z: 1 };
/** @type {Qq.Beta} */
var probe = 1;
"#,
    )]);
    assert_ts1003_without_ts2503(&out, "assignment host");
}

#[test]
fn nameless_typedef_host_is_visible_across_files() {
    // The declaring file and the referencing file are different, which is the
    // shape the `jsEnumCrossFileExport` corpus row exercises.
    let out = run_tsz(&[
        (
            "decl.js",
            r#"
var Vendor = {};
Vendor.Metrics = {};
/**
 * @typedef {string}
 */
Vendor.Metrics.Label = { x: 12 };
"#,
        ),
        (
            "use.js",
            r#"
/**
 * @type {Vendor.Metrics.Label}
 */
var probe = "ok";
"#,
        ),
    ]);
    assert_ts1003_without_ts2503(&out, "cross-file host");
}

#[test]
fn named_dotted_typedef_still_exempts_its_root() {
    // Pre-existing exemption for the *named* spelling; the nameless sibling
    // must not have displaced it. No TS1003 here — this tag has a name.
    let out = run_tsz(&[(
        "gamma.js",
        r#"
var Wide = {};
/**
 * @typedef {string} Wide.Named
 */
/** @type {Wide.Named} */
var probe = "s";
"#,
    )]);
    if out == "__SKIP__" {
        return;
    }
    assert!(
        !out.contains("TS2503") && !out.contains("TS1003"),
        "a named dotted @typedef declares its own name and is well-formed, \
         got:\n{out}"
    );
}

#[test]
fn plain_value_expando_without_any_typedef_still_reports_ts2503() {
    // The load-bearing negative control. `Root` grows a value-space expando
    // member and nothing declares a type, so tsc reports TS2503 and tsz must
    // keep doing so. If this ever goes quiet the exemption has become a blanket
    // suppression.
    let out = run_tsz(&[(
        "delta.js",
        r#"
var Root = {};
Root.Member = class {};
/** @type {Root.Member} */
var probe = null;
"#,
    )]);
    if out == "__SKIP__" {
        return;
    }
    assert!(
        out.contains("TS2503"),
        "a value-only expando root is not a namespace and must still report \
         TS2503, got:\n{out}"
    );
}

#[test]
fn nameless_typedef_declaring_a_different_name_does_not_exempt_an_unrelated_member() {
    // The namespace-root exemption itself is keyed to the qualified name
    // actually declared, not to the mere presence of a nameless typedef
    // somewhere in the file. But a *different* tsc fact dominates this exact
    // shape: the nameless tag's own TS1003 is a genuine parse-time error in
    // tsc, which suppresses every semantic diagnostic program-wide — so
    // `Root.Undeclared`'s TS2503 disappears too, verified against the pinned
    // oracle. This no longer probes the exemption's width (nothing can, once
    // a nameless typedef's TS1003 is anywhere in the program); it pins the
    // combined behavior so it cannot silently regress.
    let out = run_tsz(&[(
        "epsilon.js",
        r#"
var Root = {};
Root.Other = {};
/**
 * @typedef {string}
 */
Root.Other.Declared;
Root.Undeclared = class {};
/** @type {Root.Undeclared} */
var probe = null;
"#,
    )]);
    assert_ts1003_without_ts2503(&out, "unrelated member alongside a nameless typedef");
}

#[test]
fn nameless_typedef_with_a_bare_host_does_not_manufacture_a_namespace() {
    // Same reasoning as above for an undotted host (`Solo;`): the nameless
    // tag's TS1003 suppresses the file's other semantic diagnostics program
    // -wide in tsc, regardless of whether `Solo`'s own exemption would ever
    // reach `Root.Member`.
    let out = run_tsz(&[(
        "zeta.js",
        r#"
var Root = {};
/**
 * @typedef {string}
 */
Solo;
Root.Member = class {};
/** @type {Root.Member} */
var probe = null;
"#,
    )]);
    assert_ts1003_without_ts2503(&out, "unrelated qualified reference alongside a bare host");
}
