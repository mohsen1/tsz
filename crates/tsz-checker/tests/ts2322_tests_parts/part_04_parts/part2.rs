#[test]
fn ts2820_preserves_generic_alias_application_inside_union_target_renamed_param() {
    // "alphx" is one character off from "alpha" to trigger the spelling suggestion.
    let source = r#"
type AllValues<U> = U[keyof U];
type PickFields<Config> = AllValues<{
  [K in keyof Config]: Config[K] extends object ? keyof Config[K] : never;
}>;
type Schema<Config> = {
  [key: string]: any;
  target?: PickFields<Config>;
};
declare function run<C extends Schema<C>>(opts: C): void;

run({
  target: "alphx",
  group1: { alpha: 1, beta: null },
  group2: { gamma: {}, delta: () => {} },
});
"#;
    let msgs = ts2820_messages(source);
    assert_eq!(
        msgs.len(),
        1,
        "expected exactly one TS2820 diagnostic with renamed params, got: {msgs:#?}"
    );
    let msg = &msgs[0];
    assert!(
        msg.contains("PickFields<"),
        "ts2820 target should preserve PickFields<...> alias form regardless of type param name, got: {msg}"
    );
}

#[test]
fn ts2820_preserves_application_union_with_null_instead_of_undefined() {
    let source = r#"
interface Container<T> { value: T; tag: 1 }
declare let src: Container<"frist">;
declare let dst: Container<"first" | "second"> | null;
dst = src;
"#;
    let all = get_all_diagnostics(source);
    let msg = all
        .iter()
        .find_map(|(code, msg)| (*code == 2322 || *code == 2820).then_some(msg))
        .unwrap_or_else(|| panic!("expected a 2322/2820 diagnostic, got: {all:#?}"));
    assert!(
        msg.contains("Container<"),
        "ts2820/ts2322 should preserve Container<...> alias when target is application | null, got: {msg}"
    );
}

#[test]
fn ts2820_union_of_plain_string_literals_uses_literal_union_form() {
    let source = r#"
declare let c: "bleu";
let x: "red" | "green" | "blue" = c;
"#;
    let all = get_all_diagnostics(source);
    let msg = all
        .iter()
        .find_map(|(code, msg)| (*code == 2322 || *code == 2820).then_some(msg))
        .unwrap_or_else(|| panic!("expected a type mismatch diagnostic, got none"));
    assert!(
        msg.contains("\"red\" | \"green\" | \"blue\""),
        "plain string literal union target should use full literal union form, got: {msg}"
    );
}

/// Collect a TS2322 diagnostic's elaboration as `(depth, message)` pairs,
/// outer-to-inner, for asserting the exact `tsc` chain shape.
fn ts2322_chain(source: &str) -> (String, Vec<(u8, String)>) {
    let diags = diagnostics_for_source(source);
    let ts2322 = diags
        .iter()
        .find(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .unwrap_or_else(|| panic!("expected TS2322, got: {diags:?}"));
    let chain = ts2322
        .related_information
        .iter()
        .map(|r| (r.depth, r.message_text.clone()))
        .collect();
    (ts2322.message_text.clone(), chain)
}

#[test]
fn test_ts2322_array_element_mismatch_emits_tsc_elaboration_chain() {
    // `tsc` relates an array element failure exactly like a single-element
    // tuple: the array-to-array head line, then the element types directly
    // beneath it.
    //   Type 'number[]' is not assignable to type 'string[]'.
    //     Type 'number' is not assignable to type 'string'.
    let (head, chain) = ts2322_chain("declare const s: number[]; const t: string[] = s;");
    assert_eq!(head, "Type 'number[]' is not assignable to type 'string[]'.");
    assert_eq!(
        chain,
        vec![(0u8, "Type 'number' is not assignable to type 'string'.".to_string())],
        "array element mismatch must elaborate the element relation directly"
    );
    assert!(
        !chain.iter().any(|(_, m)| m.starts_with("Array element type")),
        "must not emit the non-tsc 'Array element type ...' wrapper"
    );
}

#[test]
fn test_ts2322_nested_array_element_mismatch_peels_one_level_per_line() {
    //   Type 'number[][]' is not assignable to type 'string[][]'.
    //     Type 'number[]' is not assignable to type 'string[]'.
    //       Type 'number' is not assignable to type 'string'.
    let (head, chain) = ts2322_chain("declare const s: number[][]; const t: string[][] = s;");
    assert_eq!(head, "Type 'number[][]' is not assignable to type 'string[][]'.");
    assert_eq!(
        chain,
        vec![
            (0u8, "Type 'number[]' is not assignable to type 'string[]'.".to_string()),
            (1u8, "Type 'number' is not assignable to type 'string'.".to_string()),
        ]
    );
}

#[test]
fn test_ts2322_array_of_object_element_drills_into_property() {
    //   Type '{ b: number; }[]' is not assignable to type '{ b: string; }[]'.
    //     Type '{ b: number; }' is not assignable to type '{ b: string; }'.
    //       Types of property 'b' are incompatible.
    //         Type 'number' is not assignable to type 'string'.
    let (head, chain) =
        ts2322_chain("declare const s: { b: number }[]; const t: { b: string }[] = s;");
    assert_eq!(
        head,
        "Type '{ b: number; }[]' is not assignable to type '{ b: string; }[]'."
    );
    assert_eq!(chain.len(), 3, "got: {chain:?}");
    assert_eq!(chain[0].0, 0);
    assert_eq!(
        chain[0].1,
        "Type '{ b: number; }' is not assignable to type '{ b: string; }'."
    );
    assert_eq!(chain[1].0, 1);
    assert!(chain[1].1.contains("Types of property 'b' are incompatible."));
    assert_eq!(chain[2].0, 2);
    assert_eq!(
        chain[2].1,
        "Type 'number' is not assignable to type 'string'."
    );
}

#[test]
fn test_ts2322_array_in_property_self_heads_with_array_relation() {
    //   Type '{ a: number[]; }' is not assignable to type '{ a: string[]; }'.
    //     Types of property 'a' are incompatible.
    //       Type 'number[]' is not assignable to type 'string[]'.
    //         Type 'number' is not assignable to type 'string'.
    let (_head, chain) =
        ts2322_chain("declare const s: { a: number[] }; const t: { a: string[] } = s;");
    assert_eq!(chain.len(), 3, "got: {chain:?}");
    assert!(chain[0].1.contains("Types of property 'a' are incompatible."));
    assert_eq!(chain[0].0, 0);
    assert_eq!(
        chain[1],
        (1u8, "Type 'number[]' is not assignable to type 'string[]'.".to_string())
    );
    assert_eq!(
        chain[2],
        (2u8, "Type 'number' is not assignable to type 'string'.".to_string())
    );
}

#[test]
fn test_ts2322_array_element_chain_independent_of_element_names() {
    // Anti-hardcoding: the same elaboration shape must hold for renamed,
    // same-shaped element interfaces — no name string drives the chain.
    let source = "interface Alpha { tag: 1 } interface Beta { tag: 2 } \
                  declare const s: Alpha[]; const t: Beta[] = s;";
    let (head, chain) = ts2322_chain(source);
    assert_eq!(head, "Type 'Alpha[]' is not assignable to type 'Beta[]'.");
    assert_eq!(chain[0].0, 0);
    assert_eq!(
        chain[0].1,
        "Type 'Alpha' is not assignable to type 'Beta'.",
        "the array head must relate the element interfaces, not collapse them"
    );
    assert!(
        chain.iter().any(|(_, m)| m.contains("Types of property 'tag' are incompatible.")),
        "got: {chain:?}"
    );
}
