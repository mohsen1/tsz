//! TypeScript Unsoundness Catalog Implementation Audit
//!
//! This module tracks the implementation status of all 44 TypeScript unsoundness
//! rules from specs/TS_UNSOUNDNESS_CATALOG.md against the actual solver implementation.
//!
//! ## Audit Matrix
//!
//! The audit maps each catalog rule to its implementation status:
//! - ✅ FULLY_IMPLEMENTED: Rule is complete and tested
//! - ⚠️ PARTIALLY_IMPLEMENTED: Partial implementation with gaps
//! - ❌ NOT_IMPLEMENTED: Rule is missing
//! - 🚫 BLOCKED: Rule cannot be implemented yet (dependencies missing)
//!
//! ## Usage
//!
//! ```rust
//! use crate::solver::unsoundness_audit::UnsoundnessAudit;
//!
//! let audit = UnsoundnessAudit::new();
//! let status = audit.get_rule_status(7); // Open Numeric Enums
//! println!("Rule #7: {:?}", status);
//! ```

use std::collections::HashMap;
use std::fmt;

/// Implementation status of a single catalog rule
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplementationStatus {
    /// Rule is fully implemented with comprehensive tests
    FullyImplemented,
    /// Rule is partially implemented - core logic exists but gaps remain
    PartiallyImplemented,
    /// Rule is not implemented at all
    NotImplemented,
    /// Rule is blocked by missing dependencies (e.g., type system features)
    Blocked,
    /// Rule is N/A for this compiler (different architecture)
    NotApplicable,
}

impl ImplementationStatus {
    /// Returns true if the rule is at least partially implemented
    pub fn is_implemented(self) -> bool {
        matches!(
            self,
            ImplementationStatus::FullyImplemented | ImplementationStatus::PartiallyImplemented
        )
    }

    /// Returns the completion percentage (0.0, 0.5, or 1.0)
    pub fn completion_ratio(self) -> f32 {
        match self {
            ImplementationStatus::FullyImplemented => 1.0,
            ImplementationStatus::PartiallyImplemented => 0.5,
            ImplementationStatus::NotImplemented | ImplementationStatus::Blocked => 0.0,
            ImplementationStatus::NotApplicable => 1.0,
        }
    }

    /// Returns the emoji representation for display
    pub fn emoji(self) -> &'static str {
        match self {
            ImplementationStatus::FullyImplemented => "✅",
            ImplementationStatus::PartiallyImplemented => "⚠️",
            ImplementationStatus::NotImplemented => "❌",
            ImplementationStatus::Blocked => "🚫",
            ImplementationStatus::NotApplicable => "➖",
        }
    }
}

/// Phase categorization from the catalog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplementationPhase {
    /// Phase 1: Hello World Barrier (Bootstrapping)
    /// Required to compile lib.d.ts and basic variables
    Phase1,
    /// Phase 2: Business Logic Barrier (Common Patterns)
    /// Required for standard application code
    Phase2,
    /// Phase 3: Library Barrier (Complex Types)
    /// Required for modern npm packages
    Phase3,
    /// Phase 4: Feature Barrier (Edge Cases)
    /// Required for 100% test suite compliance
    Phase4,
}

impl fmt::Display for ImplementationPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImplementationPhase::Phase1 => write!(f, "Phase 1: Hello World"),
            ImplementationPhase::Phase2 => write!(f, "Phase 2: Business Logic"),
            ImplementationPhase::Phase3 => write!(f, "Phase 3: Library"),
            ImplementationPhase::Phase4 => write!(f, "Phase 4: Feature"),
        }
    }
}

/// Information about a single catalog rule implementation
#[derive(Debug, Clone)]
pub struct RuleImplementation {
    /// Rule number from the catalog (1-44)
    pub rule_number: u8,
    /// Rule name/title
    pub name: &'static str,
    /// Implementation phase from catalog
    pub phase: ImplementationPhase,
    /// Current implementation status
    pub status: ImplementationStatus,
    /// File(s) where this rule is implemented
    pub implementation_files: Vec<&'static str>,
    /// Test coverage percentage (estimated)
    pub test_coverage: f32,
    /// Dependencies on other rules (by rule number)
    pub dependencies: Vec<u8>,
    /// Notes about implementation gaps
    pub notes: &'static str,
}

