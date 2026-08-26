//! Pinned TypeScript 7 standard-library selection and global symbol ownership.
//!
//! The user grammar is not asked to parse the multi-megabyte declaration
//! corpus before it supports that grammar. A build-time index contributes
//! declaration identity and narrow, structurally recognized member identities;
//! broader declaration shapes remain an explicit checker-owned porting surface.

use std::collections::{BTreeMap, BTreeSet};

use crate::bind::Meaning;
use crate::program::CompilerOptions;
use crate::source::{DeclId, FileId};

const STANDARD_LIBRARY_FILE: FileId = FileId(u32::MAX);

// Pinned TS7 source: `TypeScript/src/compiler/checker.ts`,
// `initializeTypeChecker` (the `getGlobalType(..., reportErrors: true)` calls
// around lines 51619-51632). Keep this in diagnostic sort order; the current
// rewrite option surface has TS7's default strict bind/call/apply behavior.
const ESSENTIAL_GLOBAL_TYPES: &[&str] = &[
    "Array",
    "Boolean",
    "CallableFunction",
    "Function",
    "IArguments",
    "NewableFunction",
    "Number",
    "Object",
    "RegExp",
    "String",
];

#[derive(Debug)]
pub(crate) struct CanonicalTypeAliasOrigin {
    pub(crate) name: &'static str,
    pub(crate) path: &'static str,
    pub(crate) source: &'static str,
    pub(crate) name_start: u32,
}

#[derive(Debug)]
struct GeneratedLibrary {
    name: &'static str,
    references: &'static [&'static str],
    type_names: &'static [&'static str],
    value_names: &'static [&'static str],
    homogeneous_record_type_origins: &'static [CanonicalTypeAliasOrigin],
}

include!(concat!(env!("OUT_DIR"), "/standard_library_data.rs"));

/// A declaration discovered from one or more selected pinned library files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardLibraryDeclaration {
    pub id: DeclId,
    pub name: String,
    pub meaning: Meaning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryReceiver {
    Array,
    Declaration(DeclId),
}

/// Canonical callable-member identity owned by the selected pinned libraries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryCallMember {
    IndexOf,
    LastIndexOf,
    Map,
    Push,
    Slice,
    Splice,
    MapGet,
    MapSet,
    ToString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryMemberLookup {
    Missing,
    DeferredUntilMemberMerging,
    Found(LibraryCallMember),
}

/// The deterministic, program-owned ambient declaration environment.
#[derive(Debug, Default)]
pub struct StandardLibraryEnvironment {
    selected_libraries: Vec<&'static str>,
    declarations: Vec<StandardLibraryDeclaration>,
    type_names: BTreeMap<String, DeclId>,
    value_names: BTreeMap<String, DeclId>,
    homogeneous_record_types: BTreeMap<DeclId, &'static CanonicalTypeAliasOrigin>,
}

impl StandardLibraryEnvironment {
    #[must_use]
    pub fn from_options(options: &CompilerOptions) -> Self {
        if options.no_lib {
            return Self::default();
        }

        let roots = options.lib.as_ref().map_or_else(
            || vec![default_library_for_target(&options.target)],
            |libraries| {
                libraries
                    .iter()
                    .filter_map(|name| explicit_library_name(name))
                    .collect()
            },
        );
        Self::from_roots(&roots)
    }

    #[must_use]
    pub fn selected_libraries(&self) -> &[&'static str] {
        &self.selected_libraries
    }

    #[must_use]
    pub fn declarations(&self) -> &[StandardLibraryDeclaration] {
        &self.declarations
    }

    #[must_use]
    pub fn declaration(&self, id: DeclId) -> Option<&StandardLibraryDeclaration> {
        (id.file == STANDARD_LIBRARY_FILE)
            .then(|| self.declarations.get(id.local as usize))
            .flatten()
    }

    #[must_use]
    pub fn resolve(&self, name: &str, meaning: Meaning) -> Option<DeclId> {
        match meaning {
            Meaning::Value => self.value_names.get(name),
            Meaning::Type => self.type_names.get(name),
        }
        .copied()
    }

    #[must_use]
    pub(crate) fn is_homogeneous_record_type(&self, id: DeclId) -> bool {
        self.homogeneous_record_types.contains_key(&id)
    }

