//! A `ThisType[ T ]` marker on an *enclosing* object literal's contextual type
//! types `this` inside a nested literal's members.
//!
//! `tsc`'s `getContextualThisParameter` climbs outward from the literal that
//! directly contains the member:
//!
//! ```text
//! while (type) {
//!     const thisType = getThisTypeFromContextualType(type);
//!     if (thisType) { return ...; }
//!     if (literal.parent.kind !== SyntaxKind.PropertyAssignment) break;
//!     literal = literal.parent.parent;
//!     type = getApparentTypeOfContextualType(literal);
//! }
//! ```
//!
//! tsz used to stop at the innermost literal, so the Vue options shape
//! (`ThisType[ D & M & P ] & { methods?: M }`) and the
//! `Object.defineProperties` shape (`PropDescMap[ U ] & ThisType[ T ]`) typed
//! `this` as the nested literal itself and reported spurious `TS2339` — plus
//! the `TS7023` that the resulting self-reference triggers.
//!
//! Corpus witness: `conformance/types/thisType/thisTypeInObjectLiterals2.ts`.
//!
//! These fixtures need the real `ThisType` marker interface, so they run
//! through `check_source_with_libs` with the default libs rather than the
//! no-lib `check_source_*` helpers: without a lib, `ThisType` never resolves
//! to the registered marker definition and the negative controls stop
//! discriminating.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostics_text(source: &str) -> Vec<String> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| format!("TS{}: {}", d.code, d.message_text))
    .collect()
}

fn codes(source: &str) -> Vec<u32> {
    diagnostics_text(source)
        .iter()
        .filter_map(|d| d.split_once(':').and_then(|(c, _)| c[2..].parse().ok()))
        .collect()
}

fn this_typing_errors(source: &str) -> Vec<String> {
    diagnostics_text(source)
        .into_iter()
        .filter(|d| d.starts_with("TS2339:") || d.starts_with("TS7023:"))
        .collect()
}

#[test]
fn a_marker_on_the_literal_itself_types_this_in_its_own_methods() {
    // Control: the already-working shape, where the marker sits on the same
    // literal's contextual type.
    let source = r"
type Data = { x: number; };
type Options = { move(dx: number): void; } & ThisType<Data>;
declare function configure(options: Options): void;
configure({
    move(dx) {
        this.x += dx;
    }
});
";
    assert!(
        this_typing_errors(source).is_empty(),
        "unexpected: {:?}",
        this_typing_errors(source)
    );
}

#[test]
fn a_marker_on_the_enclosing_literal_types_this_in_a_nested_literal_method() {
    let source = r"
type Data = { x: number; };
type Methods = { move(dx: number): void; };
type Options = ThisType<Data & Methods> & { methods?: Methods; };
declare function configure(options: Options): void;
configure({
    methods: {
        move(dx) {
            this.x += dx;
        }
    }
});
";
    assert!(
        this_typing_errors(source).is_empty(),
        "unexpected: {:?}",
        this_typing_errors(source)
    );
}

#[test]
fn a_marker_on_the_enclosing_literal_types_this_in_a_nested_accessor() {
    let source = r"
type Data = { x: number; };
type Slot = { readonly doubled?: number; };
type Options = ThisType<Data> & { slot?: Slot; };
declare function configure(options: Options): void;
configure({
    slot: {
        get doubled() {
            return this.x * 2;
        }
    }
});
";
    assert!(
        this_typing_errors(source).is_empty(),
        "unexpected: {:?}",
        this_typing_errors(source)
    );
}

#[test]
fn the_walk_climbs_more_than_one_property_assignment() {
    let source = r"
type Data = { x: number; };
type Inner = { move?(dx: number): void; };
type Outer = { inner?: Inner; };
type Options = ThisType<Data> & { outer?: Outer; };
declare function configure(options: Options): void;
configure({
    outer: {
        inner: {
            move(dx) {
                this.x += dx;
            }
        }
    }
});
";
    assert!(
        this_typing_errors(source).is_empty(),
        "unexpected: {:?}",
        this_typing_errors(source)
    );
}