/// Audit report for the TypeScript unsoundness catalog
#[derive(Debug, Clone)]
pub struct UnsoundnessAudit {
    rules: HashMap<u8, RuleImplementation>,
}

impl UnsoundnessAudit {
    /// Create a new audit with the current implementation status
    pub fn new() -> Self {
        let mut rules = HashMap::new();

        // =========================================================================
        // PHASE 1: Hello World Barrier (Bootstrapping)
        // =========================================================================

        rules.insert(1, RuleImplementation {
            rule_number: 1,
            name: "The \"Any\" Type",
            phase: ImplementationPhase::Phase1,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/lawyer.rs", "src/solver/compat.rs"],
            test_coverage: 0.95,
            dependencies: vec![],
            notes: "Fully implemented in lawyer.rs with AnyPropagationRules. Handles top/bottom semantics and suppression rules.",
        });

        rules.insert(20, RuleImplementation {
            rule_number: 20,
            name: "The `Object` vs `object` vs `{}` Trifecta",
            phase: ImplementationPhase::Phase1,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/compat.rs", "src/solver/subtype.rs"],
            test_coverage: 0.70,
            dependencies: vec![12],
            notes: "Empty object {} handling implemented in compat.rs. Object interface and object keyword partially done. Primitive assignability to Object needs completion.",
        });

        rules.insert(6, RuleImplementation {
            rule_number: 6,
            name: "Void Return Exception",
            phase: ImplementationPhase::Phase1,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/subtype.rs"],
            test_coverage: 0.90,
            dependencies: vec![],
            notes: "allow_void_return flag in SubtypeChecker. Functions returning void accept non-void returns.",
        });

        rules.insert(11, RuleImplementation {
            rule_number: 11,
            name: "Error Poisoning",
            phase: ImplementationPhase::Phase1,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/compat.rs", "src/solver/subtype.rs"],
            test_coverage: 0.60,
            dependencies: vec![],
            notes: "ERROR types do not silently pass in compat.rs (lines 158-164, 203-210, 254-256). However, Union(Error, T) suppression not fully implemented.",
        });

        rules.insert(3, RuleImplementation {
            rule_number: 3,
            name: "Covariant Mutable Arrays",
            phase: ImplementationPhase::Phase1,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/subtype.rs"],
            test_coverage: 0.95,
            dependencies: vec![],
            notes: "Array covariance implemented in subtype.rs (line 572-575). Dog[] assignable to Animal[] despite unsafety.",
        });

        // =========================================================================
        // PHASE 2: Business Logic Barrier (Common Patterns)
        // =========================================================================

        rules.insert(2, RuleImplementation {
            rule_number: 2,
            name: "Function Bivariance",
            phase: ImplementationPhase::Phase2,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/subtype.rs", "src/solver/compat.rs"],
            test_coverage: 0.70,
            dependencies: vec![],
            notes: "Method bivariance partially implemented. strict_function_types flag exists. Need to differentiate methods vs functions more clearly. Method bivariance disable flag added.",
        });

        rules.insert(4, RuleImplementation {
            rule_number: 4,
            name: "Freshness / Excess Property Checks",
            phase: ImplementationPhase::Phase2,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/lawyer.rs"],
            test_coverage: 0.40,
            dependencies: vec![],
            notes: "FreshnessTracker struct exists in lawyer.rs but integration with type lowering incomplete. TypeFlags::IS_FRESH not set during object literal lowering.",
        });

        rules.insert(10, RuleImplementation {
            rule_number: 10,
            name: "Literal Widening",
            phase: ImplementationPhase::Phase2,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Need widen_literal() function in lowering pass for mutable bindings (let/var).",
        });

        rules.insert(19, RuleImplementation {
            rule_number: 19,
            name: "Covariant `this` Types",
            phase: ImplementationPhase::Phase2,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. `this` appearing in method parameters should be treated covariantly, not contravariantly.",
        });

        rules.insert(14, RuleImplementation {
            rule_number: 14,
            name: "Optionality vs Undefined",
            phase: ImplementationPhase::Phase2,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/compat.rs", "src/solver/subtype.rs"],
            test_coverage: 0.85,
            dependencies: vec![],
            notes: "exact_optional_property_types flag in SubtypeChecker. Distinguishes {k?: T} from {k: T | undefined}.",
        });

        // =========================================================================
        // PHASE 3: Library Barrier (Complex Types)
        // =========================================================================

        rules.insert(25, RuleImplementation {
            rule_number: 25,
            name: "Index Signature Consistency",
            phase: ImplementationPhase::Phase3,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Need to validate all explicit properties are subtypes of index signature T.",
        });

        rules.insert(40, RuleImplementation {
            rule_number: 40,
            name: "Distributivity Disabling (`[T] extends [U]`)",
            phase: ImplementationPhase::Phase3,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Tuple wrapping to disable conditional type distribution not handled.",
        });

        rules.insert(30, RuleImplementation {
            rule_number: 30,
            name: "`keyof` Contravariance (Set Inversion)",
            phase: ImplementationPhase::Phase3,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/evaluate.rs", "src/solver/subtype.rs"],
            test_coverage: 0.50,
            dependencies: vec![],
            notes: "KeyOf evaluation exists. Union -> Intersection inversion partially implemented. Need full keyof (A | B) === keyof A & keyof B.",
        });

        rules.insert(21, RuleImplementation {
            rule_number: 21,
            name: "Intersection Reduction (Reduction to `never`)",
            phase: ImplementationPhase::Phase3,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/evaluate.rs"],
            test_coverage: 0.40,
            dependencies: vec![],
            notes: "Primitive intersection reduction exists. Disjoint object literal reduction not complete. string & number -> never works.",
        });

        rules.insert(41, RuleImplementation {
            rule_number: 41,
            name: "Key Remapping & Filtering (`as never`)",
            phase: ImplementationPhase::Phase3,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Mapped types with `as never` should remove properties from result.",
        });

        // =========================================================================
        // PHASE 4: Feature Barrier (Edge Cases)
        // =========================================================================

        // ENUMS
        rules.insert(7, RuleImplementation {
            rule_number: 7,
            name: "Open Numeric Enums",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. number <-> Enum bidirectional assignability missing. Enum type detection incomplete.",
        });

        rules.insert(24, RuleImplementation {
            rule_number: 24,
            name: "Cross-Enum Incompatibility (The Nominal Enum Rule)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![7],
            notes: "NOT IMPLEMENTED. Different enum types with same values should be rejected (nominal check).",
        });

        rules.insert(34, RuleImplementation {
            rule_number: 34,
            name: "String Enums (Strict Opaque Types)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. String literals are NOT assignable to string enums. Need identity check.",
        });

        // CLASSES
        rules.insert(5, RuleImplementation {
            rule_number: 5,
            name: "Nominal Classes (Private Members)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Classes with private/protected members should switch from structural to nominal compatibility.",
        });

        rules.insert(18, RuleImplementation {
            rule_number: 18,
            name: "Class \"Static Side\" Rules",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. typeof Class comparisons need special handling for static side. Protected static members are nominal.",
        });

        rules.insert(43, RuleImplementation {
            rule_number: 43,
            name: "Abstract Class Instantiation",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Abstract class constructor checking needed. Cannot instantiate abstract classes.",
        });

        // MODULE INTEROP
        rules.insert(39, RuleImplementation {
            rule_number: 39,
            name: "`import type` Erasure (Value vs Type Space)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. import type symbols should not exist in value space. Resolver phase check needed.",
        });

        rules.insert(44, RuleImplementation {
            rule_number: 44,
            name: "Module Augmentation Merging",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Interface merging across module boundaries. Declaration collection logic needed.",
        });

        // JSX
        rules.insert(36, RuleImplementation {
            rule_number: 36,
            name: "JSX Intrinsic Lookup (Case Sensitivity)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Lowercase tags lookup in JSX.IntrinsicElements, uppercase tags as variables.",
        });

        // OTHER PHASE 4 RULES
        rules.insert(8, RuleImplementation {
            rule_number: 8,
            name: "Unchecked Indexed Access",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/subtype.rs"],
            test_coverage: 0.85,
            dependencies: vec![],
            notes: "no_unchecked_indexed_access flag in SubtypeChecker. T[K] returns T without undefined by default.",
        });

        rules.insert(9, RuleImplementation {
            rule_number: 9,
            name: "Legacy Null/Undefined",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/compat.rs", "src/solver/subtype.rs"],
            test_coverage: 0.95,
            dependencies: vec![],
            notes: "strict_null_checks flag in both CompatChecker and SubtypeChecker. Legacy mode allows null/undefined everywhere.",
        });

        rules.insert(13, RuleImplementation {
            rule_number: 13,
            name: "Weak Type Detection",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/compat.rs"],
            test_coverage: 0.90,
            dependencies: vec![],
            notes: "violates_weak_type() in compat.rs. Objects with only optional properties require overlap check.",
        });

        rules.insert(15, RuleImplementation {
            rule_number: 15,
            name: "Tuple-Array Assignment",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/subtype.rs"],
            test_coverage: 0.60,
            dependencies: vec![],
            notes: "Tuple to Array implemented (line 585-600). Array to Tuple rejection incomplete. Empty array handling partial.",
        });

        rules.insert(16, RuleImplementation {
            rule_number: 16,
            name: "Rest Parameter Bivariance",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/subtype.rs", "src/solver/compat.rs"],
            test_coverage: 0.50,
            dependencies: vec![],
            notes: "allow_bivariant_rest flag exists in SubtypeChecker and CompatChecker. Full implementation of (...args: any[]) => void as universal supertype incomplete.",
        });

        rules.insert(17, RuleImplementation {
            rule_number: 17,
            name: "The Instantiation Depth Limit",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/subtype.rs"],
            test_coverage: 0.80,
            dependencies: vec![],
            notes: "Recursion depth check at line 320-326 in subtype.rs. depth > 100 returns False with depth_exceeded flag.",
        });

        rules.insert(22, RuleImplementation {
            rule_number: 22,
            name: "Template String Expansion Limits",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Template literal cardinality check needed. Abort expansion > 100k items.",
        });

        rules.insert(23, RuleImplementation {
            rule_number: 23,
            name: "Comparison Operator Overlap (Expression Logic)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. compute_overlap(A, B) query needed. Separate from subtyping - checks if types intersect.",
        });

        rules.insert(26, RuleImplementation {
            rule_number: 26,
            name: "Split Accessors (Getter/Setter Variance)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. PropertyInfo needs read_type/write_type fields. Covariant reads, contravariant writes.",
        });

        rules.insert(27, RuleImplementation {
            rule_number: 27,
            name: "Homomorphic Mapped Types over Primitives",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![12],
            notes: "NOT IMPLEMENTED. Mapped types over primitives should map over apparent types (String, Number interfaces).",
        });

        rules.insert(28, RuleImplementation {
            rule_number: 28,
            name: "The \"Constructor Void\" Exception",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![6],
            notes: "NOT IMPLEMENTED. Constructors declared with void return should allow concrete class implementations.",
        });

        rules.insert(29, RuleImplementation {
            rule_number: 29,
            name: "The Global `Function` Type (The Untyped Callable)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Intrinsic(Function) as untyped callable supertype. Any callable assignable to Function.",
        });

        rules.insert(31, RuleImplementation {
            rule_number: 31,
            name: "Base Constraint Assignability (Generic Erasure)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/subtype.rs"],
            test_coverage: 0.60,
            dependencies: vec![],
            notes: "Type parameter checking implemented (lines 519-560). Constraint(T) <: U logic exists. Partial implementation.",
        });

