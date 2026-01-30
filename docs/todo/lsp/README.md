# LSP Implementation Action Plan

**Status:** Research Complete - Ready for Implementation
**Created:** 2026-01-30
**Teams:** 10 parallel research teams
**Total Research Output:** ~500KB of analysis across 30+ documents

---

## Executive Summary

This comprehensive action plan synthesizes research from 10 parallel teams investigating the LSP (Language Server Protocol) implementation in the tsz TypeScript compiler. The research covered all aspects of LSP functionality, from incomplete features to performance optimization and testing infrastructure.

### Current State

- **✅ 20 LSP features implemented** (~19,656 lines of Rust code)
- **✅ Solid infrastructure** (Project container, type checker, symbol indexing)
- **⚠️ 5 features partially implemented** (hover, completions, inlay hints, signature help, type hierarchy)
- **❌ 6 major features missing** (workspace symbols, call hierarchy, go to implementation, document links, type hierarchy, document colors)
- **📊 Test coverage:** 6% fourslash pass rate (3/50 tests)

### Key Findings

#### 🎉 Surprising Discoveries

1. **Completions are ALREADY FULLY IMPLEMENTED** - Just needs wiring to LSP server (2-4 hours work)
2. **Hover infrastructure is COMPLETE** - TypeInterner integration proven via Project (1-2 days work)
3. **SymbolIndex exists but is unused** - 100-1000x performance improvement waiting to be activated
4. **Intersection type reduction IS complete** - Remove from gaps list

#### ⚠️ Critical Blockers

1. **Type checker gaps** prevent hover/completions from showing accurate types (narrowing, TDZ, definite assignment)
2. **O(N) cross-file operations** make find references slow in large projects (500-2000ms for 10k files)
3. **Standard library symbols not in global scope** - breaks 15% of fourslash tests
4. **Testing infrastructure gaps** - no integration tests, protocol validation, or performance benchmarks

---

## Quick Wins (Total: 1-2 weeks)

These are high-impact, low-effort items that can be implemented immediately:

| Priority | Feature | Effort | Impact | Team |
|----------|---------|--------|--------|------|
| 1 | **Wire up Completions** to LSP server | 2-4 hours | ⭐⭐⭐⭐⭐ | Team 4 |
| 2 | **Implement Inlay Hints type hints** | 6-10 hours | ⭐⭐⭐⭐ | Team 2 |
| 3 | **Wire up Hover** with TypeInterner | 1-2 days | ⭐⭐⭐⭐⭐ | Team 3 |
| 4 | **Add Workspace Symbols** | 2 days | ⭐⭐⭐⭐⭐ | Team 7 |
| 5 | **Add Document Links** | 1 day | ⭐⭐ | Team 7 |
| 6 | **Fix signature help incomplete member calls** | 2-3 hours | ⭐⭐⭐⭐ | Team 1 |
| 7 | **Add standard library symbols** to global scope | 2-3 days | ⭐⭐⭐⭐ | Team 6 |

**Total Quick Wins:** 7 features in ~10 days → **85% LSP feature parity**

---

## Research Team Reports

Detailed findings from each research team:

| Team | Focus Area | Report Location | Key Findings |
|------|-----------|-----------------|--------------|
| **1** | Signature Help | [`research-findings/team-1-signature-help.md`](research-findings/team-1-signature-help.md) | Incomplete member calls have fallback infrastructure, just needs wiring |
| **2** | Inlay Hints | [`research-findings/team-2-inlay-hints.md`](research-findings/team-2-inlay-hints.md) | Type hints ready to implement, clear 6-10 hour path |
| **3** | Hover Implementation | [`research-findings/team-3-hover.md`](research-findings/team-3-hover.md) | Infrastructure complete, needs TypeInterner wiring |
| **4** | Completions Enhancement | [`research-findings/team-4-completions.md`](research-findings/team-4-completions.md) | **MAJOR:** Already implemented, just needs server integration |
| **5** | Type Checker Gaps | [`research-findings/team-5-type-checker-gaps.md`](research-findings/team-5-type-checker-gaps.md) | Control flow narrowing is #1 priority for LSP accuracy |
| **6** | Fourslash Tests | [`research-findings/team-6-fourslash-tests.md`](research-findings/team-6-fourslash-tests.md) | Path to 50%+ pass rate in 2-3 weeks |
| **7** | Missing LSP Features | [`research-findings/team-7-missing-features.md`](research-findings/team-7-missing-features.md) | 3-5 weeks to 95% feature parity |
| **8** | Performance & Caching | [`research-findings/team-8-performance.md`](research-findings/team-8-performance.md) | 10-1000x speedup via SymbolIndex activation |
| **9** | Cross-File Navigation | [`research-findings/team-9-cross-file.md`](research-findings/team-9-cross-file.md) | O(N) scans, SymbolIndex provides solution |
| **10** | Testing Infrastructure | [`research-findings/team-10-testing.md`](research-findings/team-10-testing.md) | Comprehensive testing strategy defined |

