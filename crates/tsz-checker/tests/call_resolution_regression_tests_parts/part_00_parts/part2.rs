#[test]
fn property_call_on_union_of_interfaces() {
    // Method call on union where both members have the method.
    let source = r#"
interface A { run(x: number): void; }
interface B { run(x: number): void; }
declare let ab: A | B;
ab.run(42);
"#;
    assert!(
        no_errors(source),
        "Method call on union with common method should succeed"
    );
}

#[test]
fn property_call_on_union_missing_method() {
    // One union member lacks the method.
    let source = r#"
interface A { run(x: number): void; }
interface B { stop(): void; }
declare let ab: A | B;
ab.run(42);
"#;
    assert!(
        has_error(source, 2339),
        "Method call on union where one member lacks the method should emit TS2339"
    );
}

#[test]
fn overload_generic_inference_with_callbacks() {
    // Generic overload where callback param type is inferred.
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
let result: number[] = map(["a", "b"], x => x.length);
"#;
    assert!(
        no_errors(source),
        "Generic call with callback should infer T from array and U from callback return"
    );
}

#[test]
fn call_with_spread_from_tuple() {
    // Spread argument from a tuple type matches parameter positions.
    let source = r#"
declare function add(a: number, b: number): number;
let args: [number, number] = [1, 2];
add(...args);
"#;
    assert!(
        no_errors(source),
        "Spread from tuple matching exact param count should succeed"
    );
}

#[test]
fn call_with_spread_wrong_tuple_length() {
    // Spread from tuple with wrong length.
    let source = r#"
declare function add(a: number, b: number): number;
let args: [number, number, number] = [1, 2, 3];
add(...args);
"#;
    let codes = get_codes(source);
    assert!(
        codes.contains(&2554) || codes.contains(&2556),
        "Spread from wrong-length tuple should emit argument count error, got: {codes:?}",
    );
}

#[test]
fn overload_resolution_with_optional_and_required() {
    // Overload where one signature has optional param, another requires it.
    let source = r#"
declare function test(x: string): number;
declare function test(x: string, y?: number): string;
let a: number = test("hello");
"#;
    assert!(no_errors(source), "Should match first overload with 1 arg");
}