    pub(crate) fn homogeneous_record_origin(
        &self,
        id: DeclId,
    ) -> Option<&'static CanonicalTypeAliasOrigin> {
        self.homogeneous_record_types.get(&id).copied()
    }

    pub(crate) fn hide_homogeneous_record_type(&mut self, id: DeclId) {
        self.type_names.retain(|_, candidate| *candidate != id);
    }

    #[must_use]
    pub(crate) fn is_property_key_type(&self, id: DeclId) -> bool {
        self.resolve("PropertyKey", Meaning::Type) == Some(id)
    }

    #[must_use]
    pub(crate) fn is_array_type(&self, id: DeclId) -> bool {
        self.resolve("Array", Meaning::Type) == Some(id)
    }

    #[must_use]
    pub(crate) fn is_array_value(&self, id: DeclId) -> bool {
        self.resolve("Array", Meaning::Value) == Some(id)
    }

    pub(crate) fn is_instanceof_constructor_value(&self, id: DeclId) -> bool {
        ["Function", "Object"]
            .into_iter()
            .any(|name| self.resolve(name, Meaning::Value) == Some(id))
    }

    pub(crate) fn is_function_type(&self, id: DeclId) -> bool {
        self.resolve("Function", Meaning::Type) == Some(id)
    }

    pub(crate) fn is_map_type(&self, id: DeclId) -> bool {
        self.resolve("Map", Meaning::Type) == Some(id)
    }

    pub(crate) fn map_type_for_value(&self, id: DeclId) -> Option<DeclId> {
        (self.resolve("Map", Meaning::Value) == Some(id))
            .then(|| self.resolve("Map", Meaning::Type))
            .flatten()
    }

    #[must_use]
    pub(crate) fn call_member(
        &self,
        receiver: LibraryReceiver,
        name: &str,
        mut has_authored_declarations: impl FnMut(DeclId) -> bool,
        mut has_authored_member: impl FnMut(DeclId, &str) -> bool,
    ) -> LibraryMemberLookup {
        let member = match receiver {
            LibraryReceiver::Array => {
                let Some(owner) = self.resolve("Array", Meaning::Type) else {
                    return LibraryMemberLookup::Missing;
                };
                if has_authored_declarations(owner) {
                    return LibraryMemberLookup::DeferredUntilMemberMerging;
                }
                if !self.selected_libraries.contains(&"es5") {
                    return LibraryMemberLookup::Missing;
                }
                match name {
                    "indexOf" => LibraryCallMember::IndexOf,
                    "lastIndexOf" => LibraryCallMember::LastIndexOf,
                    "map" => LibraryCallMember::Map,
                    "push" => LibraryCallMember::Push,
                    "slice" => LibraryCallMember::Slice,
                    "splice" => LibraryCallMember::Splice,
                    _ => return LibraryMemberLookup::Missing,
                }
            }
            LibraryReceiver::Declaration(owner) if self.is_map_type(owner) => {
                if has_authored_declarations(owner) || self.resolve("Map", Meaning::Value).is_none()
                {
                    return LibraryMemberLookup::DeferredUntilMemberMerging;
                }
                match name {
                    "get" => LibraryCallMember::MapGet,
                    "set" => LibraryCallMember::MapSet,
                    _ => return LibraryMemberLookup::Missing,
                }
            }
            LibraryReceiver::Declaration(owner) if self.is_array_value(owner) => {
                if name != "toString" || !self.selected_libraries.contains(&"es5") {
                    return LibraryMemberLookup::Missing;
                }
                // `Function.toString` shadows `Object.toString`, so Object is
                // not an apparent-owner dependency for this member.
                for dependency in ["ArrayConstructor", "CallableFunction", "Function"] {
                    let Some(dependency) = self.resolve(dependency, Meaning::Type) else {
                        return LibraryMemberLookup::DeferredUntilMemberMerging;
                    };
                    if has_authored_member(dependency, name) {
                        return LibraryMemberLookup::DeferredUntilMemberMerging;
                    }
                }
                LibraryCallMember::ToString
            }
            LibraryReceiver::Declaration(_) => {
                return LibraryMemberLookup::Missing;
            }
        };
        LibraryMemberLookup::Found(member)
    }

    #[must_use]
    pub(crate) fn is_rest_array_type(&self, id: DeclId) -> bool {
        self.is_array_type(id) || self.resolve("ReadonlyArray", Meaning::Type) == Some(id)
    }

    #[must_use]
    pub(crate) fn is_undefined_value(&self, id: DeclId) -> bool {
        self.resolve("undefined", Meaning::Value) == Some(id)
    }

    #[must_use]
    pub(crate) const fn essential_type_names() -> &'static [&'static str] {
        ESSENTIAL_GLOBAL_TYPES
    }

    fn from_roots(roots: &[&str]) -> Self {
        let mut selected_libraries = Vec::new();
        let mut visited = BTreeSet::new();
        for root in roots {
            visit_library(root, &mut visited, &mut selected_libraries);
        }

        let mut names = BTreeSet::new();
        let mut homogeneous_record_type_origins = BTreeMap::new();
        for name in &selected_libraries {
            let Some(library) = library(name) else {
                continue;
            };
            for (entries, meaning) in [(library.type_names, 0_u8), (library.value_names, 1)] {
                names.extend(entries.iter().map(|name| ((*name).to_string(), meaning)));
            }
            homogeneous_record_type_origins.extend(
                library
                    .homogeneous_record_type_origins
                    .iter()
                    .map(|origin| (origin.name.to_string(), origin)),
            );
        }

        let mut declarations = Vec::with_capacity(names.len());
        let mut type_names = BTreeMap::new();
        let mut value_names = BTreeMap::new();
        let mut homogeneous_record_types = BTreeMap::new();
        for (name, meaning) in names {
            let meaning = if meaning == 0 {
                Meaning::Type
            } else {
                Meaning::Value
            };
            let id = DeclId {
                file: STANDARD_LIBRARY_FILE,
                local: declarations.len() as u32,
            };
            match meaning {
                Meaning::Value => value_names.insert(name.clone(), id),
                Meaning::Type => {
                    if let Some(origin) = homogeneous_record_type_origins.get(&name) {
                        homogeneous_record_types.insert(id, *origin);
                    }
                    type_names.insert(name.clone(), id)
                }
            };
            declarations.push(StandardLibraryDeclaration { id, name, meaning });
        }
        // `undefined` is an intrinsic value in a normal TypeScript program,
        // even though the pinned declaration libraries do not spell it as a
        // top-level `var`. Give it program-owned identity so ordinary lexical
        // shadowing wins before the checker recognizes the intrinsic.
        if !value_names.contains_key("undefined") {
            let name = "undefined".to_string();
            let id = DeclId {
                file: STANDARD_LIBRARY_FILE,
                local: declarations.len() as u32,
            };
            value_names.insert(name.clone(), id);
            declarations.push(StandardLibraryDeclaration {
                id,
                name,
                meaning: Meaning::Value,
            });
        }

        Self {
            selected_libraries,
            declarations,
            type_names,
            value_names,
            homogeneous_record_types,
        }
    }
}