---

## Implementation Phases

### Phase 1: Quick Wins (Weeks 1-2) - **HIGH ROI**

**Goal:** Complete partially implemented features for maximum user value

```bash
# Week 1: Day 1-2 (4-6 hours)
- Wire up completions to LSP server (Team 4)
- Fix signature help incomplete member calls (Team 1)

# Week 1: Day 3-4 (6-10 hours)
- Implement inlay hints type hints (Team 2)

# Week 1: Day 5 (1 day)
- Add document links (Team 7)

# Week 2: Day 1-2 (1-2 days)
- Wire up hover with TypeInterner (Team 3)

# Week 2: Day 3-4 (2 days)
- Add workspace symbols (Team 7)

# Week 2: Day 5 (2-3 days)
- Add standard library symbols to global scope (Team 6)
```

**Deliverables:**
- ✅ Type-aware completions working in editor
- ✅ Hover showing full type information
- ✅ Inlay hints showing type annotations
- ✅ Signature help for incomplete member calls
- ✅ Workspace-wide symbol search
- ✅ Clickable import/export links
- ✅ Standard library globals recognized

**Success Metrics:**
- 85% LSP feature parity
- User-visible improvements in daily editing
- All quick wins verified in VS Code integration

---

### Phase 2: Type System Foundation (Weeks 3-5) - **CRITICAL**

**Goal:** Fix type checker gaps that limit LSP accuracy

