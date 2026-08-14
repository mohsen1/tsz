//! Regression tests for #17456: a class must relate to *itself*.
//!
//! When a class is declared inside a namespace, `build_namespace_object_type`
//! resolves each exported value member's type, which repeatedly (re)builds the
//! class constructor type. Constructor building temporarily installs a
//! *fields-only* provisional instance (declared instance properties, no
//! methods) into `symbol_instance_types` so in-flight TYPE references resolve
//! (#17305). Before the fix, that provisional overwrote an already-registered
//! COMPLETE instance and the restore could leave it in place, so a
//! self-referencing `new C()` inside one of `C`'s own members resolved to a
//! member-less `C`. `return dup;` then related that member-less instance
//! against the complete `C` and emitted a spurious `TS2740` — "Type 'C' is
//! missing the following properties from type 'C'" — where `tsc` is silent.
//!
//! The trigger needs the real fixture's shape (a 576-line
//! `parserRealSource14.ts` would not minimize to two lines): a namespace-nested
//! class with declared instance fields, several *un-annotated* self-referential
//! methods (their return types are body-inferred, exercising the re-entrant
//! instance requests), an unresolved type reference in the member signatures,
//! and one method that returns a fresh `new C()`. Binder names are varied
//! across cases so the fix cannot key off any identifier.

use tsz_binder::BinderState;
use tsz_checker::{context::CheckerOptions, diagnostics::Diagnostic, state::CheckerState};
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn collect_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            strict: false,
            ..Default::default()
        },
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn code_count(source: &str, code: u32) -> usize {
    collect_diagnostics(source)
        .iter()
        .filter(|d| d.code == code)
        .count()
}

/// The reported witness (renamed off `AstPath`): a namespace-nested class whose
/// `copy()` returns `new C()` must not self-relate to a false `TS2740`.
#[test]
fn namespace_class_self_returning_method_does_not_self_relate() {
    let src = r#"
namespace Graph {
    export class Cursor {
        public nodes: Graph.Vertex[] = [];
        public depth: number = -1;
        public copy(): Cursor {
            var dup = new Cursor();
            return dup;
        }
        public head(): Graph.Vertex { var h = this.peek(); this.rise(); return h; }
        public seek(v: Graph.Vertex) { this.depth = this.nodes.length; this.nodes.push(v); }
        public rise() { this.depth--; }
        public peek() { return <Graph.Vertex>Cursor.at(this.nodes, this.nodes.length - (this.depth + 1)); }
        public at(i: number): Graph.Vertex { return this.nodes[i]; }
        static at(items: any[], index: number): any { return items[index]; }
    }
}
"#;
    assert_eq!(
        code_count(src, 2740),
        0,
        "a class must relate to itself; `return new Cursor()` is not TS2740"
    );
    // Sanity: the shape is intact (the unresolved `Graph.Vertex` is expected and
    // is what exercises the buggy path). If this drops to zero the case has
    // stopped covering the regression.
    assert!(
        code_count(src, 2694) > 0,
        "expected the unresolved `Graph.Vertex` references that drive the case"
    );
}

/// Same shape, every binder name changed and the returned local aliasing the
/// method name (as the original fixture did with `var clone`): the fix must not
/// depend on any identifier.
#[test]
fn renamed_binders_and_shadowing_local_still_self_relate() {
    let src = r#"
namespace Tree {
    export class Walker {
        public frames: Tree.Leaf[] = [];
        public level: number = -1;
        public clone(): Walker {
            var clone = new Walker();
            return clone;
        }
        public drop(): Tree.Leaf { var top = this.node(); this.up(); return top; }
        public add(leaf: Tree.Leaf) { this.level = this.frames.length; this.frames.push(leaf); }
        public up() { this.level--; }
        public node() { return <Tree.Leaf>Walker.pick(this.frames, this.frames.length - (this.level + 1)); }
        public get(i: number): Tree.Leaf { return this.frames[i]; }
        static pick(items: any[], index: number): any { return items[index]; }
    }
}
"#;
    assert_eq!(
        code_count(src, 2740),
        0,
        "renamed class/field/method must still self-relate cleanly"
    );
}

/// A generic namespace-nested class in the same shape self-relates too: the
/// provisional-instance install/restore is not gated on arity.
#[test]
fn generic_namespace_class_self_relates() {
    let src = r#"
namespace Store {
    export class Bag<T> {
        public data: Store.Slot[] = [];
        public size: number = 0;
        public dup(): Bag<T> {
            var copy = new Bag<T>();
            return copy;
        }
        public first(): Store.Slot { var v = this.top(); this.shrink(); return v; }
        public put(s: Store.Slot) { this.size = this.data.length; this.data.push(s); }
        public shrink() { this.size--; }
        public top() { return <Store.Slot>Bag.grab(this.data, this.data.length - this.size); }
        public get(i: number): Store.Slot { return this.data[i]; }
        static grab(items: any[], index: number): any { return items[index]; }
    }
}
"#;
    assert_eq!(
        code_count(src, 2740),
        0,
        "generic self-returning method must self-relate cleanly"
    );
}

/// Negative control: the fix must not suppress a *genuine* missing-property
/// TS2740. A source object literal that is truly missing the class's members is
/// still rejected when assigned to the class instance type.
#[test]
fn genuinely_missing_members_still_error() {
    let src = r#"
namespace Shape {
    export class Point {
        public x: number = 0;
        public y: number = 0;
        public move(): Point { var p = new Point(); return p; }
        public dist(): number { return this.x + this.y; }
        public flip() { this.x = -this.x; }
        public scale(k: number) { this.x = this.x * k; }
        public norm() { return this.dist(); }
    }
    export var bad: Point = { x: 0 };
}
"#;
    assert!(
        code_count(src, 2740) >= 1,
        "an object literal missing the class methods must still be rejected"
    );
}
