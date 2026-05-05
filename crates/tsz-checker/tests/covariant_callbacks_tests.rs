use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn method_callback_parameters_match_tsc_covariant_callback_rules() {
    let source = r#"
interface P<T> {
    then(cb: (value: T) => void): void;
}

interface A { a: string }
interface B extends A { b: string }

function f1(a: P<A>, b: P<B>) {
    a = b;
    b = a;
}

interface AList1 {
    forEach(cb: (item: A) => void): void;
}
interface BList1 {
    forEach(cb: (item: B) => void): void;
}
function f11(a: AList1, b: BList1) {
    a = b;
    b = a;
}

interface AList2 {
    forEach(cb: (item: A) => boolean): void;
}
interface BList2 {
    forEach(cb: (item: A) => void): void;
}
function f12(a: AList2, b: BList2) {
    a = b;
    b = a;
}

interface AList3 {
    forEach(cb: (item: A) => void): void;
}
interface BList3 {
    forEach(cb: (item: A, context: any) => void): void;
}
function f13(a: AList3, b: BList3) {
    a = b;
    b = a;
}

interface AList4 {
    forEach(cb: (item: A) => A): void;
}
interface BList4 {
    forEach(cb: (item: B) => B): void;
}
function f14(a: AList4, b: BList4) {
    a = b;
    b = a;
}

type Bivar1<T> = { set(value: T): void }
type Bivar2<T> = { set(value: T): void }

declare let b1fu: Bivar1<(x: unknown) => void>;
declare let b2fs: Bivar2<(x: string) => void>;
b1fu = b2fs;
b2fs = b1fu;

type SetLike1<T> = { set(value: T): void, get(): T }
type SetLike2<T> = { set(value: T): void, get(): T }

declare let sx: SetLike1<(x: unknown) => void>;
declare let sy: SetLike1<(x: string) => void>;
sx = sy;
sy = sx;

declare let s1: SetLike1<(x: unknown) => void>;
declare let s2: SetLike2<(x: string) => void>;
s1 = s2;
s2 = s1;
"#;

    let diagnostics = check_source_code_messages(source);
    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect::<Vec<_>>();

    assert_eq!(
        ts2322.len(),
        7,
        "expected only the tsc covariant-callback assignment errors; got {diagnostics:#?}"
    );
}