Based on [Team 5's research](research-findings/team-5-type-checker-gaps.md):

| Priority | Gap | Effort | LSP Impact | Days |
|----------|-----|--------|------------|------|
| **Tier 1** | Control Flow Narrowing API | 3-5 days | Fixes hover + completions in narrowed contexts | 1-5 |
| **Tier 1** | Definite Assignment Analysis | 5-7 days | Enables diagnostics, code actions | 6-12 |
| **Tier 2** | TDZ Checking | 6-9 days | Filters completions in TDZ | 13-21 |
| **Tier 2** | Module Resolution | 4-6 days | Cross-file completions, navigation | 22-27 |

**Total Effort:** 27 days (5.5 weeks)

**Key Implementation:**

```rust
// Add to CheckerState (Day 1-5)
pub fn get_type_at_location(&self, node_idx: NodeIndex) -> Option<TypeId> {
    // Return narrowed type at cursor position
    // Fixes hover showing "string | null" instead of "string" in if blocks
}

// Implement (Days 6-12)
pub fn is_definitely_assigned_at(&self, idx: NodeIndex) -> bool {
    // Track assignments on all control flow paths
    // Enables TS2454 "variable used before assignment" diagnostics
}
```

**Deliverables:**
- ✅ Accurate hover types in narrowed contexts
- ✅ Contextually appropriate completions
- ✅ Runtime error diagnostics
- ✅ Cross-file navigation working
- ✅ Fourslash pass rate: 15-20% (up from 6%)

---

### Phase 3: Performance Optimization (Weeks 6-7) - **HIGH IMPACT**

**Goal:** Activate SymbolIndex and implement caching for 10-1000x speedup

Based on [Team 8's research](research-findings/team-8-performance.md) and [Team 9's research](research-findings/team-9-cross-file.md):

| Optimization | Impact | Effort | WASM Safe |
|--------------|--------|--------|-----------|
| SymbolIndex Integration | 100-1000x faster references | Low | ✅ Yes |
| Incremental Type Cache | 3-5x faster edits | Medium | ✅ Yes |
| Query Result Caching | 10-100x for repeats | Low | ✅ Yes |
| Region-Based Scope Caching | 2-3x faster hover/completions | Low | ✅ Yes |

**Implementation:**

```rust
// Activate SymbolIndex in Project (Day 1-2)
impl Project {
    pub fn new_with_index() -> Self {
        let mut project = Self::new();
        project.symbol_index = Some(SymbolIndex::new());
        project
    }

    pub fn find_references_indexed(&mut self, file: &str, pos: Position) -> Vec<Location> {
        // Use symbol_index.get_importing_files() instead of O(N) scan
        // 500ms → 5ms for 10k file projects
    }
}
```

**Deliverables:**
- ✅ Find references: <20ms (down from 500ms)
- ✅ Rename: <30ms (down from 250ms)
- ✅ Hover (cached): <0.5ms (down from 5ms)
- ✅ Edit response: <100ms (down from 200ms)
- ✅ Memory overhead: <20MB for symbol index

---

### Phase 4: Advanced LSP Features (Weeks 8-10) - **COMPLETE FEATURE SET**

**Goal:** Implement remaining LSP protocol features

Based on [Team 7's research](research-findings/team-7-missing-features.md):

| Feature | Complexity | Effort | Value | Dependencies |
|---------|-----------|--------|-------|--------------|
| Go to Implementation | High | 2 weeks | ⭐⭐⭐⭐⭐ | Type system |
| Call Hierarchy | Medium | 5 days | ⭐⭐⭐⭐ | Find references |
| Type Hierarchy | High | 2 weeks | ⭐⭐⭐ | Go to Implementation |
| Document Colors | Low | 1 day | ⭐ | - |
| Inline Values | Low | 2 days | ⭐ | Type checker |

**Total Effort:** 3-5 weeks

**Deliverables:**
- ✅ Go to Implementation (critical TypeScript feature)
- ✅ Call Hierarchy (incoming/outgoing calls)
- ✅ Type Hierarchy (subtypes/supertypes)
- ✅ Color picker support for CSS literals
- ✅ Inline value display for constants
- ✅ **95% LSP feature parity** achieved

---

### Phase 5: Testing Infrastructure (Weeks 11-12) - **QUALITY ASSURANCE**

**Goal:** Comprehensive testing coverage and CI/CD integration

Based on [Team 10's research](research-findings/team-10-testing.md):

| Test Type | Effort | Coverage | Priority |
|-----------|--------|----------|----------|
| Protocol validation tests | 2-3 days | JSON-RPC, framing | **High** |
| Server lifecycle tests | 1-2 days | init/shutdown/exit | **High** |
| VS Code E2E tests | 3-5 days | Real editor scenarios | **High** |
| LSP benchmark suite | 2-3 days | Performance regression | Medium |
| Concurrency tests | 2-3 days | Parallel edits, cancellation | Medium |

**Total Effort:** 2 weeks

**Deliverables:**
- ✅ Protocol-level validation tests
- ✅ Server lifecycle state machine tests
- ✅ Automated VS Code extension tests
- ✅ Performance benchmarks in CI
- ✅ Fuzz testing for protocol robustness
- ✅ **Fourslash pass rate: 50%+** (up from 6%)

---

## Success Metrics

### Feature Parity

| Phase | Parity | Features Complete |
|-------|--------|-------------------|
| **Baseline** | 75% | 20/27 features |
| **After Phase 1** | 85% | 23/27 features |
| **After Phase 2** | 90% | 24/27 features (improved accuracy) |
| **After Phase 3** | 90% | 24/27 features (performance optimized) |
| **After Phase 4** | 95% | 26/27 features |
| **After Phase 5** | 95% | 26/27 features (quality assured) |

### Performance Targets

| Operation | Current | After Phase 3 | Improvement |
|-----------|---------|---------------|-------------|
| Find References (10k files) | 500-2000ms | <20ms | 100-1000x |
| Rename (10k files) | 600-2500ms | <30ms | 20-80x |
| Hover (cached) | 5ms | <0.5ms | 10x |
| Edit Response | 200ms | <100ms | 2x |
| Completions | 50ms | <20ms | 2.5x |

### Test Coverage

| Metric | Current | After Phase 5 | Target |
|--------|---------|---------------|--------|
| Fourslash Pass Rate | 6% (3/50) | 50%+ (25/50) | 50% |
| Unit Test Coverage | Good | Good | Maintain |
| Integration Tests | None | Comprehensive | 20+ scenarios |
| Protocol Tests | None | Full coverage | All messages |
| Performance Benchmarks | General | LSP-specific | 10+ operations |

---

## Estimated Timeline

| Phase | Duration | Team Size | Deliverables |
|-------|----------|-----------|--------------|
| **Phase 1: Quick Wins** | 2 weeks | 2-3 developers | 7 features completed |
| **Phase 2: Type System** | 3 weeks | 2-3 developers | 4 type checker gaps fixed |
| **Phase 3: Performance** | 2 weeks | 1-2 developers | 10-1000x speedup |
| **Phase 4: Advanced Features** | 3 weeks | 2-3 developers | 5 remaining LSP features |
| **Phase 5: Testing** | 2 weeks | 1-2 developers | Comprehensive test coverage |
| **Total** | **12 weeks** | **2-3 developers** | **95% LSP parity** |

**Parallelization Opportunities:**
- Phases 1 & 2 can overlap (Quick Wins + Type System Foundation)
- Phases 3 & 4 can partially overlap (Performance + Advanced Features)
- Phase 5 runs throughout (testing infrastructure built incrementally)

**Accelerated Timeline (4 developers):** 8 weeks total

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Type checker complexity underestimated | Medium | High | Incremental implementation, daily testing |
| Performance optimization breaks correctness | Low | High | Extensive test coverage before optimization |
| Fourslash pass rate stalls at 30% | Medium | Medium | Focus on user-visible features over conformance |
| Developer availability | Medium | Medium | Prioritize Phase 1 features for maximum value |
| Integration with existing editor workflows | Low | Medium | Beta testing with real users |

---

## Next Steps

### Immediate Actions (This Week)

1. **Review Research Reports** - All teams have completed comprehensive analysis
2. **Prioritize Quick Wins** - Confirm Phase 1 feature order
3. **Set Up Development Environment** - Ensure all team members have testing infrastructure
4. **Create Tracking Board** - GitHub Projects or similar for task tracking

### Implementation Kickoff

1. **Start with Completions** (Team 4 finding) - 2-4 hour win for immediate user value
2. **Fix Signature Help** (Team 1 finding) - Quick fix for ignored test
3. **Implement Inlay Hints** (Team 2 finding) - Clear 6-10 hour path
4. **Wire up Hover** (Team 3 finding) - TypeInterner integration proven

### Week 1 Goals

- ✅ Completions working in VS Code
- ✅ Signature help for incomplete member calls
- ✅ Inlay hints type hints
- ✅ Hover showing type information
- ✅ 4 quick wins delivered

---

## Research Artifacts

All research reports are available in [`research-findings/`](research-findings/):

```
docs/todo/lsp/
├── README.md (this file)
├── research-findings/
│   ├── team-1-signature-help.md
│   ├── team-2-inlay-hints.md
│   ├── team-3-hover.md
│   ├── team-4-completions.md
│   ├── team-5-type-checker-gaps.md
│   ├── team-6-fourslash-tests.md
│   ├── team-7-missing-features.md
│   ├── team-8-performance.md
│   ├── team-9-cross-file.md
│   └── team-10-testing.md
├── implementation-tasks/
│   ├── phase-1-quick-wins.md
│   ├── phase-2-type-system.md
│   ├── phase-3-performance.md
│   ├── phase-4-advanced-features.md
│   └── phase-5-testing.md
└── roadmap/
    ├── timeline.md
    ├── dependencies.md
    └── success-metrics.md
```

---

## Conclusion

This action plan provides a **clear, prioritized roadmap** to achieve **95% LSP feature parity** in **12 weeks** with **2-3 developers**. The research has revealed several **surprising quick wins** (completions already implemented) and identified the **critical path** (type system foundation → performance optimization → advanced features).

The **highest ROI activities** are:
1. Wire up existing completions (2-4 hours)
2. Implement inlay hints (6-10 hours)
3. Activate SymbolIndex (100-1000x speedup)
4. Fix control flow narrowing (fixes hover + completions)

**Ready to start implementation immediately.**

---

**Last Updated:** 2026-01-30
**Research Teams:** 10 parallel teams using Gemini AI analysis
**Total Research Time:** Comprehensive investigation of ~25,000 lines of Rust code
**Confidence:** High - All findings backed by code analysis and AI validation
