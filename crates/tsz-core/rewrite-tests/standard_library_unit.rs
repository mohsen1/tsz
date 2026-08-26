use super::*;

#[test]
fn canonical_array_value_member_uses_typed_identity_and_dependencies() {
    let environment = StandardLibraryEnvironment::from_roots(&["es5"]);
    let owner = environment
        .resolve("Array", Meaning::Value)
        .expect("ambient Array value");
    let receiver = LibraryReceiver::Declaration(owner);
    let found = LibraryMemberLookup::Found(LibraryCallMember::ToString);
    assert_eq!(
        environment.call_member(receiver, "toString", |_| false, |_, _| false),
        found
    );
    assert_eq!(
        environment.call_member(receiver, "toLocaleString", |_| false, |_, _| false),
        LibraryMemberLookup::Missing
    );
    assert_eq!(
        environment.call_member(receiver, "renamedMissing", |_| false, |_, _| false),
        LibraryMemberLookup::Missing
    );

    let mut dependencies = Vec::new();
    assert_eq!(
        environment.call_member(
            receiver,
            "toString",
            |_| false,
            |dependency, name| {
                dependencies.push((dependency, name.to_string()));
                false
            }
        ),
        found
    );
    assert_eq!(dependencies.len(), 3);
    assert!(dependencies.iter().all(|(_, name)| name == "toString"));
    assert_eq!(
        dependencies
            .iter()
            .map(|(dependency, _)| {
                environment
                    .declaration(*dependency)
                    .expect("dependency declaration")
                    .name
                    .as_str()
            })
            .collect::<Vec<_>>(),
        ["ArrayConstructor", "CallableFunction", "Function"]
    );
}

#[test]
fn canonical_array_call_members_require_es5_and_an_unmerged_owner() {
    let environment = StandardLibraryEnvironment::from_roots(&["es5"]);
    for (name, member) in [
        ("indexOf", LibraryCallMember::IndexOf),
        ("lastIndexOf", LibraryCallMember::LastIndexOf),
        ("map", LibraryCallMember::Map),
        ("push", LibraryCallMember::Push),
        ("slice", LibraryCallMember::Slice),
        ("splice", LibraryCallMember::Splice),
    ] {
        assert_eq!(
            environment.call_member(LibraryReceiver::Array, name, |_| false, |_, _| false),
            LibraryMemberLookup::Found(member)
        );
    }
    assert_eq!(
        environment.call_member(
            LibraryReceiver::Array,
            "renamedMissing",
            |_| false,
            |_, _| false
        ),
        LibraryMemberLookup::Missing
    );
    assert_eq!(
        environment.call_member(LibraryReceiver::Array, "indexOf", |_| true, |_, _| false),
        LibraryMemberLookup::DeferredUntilMemberMerging
    );
    for environment in [
        StandardLibraryEnvironment::from_roots(&[]),
        StandardLibraryEnvironment::from_roots(&["es2015.core"]),
    ] {
        assert_eq!(
            environment.call_member(LibraryReceiver::Array, "indexOf", |_| false, |_, _| false),
            LibraryMemberLookup::Missing
        );
    }
}

#[test]
fn canonical_map_call_members_require_the_type_and_value_declaration_pair() {
    let environment = StandardLibraryEnvironment::from_roots(&["es2015.collection"]);
    let owner = environment.resolve("Map", Meaning::Type).unwrap();
    let receiver = LibraryReceiver::Declaration(owner);
    assert_eq!(
        environment.call_member(receiver, "get", |_| false, |_, _| false),
        LibraryMemberLookup::Found(LibraryCallMember::MapGet)
    );
    assert_eq!(
        environment.call_member(receiver, "set", |_| false, |_, _| false),
        LibraryMemberLookup::Found(LibraryCallMember::MapSet)
    );
    assert_eq!(
        environment.call_member(receiver, "clear", |_| false, |_, _| false),
        LibraryMemberLookup::Missing
    );
    assert_eq!(
        environment.call_member(receiver, "get", |_| true, |_, _| false),
        LibraryMemberLookup::DeferredUntilMemberMerging
    );

    let type_only = StandardLibraryEnvironment::from_roots(&["es2015.iterable"]);
    let owner = type_only.resolve("Map", Meaning::Type).unwrap();
    assert_eq!(type_only.resolve("Map", Meaning::Value), None);
    assert_eq!(
        type_only.call_member(
            LibraryReceiver::Declaration(owner),
            "get",
            |_| false,
            |_, _| false
        ),
        LibraryMemberLookup::DeferredUntilMemberMerging
    );
}
