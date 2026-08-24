use super::*;

#[test]
fn generated_function_string_methods_and_member_ids_are_stable() {
    let es5 = library("es5").expect("pinned ES5 library");
    assert_eq!(
        es5.function_zero_argument_string_method_names,
        &["toString"]
    );

    let environment = StandardLibraryEnvironment::from_roots(&["es5"]);
    let owner = environment
        .resolve("Array", Meaning::Value)
        .expect("ambient Array value");
    let to_string = environment.value_member(owner, "toString", |_, _| false);
    let StandardLibraryValueMemberLookup::Found { id, kind } = to_string else {
        panic!("generated Function method: {to_string:?}");
    };
    assert_eq!(id.owner, owner);
    assert_eq!(id.local, 0);
    assert_eq!(kind, StandardLibraryMemberKind::ZeroArgumentStringMethod);
    assert_eq!(
        environment.value_member(owner, "toLocaleString", |_, _| false),
        StandardLibraryValueMemberLookup::Missing
    );
    assert_eq!(
        environment.value_member(owner, "renamedMissing", |_, _| false),
        StandardLibraryValueMemberLookup::Missing
    );

    let mut dependencies = Vec::new();
    assert_eq!(
        environment.value_member(owner, "toString", |dependency, name| {
            dependencies.push((dependency, name.to_string()));
            false
        }),
        StandardLibraryValueMemberLookup::Found { id, kind }
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
        ARRAY_FUNCTION_MEMBER_OWNER_TYPES
    );
}
