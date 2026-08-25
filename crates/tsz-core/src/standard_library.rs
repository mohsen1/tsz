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

// Pinned TS7 apparent owners for `Array` function members. `Function.toString`
// shadows `Object.toString`, so `Object` is not a dependency for this member.
const ARRAY_FUNCTION_MEMBER_OWNER_TYPES: &[&str] =
    &["ArrayConstructor", "CallableFunction", "Function"];

#[derive(Debug)]
struct GeneratedLibrary {
    name: &'static str,
    references: &'static [&'static str],
    type_names: &'static [&'static str],
    value_names: &'static [&'static str],
    string_record_type_names: &'static [&'static str],
    function_zero_argument_string_method_names: &'static [&'static str],
}

include!(concat!(env!("OUT_DIR"), "/standard_library_data.rs"));

/// A declaration discovered from one or more selected pinned library files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardLibraryDeclaration {
    pub id: DeclId,
    pub name: String,
    pub meaning: Meaning,
}

/// Program-owned identity for a member projected from the pinned libraries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct StandardLibraryMemberId {
    owner: DeclId,
    local: u32,
}

/// Narrow semantic shapes structurally recognized by the pinned-library index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StandardLibraryMemberKind {
    ZeroArgumentStringMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StandardLibraryValueMemberLookup {
    Missing,
    DeferredUntilMemberMerging,
    Found {
        id: StandardLibraryMemberId,
        kind: StandardLibraryMemberKind,
    },
}

/// A deterministic ambient value member owned by one library declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StandardLibraryValueMember {
    id: StandardLibraryMemberId,
    kind: StandardLibraryMemberKind,
    dependency_type_owners: Option<Vec<DeclId>>,
}

/// The deterministic, program-owned ambient declaration environment.
#[derive(Debug, Default)]
pub struct StandardLibraryEnvironment {
    selected_libraries: Vec<&'static str>,
    declarations: Vec<StandardLibraryDeclaration>,
    type_names: BTreeMap<String, DeclId>,
    value_names: BTreeMap<String, DeclId>,
    value_members: BTreeMap<DeclId, BTreeMap<String, StandardLibraryValueMember>>,
    string_record_types: BTreeSet<DeclId>,
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
    pub(crate) fn is_string_record_type(&self, id: DeclId) -> bool {
        self.string_record_types.contains(&id)
    }

    #[must_use]
    pub(crate) fn is_array_type(&self, id: DeclId) -> bool {
        self.resolve("Array", Meaning::Type) == Some(id)
    }

    #[must_use]
    pub(crate) fn is_array_value(&self, id: DeclId) -> bool {
        self.resolve("Array", Meaning::Value) == Some(id)
    }

    #[must_use]
    pub(crate) fn value_member(
        &self,
        owner: DeclId,
        name: &str,
        mut has_authored_member: impl FnMut(DeclId, &str) -> bool,
    ) -> StandardLibraryValueMemberLookup {
        let Some(member) = self
            .value_members
            .get(&owner)
            .and_then(|members| members.get(name))
        else {
            return StandardLibraryValueMemberLookup::Missing;
        };
        let Some(dependency_type_owners) = &member.dependency_type_owners else {
            return StandardLibraryValueMemberLookup::DeferredUntilMemberMerging;
        };
        if dependency_type_owners
            .iter()
            .copied()
            .any(|dependency| has_authored_member(dependency, name))
        {
            StandardLibraryValueMemberLookup::DeferredUntilMemberMerging
        } else {
            StandardLibraryValueMemberLookup::Found {
                id: member.id,
                kind: member.kind,
            }
        }
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
        let mut string_record_type_names = BTreeSet::new();
        let mut function_zero_argument_string_method_names = BTreeSet::new();
        for name in &selected_libraries {
            let Some(library) = library(name) else {
                continue;
            };
            for (entries, meaning) in [(library.type_names, 0_u8), (library.value_names, 1)] {
                names.extend(entries.iter().map(|name| ((*name).to_string(), meaning)));
            }
            string_record_type_names.extend(
                library
                    .string_record_type_names
                    .iter()
                    .map(|name| (*name).to_string()),
            );
            function_zero_argument_string_method_names.extend(
                library
                    .function_zero_argument_string_method_names
                    .iter()
                    .map(|name| (*name).to_string()),
            );
        }

        let mut declarations = Vec::with_capacity(names.len());
        let mut type_names = BTreeMap::new();
        let mut value_names = BTreeMap::new();
        let mut string_record_types = BTreeSet::new();
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
                    if string_record_type_names.contains(&name) {
                        string_record_types.insert(id);
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

        let mut value_members = BTreeMap::new();
        if let Some(owner) = value_names.get("Array").copied() {
            let dependency_type_owners = ARRAY_FUNCTION_MEMBER_OWNER_TYPES
                .iter()
                .map(|name| type_names.get(*name).copied())
                .collect::<Option<Vec<_>>>();
            let members = function_zero_argument_string_method_names
                .into_iter()
                .enumerate()
                .map(|(local, name)| {
                    let member = StandardLibraryValueMember {
                        id: StandardLibraryMemberId {
                            owner,
                            local: local as u32,
                        },
                        kind: StandardLibraryMemberKind::ZeroArgumentStringMethod,
                        dependency_type_owners: dependency_type_owners.clone(),
                    };
                    (name, member)
                })
                .collect();
            value_members.insert(owner, members);
        }

        Self {
            selected_libraries,
            declarations,
            type_names,
            value_names,
            value_members,
            string_record_types,
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
