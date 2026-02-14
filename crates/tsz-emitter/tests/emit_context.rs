use super::*;

#[test]
fn test_emit_flags_defaults() {
    let flags = EmitFlags::new();
    assert!(!flags.in_async);
    assert!(!flags.in_generator);
    assert!(!flags.capture_this);
}

#[test]
fn test_arrow_transform_state() {
    let mut state = ArrowTransformState::default();

    assert!(!state.is_capturing_this());

    state.enter_arrow_with_this();
    assert!(state.is_capturing_this());

    state.enter_arrow_with_this();
    assert_eq!(state.this_capture_depth, 2);

    state.exit_arrow_with_this();
    assert!(state.is_capturing_this());

    state.exit_arrow_with_this();
    assert!(!state.is_capturing_this());
}

#[test]
fn test_destructuring_temp_vars() {
    let mut state = DestructuringState::default();

    assert_eq!(state.next_temp_var(), "_a");
    assert_eq!(state.next_temp_var(), "_b");
    assert_eq!(state.next_temp_var(), "_c");

    state.reset();
    assert_eq!(state.next_temp_var(), "_a");
}

#[test]
fn test_emit_context_es5_detection() {
    let es5 = EmitContext::es5();
    assert!(es5.is_es5());

    let es6 = EmitContext::es6();
    assert!(!es6.is_es5());
}

#[test]
fn test_module_state() {
    let mut state = ModuleTransformState::default();

    assert!(!state.commonjs_mode);

    state.enter_commonjs();
    assert!(state.commonjs_mode);

    state.add_export("foo".to_string());
    state.add_export("bar".to_string());

    let exports = state.take_exports();
    assert_eq!(exports, vec!["foo", "bar"]);
    assert!(state.pending_exports.is_empty());
}