        rules.insert(32, RuleImplementation {
            rule_number: 32,
            name: "Best Common Type (BCT) Inference",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Array literal type inference needs BCT algorithm. Find supertype of all candidates.",
        });

        rules.insert(33, RuleImplementation {
            rule_number: 33,
            name: "The \"Object\" vs \"Primitive\" boxing behavior",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/subtype.rs", "src/solver/apparent.rs"],
            test_coverage: 0.40,
            dependencies: vec![20],
            notes: "Primitive boxing partially implemented. apparent_primitive_members in subtype.rs. Need full Intrinsic::Number vs Ref(Symbol::Number) distinction.",
        });

        rules.insert(35, RuleImplementation {
            rule_number: 35,
            name: "The Recursion Depth Limiter (\"The Circuit Breaker\")",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::FullyImplemented,
            implementation_files: vec!["src/solver/subtype.rs"],
            test_coverage: 0.85,
            dependencies: vec![17],
            notes: "Recursion depth limiter same as Rule #17. Implemented at line 320-326. depth_exceeded flag.",
        });

        rules.insert(37, RuleImplementation {
            rule_number: 37,
            name: "`unique symbol` (Nominal Primitives)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. unique symbol as Ref(SymbolId) not Intrinsic. Nominal comparison by declaration identity.",
        });

