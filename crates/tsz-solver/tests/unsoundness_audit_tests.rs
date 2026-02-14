use super::*;

#[test]
fn test_audit_completeness() {
    let audit = UnsoundnessAudit::new();
    assert_eq!(audit.rules.len(), 44, "Should have all 44 rules");
}

#[test]
fn test_phase_distribution() {
    let audit = UnsoundnessAudit::new();
    // Phase 1 should have 5 rules
    assert_eq!(audit.rules_by_phase(ImplementationPhase::Phase1).len(), 5);
    // Phase 2 should have 5 rules
    assert_eq!(audit.rules_by_phase(ImplementationPhase::Phase2).len(), 5);
    // Phase 3 should have 5 rules
    assert_eq!(audit.rules_by_phase(ImplementationPhase::Phase3).len(), 5);
    // Phase 4 should have 29 rules (all others)
    assert_eq!(audit.rules_by_phase(ImplementationPhase::Phase4).len(), 29);
}

#[test]
fn test_enum_rules_status() {
    let audit = UnsoundnessAudit::new();
    // All enum rules should be fully implemented
    for rule_num in [7u8, 24, 34] {
        let rule = audit.get_rule_status(rule_num).unwrap();
        assert_eq!(rule.status, ImplementationStatus::FullyImplemented);
        assert_eq!(rule.phase, ImplementationPhase::Phase4);
    }
}

#[test]
fn test_phase1_rules_status() {
    let audit = UnsoundnessAudit::new();
    // Phase 1 rules should be at least partially implemented
    for rule in audit.rules_by_phase(ImplementationPhase::Phase1) {
        assert!(
            rule.status.is_implemented(),
            "Phase 1 rule #{} should be implemented",
            rule.rule_number
        );
    }
}

#[test]
fn test_missing_rules_count() {
    let audit = UnsoundnessAudit::new();
    let missing = audit.missing_rules();
    // All rules should now be implemented
    assert!(
        missing.is_empty(),
        "All rules should be implemented, but found {} missing: {:?}",
        missing.len(),
        missing.iter().map(|r| &r.rule_number).collect::<Vec<_>>()
    );
}

#[test]
fn test_summary_report_generation() {
    let audit = UnsoundnessAudit::new();
    let report = audit.summary_report();
    assert!(report.contains("Overall Status"));
    assert!(report.contains("Completion by Phase"));
    assert!(report.contains("Critical Gaps"));
    assert!(report.contains("Interdependencies"));
}

#[test]
fn test_matrix_table_generation() {
    let audit = UnsoundnessAudit::new();
    let table = audit.matrix_table();
    // Check table headers
    assert!(table.contains("| # | Rule | Phase |"));
    // Check some known rules are present
    assert!(table.contains("The \"Any\" Type"));
    assert!(table.contains("Open Numeric Enums"));
    assert!(table.contains("String Enums"));
}
