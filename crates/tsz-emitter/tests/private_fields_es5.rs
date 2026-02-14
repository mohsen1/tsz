use super::*;

#[test]
fn test_private_field_state() {
    let mut state = PrivateFieldState::new();

    state.enter_class("MyClass");
    state.register_private_field("#value", true, NodeIndex::NONE, false);
    state.register_private_field("#count", false, NodeIndex::NONE, false);

    assert!(state.has_private_fields());
    assert_eq!(
        state.get_weakmap_name("#value"),
        Some("_MyClass_value".to_string())
    );
    assert_eq!(
        state.get_weakmap_name("value"),
        Some("_MyClass_value".to_string())
    );

    let names = state.get_weakmap_names();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"_MyClass_value"));
    assert!(names.contains(&"_MyClass_count"));

    state.exit_class();
    assert!(!state.has_private_fields());
}

#[test]
fn test_generate_weakmap_var_declaration() {
    let fields = vec![
        PrivateFieldInfo {
            name: "value".to_string(),
            weakmap_name: "_C_value".to_string(),
            has_initializer: true,
            initializer: NodeIndex::NONE,
            is_static: false,
        },
        PrivateFieldInfo {
            name: "count".to_string(),
            weakmap_name: "_C_count".to_string(),
            has_initializer: false,
            initializer: NodeIndex::NONE,
            is_static: false,
        },
    ];

    let decl = generate_weakmap_var_declaration(&fields);
    assert_eq!(decl, "var _C_value, _C_count;");
}

#[test]
fn test_generate_weakmap_instantiation() {
    let fields = vec![
        PrivateFieldInfo {
            name: "value".to_string(),
            weakmap_name: "_C_value".to_string(),
            has_initializer: true,
            initializer: NodeIndex::NONE,
            is_static: false,
        },
        PrivateFieldInfo {
            name: "count".to_string(),
            weakmap_name: "_C_count".to_string(),
            has_initializer: false,
            initializer: NodeIndex::NONE,
            is_static: false,
        },
    ];

    let inst = generate_weakmap_instantiation(&fields);
    assert_eq!(inst, "_C_value = new WeakMap(), _C_count = new WeakMap();");
}
