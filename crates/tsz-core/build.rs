use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
struct LibraryIndex {
    references: Vec<String>,
    type_names: BTreeSet<String>,
    value_names: BTreeSet<String>,
    string_record_type_names: BTreeSet<String>,
    function_zero_argument_string_method_names: BTreeSet<String>,
    array_search_method_names: BTreeSet<String>,
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let library_dir = manifest_dir.join("data/lib");
    println!("cargo:rerun-if-changed={}", library_dir.display());

    let mut paths = fs::read_dir(&library_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".d.ts"))
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut libraries = BTreeMap::new();
    for path in paths {
        let name = library_name(&path);
        let source = fs::read_to_string(&path).unwrap();
        let references = reference_libraries(&source);
        libraries.insert(name, declaration_names(&source, references));
    }
    for (name, library) in &libraries {
        for reference in &library.references {
            assert!(
                libraries.contains_key(reference),
                "pinned library {name} references missing library {reference}"
            );
        }
    }

    let output = generated_source(&libraries);
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out_dir.join("standard_library_data.rs"), output).unwrap();
}

fn library_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".d.ts"))
        .unwrap()
        .to_ascii_lowercase()
}

fn reference_libraries(source: &str) -> Vec<String> {
    const PREFIX: &str = "/// <reference lib=\"";
    let mut references = source
        .lines()
        .filter_map(|line| {
            let start = line.find(PREFIX)? + PREFIX.len();
            let tail = line.get(start..)?;
            let end = tail.find('"')?;
            Some(tail[..end].to_ascii_lowercase())
        })
        .collect::<Vec<_>>();
    references.dedup();
    references
}

