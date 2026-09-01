use crate::context::{CheckerContext, CheckerOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::ParserState;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::computation::CompatChecker;
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::resolver::TypeResolver;
use tsz_solver::def::{DefId, DefinitionStore};
use tsz_solver::{
    FunctionShape, ParamInfo, PropertyInfo, RelationCacheKey, SymbolRef, TypeId, TypeParamInfo,
    Visibility,
};

/// Read a checker source path. If the path is a directory, concatenate all .rs files.
/// If the path ends with .rs and doesn't exist, try the path without .rs as a directory.
fn read_checker_source_file(path: &str) -> String {
    let p = Path::new(path);
    if p.is_file() {
        return fs::read_to_string(p).unwrap_or_default();
    }
    if p.is_dir() {
        let mut combined = String::new();
        if let Ok(entries) = fs::read_dir(p) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|e| e.to_str()) == Some("rs")
                    && let Ok(c) = fs::read_to_string(entry.path())
                {
                    combined.push_str(&c);
                }
            }
        }
        return combined;
    }
    // Try stripping .rs extension and treating as directory
    if let Some(dir_path) = path.strip_suffix(".rs")
        && Path::new(dir_path).is_dir()
    {
        return read_checker_source_file(dir_path);
    }
    String::new()
}

fn collect_checker_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_checker_rs_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn make_animal_and_dog(interner: &TypeInterner) -> (TypeId, TypeId) {
    let animal_name = interner.intern_string("name");
    let dog_breed = interner.intern_string("breed");

    let animal = interner.object(vec![tsz_solver::PropertyInfo {
        name: animal_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
        non_widening: false,
    }]);

    let dog = interner.object(vec![
        tsz_solver::PropertyInfo {
            name: animal_name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        },
        tsz_solver::PropertyInfo {
            name: dog_breed,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
            is_symbol_named: false,
            single_quoted_name: false,
            non_widening: false,
        },
    ]);

    (animal, dog)
}

fn collect_checker_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("failed to read checker source directory {}", dir.display()));
    for entry in entries {
        let entry = entry.expect("failed to read checker source directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_checker_rs_files_recursive(&path, files);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

/// Helper: recursively walk a directory collecting production `.rs` files.
fn walk_rs_files_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "tests" {
                continue;
            }
            walk_rs_files_recursive(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if path.file_name().is_some_and(|name| name == "tests.rs") {
                continue;
            }
            files.push(path);
        }
    }
}

#[path = "architecture_contract_tests/part_00.rs"]
mod part_00;
#[path = "architecture_contract_tests/part_01.rs"]
mod part_01;
#[path = "architecture_contract_tests/part_02.rs"]
mod part_02;
#[path = "architecture_contract_tests/part_03.rs"]
mod part_03;
#[path = "architecture_contract_tests/part_04.rs"]
mod part_04;
#[path = "architecture_contract_tests/part_05.rs"]
mod part_05;
#[path = "architecture_contract_tests/part_06.rs"]
mod part_06;