        rules.insert(38, RuleImplementation {
            rule_number: 38,
            name: "Correlated Unions (The Cross-Product limitation)",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. IndexAccess(Union, Union) cross-product expansion needed. Do NOT implement correlated access.",
        });

        rules.insert(42, RuleImplementation {
            rule_number: 42,
            name: "CFA Invalidation in Closures",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::NotImplemented,
            implementation_files: vec![],
            test_coverage: 0.0,
            dependencies: vec![],
            notes: "NOT IMPLEMENTED. Control flow analysis narrowing reset in closures. Mutable let/var lose narrowing, const keeps it.",
        });

        rules.insert(12, RuleImplementation {
            rule_number: 12,
            name: "Apparent Members of Primitives",
            phase: ImplementationPhase::Phase4,
            status: ImplementationStatus::PartiallyImplemented,
            implementation_files: vec!["src/solver/apparent.rs", "src/solver/subtype.rs"],
            test_coverage: 0.60,
            dependencies: vec![],
            notes: "Apparent types module exists. apparent_primitive_for_key() in subtype.rs. Need full primitive to apparent type lowering.",
        });

        UnsoundnessAudit { rules }
    }

    /// Get the implementation status of a specific rule by number
    pub fn get_rule_status(&self, rule_number: u8) -> Option<&RuleImplementation> {
        self.rules.get(&rule_number)
    }

    /// Get all rules
    pub fn all_rules(&self) -> impl Iterator<Item = &RuleImplementation> {
        self.rules.values().collect::<Vec<_>>().into_iter()
    }

    /// Get rules by phase
    pub fn rules_by_phase(&self, phase: ImplementationPhase) -> Vec<&RuleImplementation> {
        self.all_rules().filter(|r| r.phase == phase).collect()
    }

    /// Get rules by status
    pub fn rules_by_status(&self, status: ImplementationStatus) -> Vec<&RuleImplementation> {
        self.all_rules().filter(|r| r.status == status).collect()
    }

    /// Count rules with given status
    pub fn count_by_status(&self, status: ImplementationStatus) -> usize {
        self.rules_by_status(status).len()
    }

    /// Calculate overall completion percentage
    pub fn overall_completion(&self) -> f32 {
        let total = self.rules.len() as f32;
        let completed = self
            .rules
            .values()
            .map(|r| r.status.completion_ratio())
            .sum::<f32>();
        if total > 0.0 { completed / total } else { 0.0 }
    }

    /// Calculate completion by phase
    pub fn completion_by_phase(&self, phase: ImplementationPhase) -> f32 {
        let rules = self.rules_by_phase(phase);
        if rules.is_empty() {
            return 0.0;
        }
        let sum = rules
            .iter()
            .map(|r| r.status.completion_ratio())
            .sum::<f32>();
        sum / rules.len() as f32
    }

    /// Get missing rules (not implemented)
    pub fn missing_rules(&self) -> Vec<&RuleImplementation> {
        self.rules_by_status(ImplementationStatus::NotImplemented)
    }

    /// Get blocked rules
    pub fn blocked_rules(&self) -> Vec<&RuleImplementation> {
        self.rules_by_status(ImplementationStatus::Blocked)
    }

    /// Generate a summary report
    pub fn summary_report(&self) -> String {
        let mut report = String::new();
        report.push_str("# TypeScript Unsoundness Catalog - Implementation Audit\n\n");

        // Overall stats
        report.push_str("## Overall Status\n\n");
        report.push_str(&format!("- **Total Rules:** 44\n",));
        report.push_str(&format!(
            "- **Fully Implemented:** {} ({:.1}%)\n",
            self.count_by_status(ImplementationStatus::FullyImplemented),
            self.count_by_status(ImplementationStatus::FullyImplemented) as f32 / 44.0 * 100.0
        ));
        report.push_str(&format!(
            "- **Partially Implemented:** {} ({:.1}%)\n",
            self.count_by_status(ImplementationStatus::PartiallyImplemented),
            self.count_by_status(ImplementationStatus::PartiallyImplemented) as f32 / 44.0 * 100.0
        ));
        report.push_str(&format!(
            "- **Not Implemented:** {} ({:.1}%)\n",
            self.count_by_status(ImplementationStatus::NotImplemented),
            self.count_by_status(ImplementationStatus::NotImplemented) as f32 / 44.0 * 100.0
        ));
        report.push_str(&format!(
            "- **Overall Completion:** {:.1}%\n\n",
            self.overall_completion() * 100.0
        ));

        // Phase breakdown
        report.push_str("## Completion by Phase\n\n");
        for phase in [
            ImplementationPhase::Phase1,
            ImplementationPhase::Phase2,
            ImplementationPhase::Phase3,
            ImplementationPhase::Phase4,
        ] {
            report.push_str(&format!(
                "- **{}:** {:.1}%\n",
                phase,
                self.completion_by_phase(phase) * 100.0
            ));
        }
        report.push('\n');

        // Critical gaps
        report.push_str("## Critical Gaps (High Priority)\n\n");
        report.push_str("### Enum Rules (Phase 4)\n");
        for rule in [7u8, 24, 34] {
            if let Some(r) = self.get_rule_status(rule) {
                report.push_str(&format!(
                    "- **Rule #{} ({}):** {} {}\n",
                    rule,
                    r.name,
                    r.status.emoji(),
                    r.notes
                ));
            }
        }
        report.push('\n');

        report.push_str("### Class Rules (Phase 4)\n");
        for rule in [5u8, 18, 43] {
            if let Some(r) = self.get_rule_status(rule) {
                report.push_str(&format!(
                    "- **Rule #{} ({}):** {} {}\n",
                    rule,
                    r.name,
                    r.status.emoji(),
                    r.notes
                ));
            }
        }
        report.push('\n');

        report.push_str("### Phase 2 Gaps (Blockers)\n");
        for rule in [10u8, 19] {
            if let Some(r) = self.get_rule_status(rule) {
                report.push_str(&format!(
                    "- **Rule #{} ({}):** {} {}\n",
                    rule,
                    r.name,
                    r.status.emoji(),
                    r.notes
                ));
            }
        }
        report.push('\n');

        // Interdependencies
        report.push_str("## Key Interdependencies\n\n");
        report.push_str("- **Weak Type Detection (#13)** ↔ **Excess Properties (#4)** ↔ **Freshness**: All three work together for object literal checks\n");
        report.push_str("- **Apparent Types (#12)** → **Object Trifecta (#20)** → **Primitive Boxing (#33)**: Primitive type handling chain\n");
        report.push_str("- **Void Return (#6)** → **Constructor Void (#28)**: Void exception applies to both functions and constructors\n");
        report.push_str("- **Enum Open (#7)** → **Cross-Enum (#24)** → **String Enum (#34)**: Enum assignability rules build on each other\n\n");

        report
    }

    /// Generate a detailed matrix table
    pub fn matrix_table(&self) -> String {
        let mut table = String::new();
        table.push_str("| # | Rule | Phase | Status | Files | Coverage | Notes |\n");
        table.push_str("|---|------|-------|--------|-------|----------|-------|\n");

        let mut rules: Vec<_> = self.rules.values().collect();
        rules.sort_by_key(|r| r.rule_number);

        for rule in rules {
            let files = rule.implementation_files.join(", ");
            let status = format!("{} {:?}", rule.status.emoji(), rule.status);
            table.push_str(&format!(
                "| {} | {} | {} | {} | {} | {:.0}% | {} |\n",
                rule.rule_number,
                rule.name,
                match rule.phase {
                    ImplementationPhase::Phase1 => "P1",
                    ImplementationPhase::Phase2 => "P2",
                    ImplementationPhase::Phase3 => "P3",
                    ImplementationPhase::Phase4 => "P4",
                },
                status,
                if files.is_empty() { "N/A" } else { &files },
                rule.test_coverage * 100.0,
                rule.notes
            ));
        }

        table
    }
}

impl Default for UnsoundnessAudit {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
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
        // All enum rules should be not implemented
        for rule_num in [7u8, 24, 34] {
            let rule = audit.get_rule_status(rule_num).unwrap();
            assert_eq!(rule.status, ImplementationStatus::NotImplemented);
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
        // We expect some rules to be missing
        assert!(!missing.is_empty(), "Should have missing rules");
        // But not too many critical Phase 1/2 rules
        let critical_missing = missing
            .iter()
            .filter(|r| {
                matches!(
                    r.phase,
                    ImplementationPhase::Phase1 | ImplementationPhase::Phase2
                )
            })
            .count();
        assert!(
            critical_missing <= 2,
            "Should not have many critical missing rules, found {}",
            critical_missing
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
}
