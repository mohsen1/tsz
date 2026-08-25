use super::{compiler_option_spans, parse_jsonc, reference_object_spans};

#[test]
fn one_jsonc_scan_preserves_original_option_and_reference_byte_spans() {
    let source = concat!(
        "\u{feff}{\n",
        "  // comments remain part of the original byte coordinate space\n",
        "  \"compilerOptions\" /* owner */ : {\n",
        "    \"target\" /* key gap */ : \"wat\", /* trailing */\n",
        "    \"module\": \"co//mmonjs\",\n",
        "  },\n",
        "  \"references\" : [ /* open */\n",
        "    { \"path\": \"./dependency\" }, /* trailing */\n",
        "  ],\n",
        "}\n",
    );
    let document = parse_jsonc(source).expect("valid JSONC");
    assert_eq!(document.value["compilerOptions"]["target"], "wat");
    assert_eq!(document.value["compilerOptions"]["module"], "co//mmonjs");

    let option = compiler_option_spans(&document.tokens)["target"];
    assert_eq!(option.key_start, source.find("\"target\"").unwrap() as u32);
    assert_eq!(option.key_length, "\"target\"".len() as u32);
    assert_eq!(
        option.value_start,
        Some(source.find("\"wat\"").unwrap() as u32)
    );
    assert_eq!(option.value_length, Some("\"wat\"".len() as u32));

    let object_start = source.find("{ \"path\"").unwrap();
    let object_end = source[object_start..].find('}').unwrap() + object_start + 1;
    assert_eq!(
        reference_object_spans(&document.tokens),
        [Some((
            object_start as u32,
            (object_end - object_start) as u32
        ))]
    );
}
