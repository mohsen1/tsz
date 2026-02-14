use super::*;
use crate::TypeInterner;

fn create_test_interner() -> TypeInterner {
    TypeInterner::new()
}

#[test]
fn test_sound_diagnostic_formatting() {
    let diag = SoundDiagnostic::new(SoundDiagnosticCode::ExcessPropertyStickyFreshness)
        .with_arg("extraProp")
        .with_arg("{ x: number }");

    let msg = diag.format_message();
    assert!(msg.contains("extraProp"));
    assert!(msg.contains("{ x: number }"));
}

#[test]
fn test_sound_lawyer_any_escape() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let config = JudgeConfig::default();
    let mut lawyer = SoundLawyer::new(&interner, &env, config);

    // In sound mode, any -> number should be flagged
    assert!(!lawyer.is_assignable(TypeId::ANY, TypeId::NUMBER));

    // But number -> any is fine
    assert!(lawyer.is_assignable(TypeId::NUMBER, TypeId::ANY));

    // any -> any is fine
    assert!(lawyer.is_assignable(TypeId::ANY, TypeId::ANY));

    // any -> unknown is fine
    assert!(lawyer.is_assignable(TypeId::ANY, TypeId::UNKNOWN));
}

#[test]
fn test_sound_lawyer_array_covariance() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let config = JudgeConfig::default();
    let lawyer = SoundLawyer::new(&interner, &env, config);

    // Create Array<number> and Array<string>
    let array_number = interner.array(TypeId::NUMBER);
    let array_string = interner.array(TypeId::STRING);

    // These should fail
    assert!(
        lawyer
            .check_array_covariance(array_number, array_string)
            .is_none()
    );
    assert!(
        lawyer
            .check_array_covariance(array_string, array_number)
            .is_none()
    );

    // Same type is fine
    assert!(
        lawyer
            .check_array_covariance(array_number, array_number)
            .is_none()
    );
}

#[test]
fn test_sound_mode_config() {
    let all = SoundModeConfig::all();
    assert!(all.sticky_freshness);
    assert!(all.strict_any);
    assert!(all.strict_array_covariance);
    assert!(all.strict_method_bivariance);
    assert!(all.strict_enums);

    let minimal = SoundModeConfig::minimal();
    assert!(minimal.sticky_freshness);
    assert!(!minimal.strict_any);
}

#[test]
fn test_sound_diagnostic_codes() {
    assert_eq!(
        SoundDiagnosticCode::ExcessPropertyStickyFreshness.code(),
        9001
    );
    assert_eq!(SoundDiagnosticCode::MutableArrayCovariance.code(), 9002);
    assert_eq!(SoundDiagnosticCode::MethodBivariance.code(), 9003);
    assert_eq!(SoundDiagnosticCode::AnyEscape.code(), 9004);
    assert_eq!(SoundDiagnosticCode::EnumNumberAssignment.code(), 9005);
}
