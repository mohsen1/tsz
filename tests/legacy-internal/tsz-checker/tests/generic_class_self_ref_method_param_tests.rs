use crate::test_utils::check_source_diagnostics;

#[test]
fn generic_class_self_param_vec2_a() {
    let diags = check_source_diagnostics(
        r#"
class Vec2<A> {
    constructor(public x: A, public y: A) {}
    apply<B>(f: Vec2<(a: A) => B>): Vec2<B> {
        var x: B = f.x(this.x);
        var y: B = f.y(this.y);
        return new Vec2(x, y);
    }
}
"#,
    );
    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349: f.x and f.y must be callable after instantiation; got: {ts2349:?}"
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 from Vec2 apply; got: {ts2322:?}"
    );
}

#[test]
fn generic_class_self_param_different_names_t_u() {
    let diags = check_source_diagnostics(
        r#"
class Pair<T> {
    constructor(public first: T, public second: T) {}
    map<U>(fn: Pair<(t: T) => U>): Pair<U> {
        return new Pair(fn.first(this.first), fn.second(this.second));
    }
}
"#,
    );
    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 with T/U params; got: {ts2349:?}"
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 with T/U params; got: {ts2322:?}"
    );
}

#[test]
fn generic_class_self_param_triplet_x() {
    let diags = check_source_diagnostics(
        r#"
class Triplet<X> {
    constructor(public a: X, public b: X, public c: X) {}
    transform<Y>(fns: Triplet<(x: X) => Y>): Triplet<Y> {
        return new Triplet(fns.a(this.a), fns.b(this.b), fns.c(this.c));
    }
}
"#,
    );
    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 with Triplet/X; got: {ts2349:?}"
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 with Triplet/X; got: {ts2322:?}"
    );
}

#[test]
fn generic_class_self_return_no_false_error() {
    let diags = check_source_diagnostics(
        r#"
class Box<V> {
    constructor(public value: V) {}
    map<W>(f: (v: V) => W): Box<W> {
        return new Box(f(this.value));
    }
    flatMap<W>(f: (v: V) => Box<W>): Box<W> {
        return f(this.value);
    }
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| [2322, 2349, 2339].contains(&d.code))
        .collect();
    assert!(
        errors.is_empty(),
        "Expected no type errors in Box<V>; got: {errors:?}"
    );
}

#[test]
fn generic_class_non_callable_property_still_errors() {
    let diags = check_source_diagnostics(
        r#"
class Holder<T> {
    constructor(public val: T) {}
    bad(other: Holder<number>): number {
        return other.val();
    }
}
"#,
    );
    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        !ts2349.is_empty(),
        "Expected TS2349: Holder<number>.val is number, not callable"
    );
}
