use super::*;

#[test]
fn typescript_type_declarations_preserve_emit_claims() {
    let file = program_file(
        0,
        "owned-declarations.ts",
        concat!(
            "interface RenamedPoint { value: number; }\n",
            "type RenamedScalar = number;\n",
            "const renamed: RenamedPoint = { value: 1 };\n",
            "function project(value: number): number { return value; }\n",
        ),
    );
    let analysis = CapabilityAnalysis::derive(
        std::slice::from_ref(&file),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    for target in [CapabilityTarget::JavaScript, CapabilityTarget::Declaration] {
        let claim = analysis.claim(target, CapabilityScope::File(file.source.id));
        assert!(
            claim.is_claimed(),
            "{target:?} must stay claimed: {claim:?}"
        );
    }

    let ordinary = program_file(0, "ordinary-recovery.ts", "const kept trailing;");
    assert!(
        ordinary
            .syntax
            .parser_recovery_facts()
            .iter()
            .any(|fact| { fact.kind == ParserRecoveryKind::Declaration })
    );
    let ordinary = CapabilityAnalysis::derive(
        std::slice::from_ref(&ordinary),
        &CompilerOptions::default(),
        CapabilityContext::default(),
    );
    assert!(
        [CapabilityTarget::JavaScript, CapabilityTarget::Declaration]
            .into_iter()
            .all(|target| ordinary
                .claim(target, CapabilityScope::File(FileId(0)))
                .is_claimed())
    );
}
