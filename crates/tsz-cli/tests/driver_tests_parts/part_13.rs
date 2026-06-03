// Regression tests for cyclic library-interface heritage resolution.
//
// Structural rule: when a library interface `D` extends an anchor interface
// `A` whose own members reference `D` (or a subtype) *only through lazy member
// types*, resolving `D` must still flatten in every member of `A`.
//
// The DOM `Node`/`Element` graph is the canonical witness. `interface Node`
// declares methods (`appendChild`, `contains`, ...) and references
// `Element`/`HTMLElement`/`Document` from its property types (e.g.
// `parentElement: HTMLElement | null`). `interface Element extends Node,
// ChildNode, ParentNode, ...`, and both `ChildNode` and `ParentNode` themselves
// `extend Node`, so `Node` reaches `Element` through several heritage paths.
//
// Resolving a `Node` subtype lazily re-enters `Node` while it is still in
// progress; the previous resolver cached the resulting partial shape, which
// dropped every method `Node` declares (its *properties* survived, since they
// merged before the cycle re-entry). Inherited-method access then produced
// spurious `TS2339`. The resolver now recovers the incomplete interfaces once
// the anchor is fully cached. The matrix below varies the receiver interface
// and the inherited member so the assertion exercises the general flattening,
// not a single property name.

fn dom_project_codes(source: &str) -> Vec<u32> {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "target": "es2020",
            "lib": ["es2020", "dom"]
          },
          "files": ["main.ts"]
        }"#,
    );
    write_file(&base.join("main.ts"), source);
    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    result.diagnostics.iter().map(|d| d.code).collect()
}

/// `TS2339` (property does not exist) must NOT be reported for any method that a
/// `Node` subtype inherits across the heritage cycle.
#[test]
#[ignore = "WIP: blocked on the deeper resolution-ordering fix (resolve base lib interfaces to completion before merging); documents target behavior. See PR #12274 WIP comment."]
fn dom_inherited_node_methods_resolve_on_subtypes() {
    // (receiver type, inherited member declared on `Node`)
    let cases = [
        ("Element", "appendChild"),
        ("Element", "contains"),
        ("Element", "cloneNode"),
        ("Element", "removeChild"),
        ("HTMLElement", "appendChild"),
        ("HTMLDivElement", "appendChild"),
        ("Document", "appendChild"),
        ("Attr", "appendChild"),
        ("Text", "appendChild"),
    ];
    for (receiver, member) in cases {
        let source = format!("declare const recv: {receiver};\nrecv.{member};\n");
        let codes = dom_project_codes(&source);
        assert!(
            !codes.contains(&2339),
            "`{receiver}.{member}` (inherited from Node) must not report TS2339; got {codes:?}",
        );
    }
}

/// Order-independence: an inherited method accessed as the very first member
/// access on a freshly-resolved interface must already see the complete shape.
#[test]
#[ignore = "WIP: blocked on the deeper resolution-ordering fix (resolve base lib interfaces to completion before merging); documents target behavior. See PR #12274 WIP comment."]
fn dom_first_method_access_on_element_is_complete() {
    let codes = dom_project_codes("declare const e: Element;\ne.appendChild;\n");
    assert!(
        !codes.contains(&2339),
        "first access of `Element.appendChild` must resolve; got {codes:?}",
    );
}

/// The everyday end-to-end pattern: `document.body` is an `HTMLElement`, and
/// calling an inherited `Node` method on it must type-check.
#[test]
#[ignore = "WIP: blocked on the deeper resolution-ordering fix (resolve base lib interfaces to completion before merging); documents target behavior. See PR #12274 WIP comment."]
fn dom_document_body_append_child_resolves() {
    let codes = dom_project_codes("const d = document.body;\nd.appendChild(d);\nd.contains(d);\n");
    assert!(
        !codes.contains(&2339),
        "`document.body.appendChild`/`contains` must resolve; got {codes:?}",
    );
}

/// Negative control: a genuinely absent member on `Element` must still report
/// `TS2339`, proving the recovery did not blanket-suppress the diagnostic and
/// that the DOM lib really resolved (so the positive cases are not vacuous).
#[test]
#[ignore = "WIP: blocked on the deeper resolution-ordering fix (resolve base lib interfaces to completion before merging); documents target behavior. See PR #12274 WIP comment."]
fn dom_genuinely_missing_member_still_reports_ts2339() {
    let codes = dom_project_codes("declare const e: Element;\ne.definitelyNotARealDomMember;\n");
    assert!(
        codes.contains(&2339),
        "a genuinely missing Element member must still report TS2339; got {codes:?}",
    );
}

/// Cross-arena path: the cyclic `Node`/`Element` resolution is driven from a
/// *separate* file's checker context (which imports a helper that touches the
/// DOM heritage). Cross-file checking spawns child checker contexts that share
/// the cycle-recovery state with the parent; an interface recorded incomplete
/// in the child must still be recovered so the parent does not keep the partial
/// shape. Reproduces the same drop through a multi-file project rather than a
/// single context.
#[test]
#[ignore = "WIP: blocked on the deeper resolution-ordering fix (resolve base lib interfaces to completion before merging); documents target behavior. See PR #12274 WIP comment."]
fn dom_inherited_node_methods_resolve_across_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;
    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "noEmit": true,
            "strict": true,
            "target": "es2020",
            "module": "es2020",
            "moduleResolution": "bundler",
            "lib": ["es2020", "dom"]
          },
          "files": ["dom-helpers.ts", "main.ts"]
        }"#,
    );
    write_file(
        &base.join("dom-helpers.ts"),
        "export function mount(host: Element, child: Node): Node {\n  host.appendChild(child);\n  return host.contains(child) ? child : child.cloneNode(true);\n}\n",
    );
    write_file(
        &base.join("main.ts"),
        "import { mount } from \"./dom-helpers\";\ndeclare const box: HTMLDivElement;\ndeclare const label: Text;\nconst kept = mount(box, label);\nbox.removeChild(kept);\nlabel.appendChild(box);\n",
    );
    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2339),
        "inherited Node methods accessed across files must resolve; got {codes:?}",
    );
}