fn visit_library(
    name: &str,
    visited: &mut BTreeSet<&'static str>,
    selected: &mut Vec<&'static str>,
) {
    let Some(library) = library(name) else {
        return;
    };
    if !visited.insert(library.name) {
        return;
    }
    for reference in library.references {
        visit_library(reference, visited, selected);
    }
    selected.push(library.name);
}

fn library(name: &str) -> Option<&'static GeneratedLibrary> {
    LIBRARIES
        .binary_search_by_key(&name, |library| library.name)
        .ok()
        .map(|index| &LIBRARIES[index])
}

fn default_library_for_target(target: &str) -> &'static str {
    match target.trim().to_ascii_lowercase().as_str() {
        "es2015" | "es6" => "es6",
        "es2016" | "es7" => "es2016.full",
        "es2017" => "es2017.full",
        "es2018" => "es2018.full",
        "es2019" => "es2019.full",
        "es2020" => "es2020.full",
        "es2021" => "es2021.full",
        "es2022" => "es2022.full",
        "es2023" => "es2023.full",
        "es2024" => "es2024.full",
        "esnext" | "latest" => "esnext.full",
        _ => "es2025.full",
    }
}

fn explicit_library_name(name: &str) -> Option<&'static str> {
    let mut name = name.trim().to_ascii_lowercase();
    if name == "lib.d.ts" {
        return Some("es5.full");
    }
    if let Some(stripped) = name.strip_prefix("lib.") {
        name = stripped.to_string();
    }
    if let Some(stripped) = name.strip_suffix(".d.ts") {
        name = stripped.to_string();
    }
    let alias = match name.as_str() {
        "es6" => "es2015",
        "es7" => "es2016",
        "esnext.asynciterable" => "es2018.asynciterable",
        "esnext.symbol" => "es2019.symbol",
        "esnext.bigint" => "es2020.bigint",
        "esnext.weakref" => "es2021.weakref",
        "esnext.object" => "es2024.object",
        "esnext.regexp" => "es2024.regexp",
        "esnext.string" => "es2024.string",
        "esnext.float16" => "es2025.float16",
        "esnext.iterator" => "es2025.iterator",
        "esnext.promise" => "es2025.promise",
        _ => name.as_str(),
    };
    library(alias).map(|library| library.name)
}

#[cfg(test)]
#[path = "../rewrite-tests/standard_library_unit.rs"]
mod tests;