fn declaration_names(source: &str, references: Vec<String>) -> LibraryIndex {
    let tokens = tokens(source);
    let mut scopes = Vec::new();
    let mut type_names = BTreeSet::new();
    let mut value_names = BTreeSet::new();
    let mut string_record_type_names = BTreeSet::new();
    let mut function_zero_argument_string_method_names = BTreeSet::new();
    let mut array_search_method_names = BTreeSet::new();

    for (index, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "{" => {
                let global = is_declaration_scope(&scopes)
                    && tokens
                        .get(index.wrapping_sub(1))
                        .is_some_and(|token| token == "global")
                    && tokens
                        .get(index.wrapping_sub(2))
                        .is_some_and(|token| token == "declare");
                scopes.push(global);
            }
            "}" => {
                scopes.pop();
            }
            _ if is_declaration_scope(&scopes) => {
                let Some(name) = tokens.get(index + 1).filter(|name| is_identifier(name)) else {
                    continue;
                };
                match token.as_str() {
                    "interface" | "type" => {
                        type_names.insert(name.clone());
                        if token == "type" && is_homogeneous_string_record_alias(&tokens, index) {
                            string_record_type_names.insert(name.clone());
                        }
                        if token == "interface" && name == "Function" {
                            function_zero_argument_string_method_names
                                .extend(zero_argument_string_methods(&tokens, index));
                        }
                        if token == "interface" && name == "Array" {
                            array_search_method_names.extend(array_search_methods(&tokens, index));
                        }
                    }
                    "class" | "enum" | "namespace" | "module" => {
                        type_names.insert(name.clone());
                        value_names.insert(name.clone());
                    }
                    "function" => {
                        value_names.insert(name.clone());
                    }
                    "var" | "let" | "const" if name != "enum" => {
                        value_names.insert(name.clone());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    LibraryIndex {
        references,
        type_names,
        value_names,
        string_record_type_names,
        function_zero_argument_string_method_names,
        array_search_method_names,
    }
}

/// Extract the pinned intrinsic interface member shape `name(): string`.
///
/// The interface name is selected by the caller while declaration tokens are
/// still known to come from the pinned library corpus. Member spellings are
/// data: no checker path elects a property by a source or fixture string.
fn zero_argument_string_methods(tokens: &[String], interface_start: usize) -> BTreeSet<String> {
    let Some(open) = tokens
        .iter()
        .enumerate()
        .skip(interface_start + 2)
        .find_map(|(index, token)| (token == "{").then_some(index))
    else {
        return BTreeSet::new();
    };

    let mut methods = BTreeSet::new();
    let mut depth = 1_u32;
    let mut index = open + 1;
    while index < tokens.len() && depth > 0 {
        match tokens[index].as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            _ if depth == 1 => {
                if let Some(shape) = tokens.get(index..index + 6)
                    && is_identifier(&shape[0])
                    && shape[1] == "("
                    && shape[2] == ")"
                    && shape[3] == ":"
                    && shape[4] == "string"
                    && shape[5] == ";"
                {
                    methods.insert(shape[0].clone());
                    index += shape.len();
                    continue;
                }
            }
            _ => {}
        }
        index += 1;
    }
    methods
}

/// Extract direct members from canonical pinned `interface Array<E>`
/// declarations whose complete callable shape is
/// `(required E, optional number) => number`.
fn array_search_methods(tokens: &[String], interface_start: usize) -> BTreeSet<String> {
    let Some(header) = tokens.get(interface_start..interface_start + 6) else {
        return BTreeSet::new();
    };
    if header[0] != "interface"
        || !is_identifier(&header[1])
        || header[2] != "<"
        || !is_identifier(&header[3])
        || header[4] != ">"
        || header[5] != "{"
    {
        return BTreeSet::new();
    }
    let element_parameter = &header[3];
    let mut methods = BTreeSet::new();
    let mut depth = 1_u32;
    let mut index = interface_start + header.len();
    while index < tokens.len() && depth > 0 {
        match tokens[index].as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            _ if depth == 1 => {
                if let Some(shape) = tokens.get(index..index + 14)
                    && is_identifier(&shape[0])
                    && shape[1] == "("
                    && is_identifier(&shape[2])
                    && shape[3] == ":"
                    && shape[4] == *element_parameter
                    && shape[5] == ","
                    && is_identifier(&shape[6])
                    && shape[7] == "?"
                    && shape[8] == ":"
                    && shape[9] == "number"
                    && shape[10] == ")"
                    && shape[11] == ":"
                    && shape[12] == "number"
                    && shape[13] == ";"
                {
                    methods.insert(shape[0].clone());
                    index += shape.len();
                    continue;
                }
            }
            _ => {}
        }
        index += 1;
    }
    methods
}

/// Recognize the pinned-library declaration shape
/// `<K extends keyof any, V> = { [P in K]: V }` without using its binder names.
fn is_homogeneous_string_record_alias(tokens: &[String], start: usize) -> bool {
    let Some(shape) = tokens.get(start..start + 21) else {
        return false;
    };
    let key_parameter = &shape[3];
    let value_parameter = &shape[8];
    let mapped_parameter = &shape[13];
    shape[0] == "type"
        && is_identifier(&shape[1])
        && shape[2] == "<"
        && is_identifier(key_parameter)
        && shape[4] == "extends"
        && shape[5] == "keyof"
        && shape[6] == "any"
        && shape[7] == ","
        && is_identifier(value_parameter)
        && shape[9] == ">"
        && shape[10] == "="
        && shape[11] == "{"
        && shape[12] == "["
        && is_identifier(mapped_parameter)
        && shape[14] == "in"
        && shape[15] == *key_parameter
        && shape[16] == "]"
        && shape[17] == ":"
        && shape[18] == *value_parameter
        && shape[19] == ";"
        && shape[20] == "}"
}

fn is_declaration_scope(scopes: &[bool]) -> bool {
    scopes.is_empty() || scopes.iter().all(|scope| *scope)
}

fn tokens(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            quote @ (b'\'' | b'"' | b'`') => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index = (index + 2).min(bytes.len());
                    } else if bytes[index] == quote {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            b'{' | b'}' | b'[' | b']' | b'(' | b')' | b'<' | b'>' | b',' | b':' | b';' | b'='
            | b'?' => {
                tokens.push(char::from(bytes[index]).to_string());
                index += 1;
            }
            byte if is_identifier_start(byte) => {
                let start = index;
                index += 1;
                while bytes
                    .get(index)
                    .is_some_and(|byte| is_identifier_part(*byte))
                {
                    index += 1;
                }
                tokens.push(source[start..index].to_string());
            }
            _ => index += 1,
        }
    }
    tokens
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

const fn is_identifier_part(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn is_identifier(token: &str) -> bool {
    token
        .as_bytes()
        .first()
        .is_some_and(|byte| is_identifier_start(*byte))
}

fn generated_source(libraries: &BTreeMap<String, LibraryIndex>) -> String {
    let mut output = String::from(
        "// Generated by crates/tsz-core/build.rs from the pinned TS7 library assets.\n",
    );
    output.push_str("static LIBRARIES: &[GeneratedLibrary] = &[\n");
    for (name, library) in libraries {
        output.push_str("    GeneratedLibrary { name: ");
        output.push_str(&format!("{name:?}"));
        output.push_str(", references: &");
        output.push_str(&render_strings(&library.references));
        output.push_str(", type_names: &");
        output.push_str(&render_strings(&library.type_names));
        output.push_str(", value_names: &");
        output.push_str(&render_strings(&library.value_names));
        output.push_str(", string_record_type_names: &");
        output.push_str(&render_strings(&library.string_record_type_names));
        output.push_str(", function_zero_argument_string_method_names: &");
        output.push_str(&render_strings(
            &library.function_zero_argument_string_method_names,
        ));
        output.push_str(", array_search_method_names: &");
        output.push_str(&render_strings(&library.array_search_method_names));
        output.push_str(" },\n");
    }
    output.push_str("];\n");
    let array_search_method_names = libraries
        .values()
        .flat_map(|library| library.array_search_method_names.iter().cloned())
        .collect::<BTreeSet<_>>();
    output.push_str("static ARRAY_SEARCH_METHOD_NAMES: &[&str] = &");
    output.push_str(&render_strings(&array_search_method_names));
    output.push_str(";\n");
    output
}

fn render_strings<'a>(values: impl IntoIterator<Item = &'a String>) -> String {
    let mut output = String::from("[");
    for (index, value) in values.into_iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str(&format!("{value:?}"));
    }
    output.push(']');
    output
}
