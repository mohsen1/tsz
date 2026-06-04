#[test]
fn compile_project_umd_global_class_surface_stays_unaugmented() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("node_modules/math2d/index.d.ts"),
        r#"export as namespace Math2d;

export interface Point {
    x: number;
    y: number;
}

export class Vector implements Point {
    x: number;
    y: number;
    constructor(x: number, y: number);

    translate(dx: number, dy: number): Vector;
}

export function getLength(p: Vector): number;
"#,
    );
    write_file(
        &base.join("math2d-augment.d.ts"),
        r#"import * as Math2d from "math2d";

declare module "math2d" {
    interface Vector {
        reverse(): Math2d.Point;
    }
}
"#,
    );
    write_file(
        &base.join("a.ts"),
        r#"/// <reference path="node_modules/math2d/index.d.ts" />
/// <reference path="math2d-augment.d.ts" />

let v = new Math2d.Vector(3, 2);
v.reverse();
"#,
    );
    write_file(
        &base.join("b.ts"),
        r#"/// <reference path="math2d-augment.d.ts" />
import * as m from "math2d";

let v = new m.Vector(3, 2);
v.reverse();
"#,
    );
    write_file(
        &base.join("tsconfig.global.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "noEmit": true
  },
  "files": ["a.ts"]
}"#,
    );
    write_file(
        &base.join("tsconfig.import.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "noEmit": true
  },
  "files": ["b.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.global.json"));
    let global_result = compile(&args, base).expect("global compile should succeed");
    assert!(
        global_result.diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                && d.message_text
                    .contains("Property 'reverse' does not exist on type 'Vector'.")
        }),
        "Expected bare UMD global access to keep the class declaration surface and report TS2339 on Vector. Actual diagnostics: {:#?}",
        global_result.diagnostics
    );

    args.project = Some(base.join("tsconfig.import.json"));
    let import_result = compile(&args, base).expect("import compile should succeed");
    assert!(
        import_result
            .diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected real module imports to keep the class augmentation visible. Actual diagnostics: {:#?}",
        import_result.diagnostics
    );
}

#[derive(Debug, PartialEq, Eq)]
struct SymbolSnapshot {
    flags: u32,
    declarations_len: usize,
    value_declaration: u32,
    value_declaration_span: Option<(u32, u32)>,
    first_declaration_span: Option<(u32, u32)>,
    parent_name: Option<String>,
    exports: Vec<(String, String)>,
    members: Vec<(String, String)>,
    is_exported: bool,
    is_type_only: bool,
    import_module: Option<String>,
    import_name: Option<String>,
    is_umd_export: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct SemanticDefSnapshot {
    kind: tsz_binder::state::SemanticDefKind,
    name: String,
    file_id: u32,
    span_start: u32,
    type_param_count: u16,
    type_param_names: Vec<String>,
    is_exported: bool,
    enum_member_names: Vec<String>,
    is_const: bool,
    is_abstract: bool,
    extends_names: Vec<String>,
    implements_names: Vec<String>,
    parent_namespace_name: Option<String>,
    is_global_augmentation: bool,
    is_declare: bool,
}

fn symbol_name_for_id(binder: &BinderState, sym_id: SymbolId) -> Option<String> {
    binder
        .symbols
        .get(sym_id)
        .map(|sym| sym.escaped_name.clone())
}

fn semantic_def_snapshot(
    binder: &BinderState,
    sym_id: SymbolId,
    entry: &tsz_binder::state::SemanticDefEntry,
) -> SemanticDefSnapshot {
    SemanticDefSnapshot {
        kind: entry.kind,
        name: entry.name.clone(),
        file_id: entry.file_id,
        span_start: entry.span_start,
        type_param_count: entry.type_param_count,
        type_param_names: entry.type_param_names.clone(),
        is_exported: entry.is_exported,
        enum_member_names: entry.enum_member_names.clone(),
        is_const: entry.is_const,
        is_abstract: entry.is_abstract,
        extends_names: entry.extends_names.clone(),
        implements_names: entry.implements_names.clone(),
        parent_namespace_name: entry.parent_namespace.and_then(|parent| {
            if parent == sym_id {
                Some("<self>".to_string())
            } else {
                symbol_name_for_id(binder, parent).or_else(|| Some(format!("#{}", parent.0)))
            }
        }),
        is_global_augmentation: entry.is_global_augmentation,
        is_declare: entry.is_declare,
    }
}

fn symbol_snapshot_by_id(binder: &BinderState, sym_id: SymbolId) -> Option<SymbolSnapshot> {
    let sym = binder.symbols.get(sym_id)?;
    let mut exports = sym
        .exports
        .as_ref()
        .map(|table| {
            table
                .iter()
                .map(|(export_name, export_sym_id)| {
                    (
                        export_name.clone(),
                        symbol_name_for_id(binder, *export_sym_id)
                            .unwrap_or_else(|| format!("#{}", export_sym_id.0)),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    exports.sort();

    let mut members = sym
        .members
        .as_ref()
        .map(|table| {
            table
                .iter()
                .map(|(member_name, member_sym_id)| {
                    (
                        member_name.clone(),
                        symbol_name_for_id(binder, *member_sym_id)
                            .unwrap_or_else(|| format!("#{}", member_sym_id.0)),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    members.sort();

    Some(SymbolSnapshot {
        flags: sym.flags,
        declarations_len: sym.declarations.len(),
        value_declaration: sym.value_declaration.0,
        value_declaration_span: sym.value_declaration_span,
        first_declaration_span: sym.first_declaration_span,
        parent_name: (sym.parent.is_some()).then(|| {
            symbol_name_for_id(binder, sym.parent).unwrap_or_else(|| format!("#{}", sym.parent.0))
        }),
        exports,
        members,
        is_exported: sym.is_exported,
        is_type_only: sym.is_type_only,
        import_module: sym.import_module.clone(),
        import_name: sym.import_name.clone(),
        is_umd_export: sym.is_umd_export,
    })
}

fn symbol_snapshot(binder: &BinderState, name: &str) -> Option<SymbolSnapshot> {
    let sym_id = binder.file_locals.get(name)?;
    symbol_snapshot_by_id(binder, sym_id)
}

fn declaration_arena_file_names_for_symbol(
    binder: &BinderState,
    sym_id: SymbolId,
) -> Vec<(u32, Vec<String>)> {
    let Some(sym) = binder.symbols.get(sym_id) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for &decl_idx in &sym.declarations {
        let mut arena_files = binder
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .map(|arenas| {
                arenas
                    .iter()
                    .filter_map(|arena| arena.source_files.first().map(|sf| sf.file_name.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        arena_files.sort();
        result.push((decl_idx.0, arena_files));
    }
    result.sort_by_key(|(decl, _)| *decl);
    result
}

#[test]
fn compile_with_tsconfig_emits_outputs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(
        &base.join("tsconfig.json"),
        r#"{
          "compilerOptions": {
            "outDir": "dist",
            "rootDir": ".",
            "declaration": true
          },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write_file(&base.join("src/index.ts"), "export const value = 1;");

    let args = default_args();
    let result = with_types_versions_env(Some("5.9"), || {
        compile(&args, base).expect("compile should succeed")
    });

    assert!(result.diagnostics.is_empty());
    assert!(base.join("dist/src/index.js").is_file());
    assert!(base.join("dist/src/index.d.ts").is_file());
}