#[test]
fn a_nearer_marker_wins_over_an_enclosing_one() {
    // The innermost marker is found first, so `this` is `Near`, not `Far`.
    // `far` exists only on `Far`, so reading it must still be an error.
    let source = r"
type Far = { far: number; };
type Near = { near: number; };
type Inner = { read?(): number; } & ThisType<Near>;
type Options = ThisType<Far> & { inner?: Inner; };
declare function configure(options: Options): void;
configure({
    inner: {
        read() {
            return this.near;
        }
    }
});
";
    assert!(
        this_typing_errors(source).is_empty(),
        "expected the nearer marker to apply, got {:?}",
        this_typing_errors(source)
    );

    let shadowed = r"
type Far = { far: number; };
type Near = { near: number; };
type Inner = { read?(): number; } & ThisType<Near>;
type Options = ThisType<Far> & { inner?: Inner; };
declare function configure(options: Options): void;
configure({
    inner: {
        read() {
            return this.far;
        }
    }
});
";
    assert!(
        codes(shadowed).contains(&2339),
        "the enclosing marker must not leak past a nearer one, got {:?}",
        diagnostics_text(shadowed)
    );
}

#[test]
fn renamed_binders_do_not_change_the_outcome() {
    let source = r"
type Store = { counter: number; };
type Handlers = { bump(step: number): void; };
type Config = ThisType<Store & Handlers> & { handlers?: Handlers; };
declare function register(config: Config): void;
register({
    handlers: {
        bump(step) {
            this.counter += step;
        }
    }
});
";
    assert!(
        this_typing_errors(source).is_empty(),
        "unexpected: {:?}",
        this_typing_errors(source)
    );
}

#[test]
fn a_member_missing_from_the_marker_type_still_reports_ts2339() {
    // Negative control: the walk must not make `this` permissive.
    let source = r"
type Data = { x: number; };
type Methods = { move(dx: number): void; };
type Options = ThisType<Data & Methods> & { methods?: Methods; };
declare function configure(options: Options): void;
configure({
    methods: {
        move(dx) {
            this.notAMember += dx;
        }
    }
});
";
    assert!(
        codes(source).contains(&2339),
        "expected TS2339 for `this.notAMember`, got {:?}",
        diagnostics_text(source)
    );
}

#[test]
fn a_nested_literal_without_any_enclosing_marker_keeps_its_own_this() {
    // No marker anywhere: `this` inside the nested literal's method stays the
    // nested literal, so a member of the *outer* literal is not reachable.
    let source = r"
let o = {
    x: 1,
    inner: {
        read() {
            return this.x;
        }
    }
};
";
    assert!(
        codes(source).contains(&2339),
        "expected TS2339 for `this.x` in the nested literal, got {:?}",
        diagnostics_text(source)
    );
}

#[test]
fn the_walk_stops_at_a_non_property_assignment_parent() {
    // `tsc` breaks the climb when the literal's parent is not a
    // `PropertyAssignment`. An array-literal element is such a parent, so the
    // enclosing `ThisType` marker must not reach the method inside it.
    let source = r"
type Data = { x: number; };
type Entry = { read?(): number; };
type Options = ThisType<Data> & { entries?: Entry[]; };
declare function configure(options: Options): void;
configure({
    entries: [
        {
            read() {
                return this.x;
            }
        }
    ]
});
";
    assert!(
        codes(source).contains(&2339),
        "the marker must not climb through an array literal, got {:?}",
        diagnostics_text(source)
    );
}

#[test]
fn a_plain_nested_literal_under_a_marker_free_context_is_unaffected() {
    let source = r"
type Inner = { read(): number };
type Outer = { inner: Inner; value: number };
let o: Outer = {
    value: 1,
    inner: {
        read() {
            return 42;
        }
    }
};
";
    assert!(
        diagnostics_text(source).is_empty(),
        "unexpected: {:?}",
        diagnostics_text(source)
    );
}
