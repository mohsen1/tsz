# LSP Action Plan Index

This directory contains a comprehensive, multi-file action plan for completing the LSP implementation in tsz.

## 📑 Directory Structure

```
docs/todo/lsp/
├── README.md                    # Executive summary and overview
├── INDEX.md                     # This file
├── research-findings/           # Detailed research from 10 teams
│   ├── team-1-signature-help.md
│   ├── team-2-inlay-hints.md
│   ├── team-3-hover.md
│   ├── team-4-completions.md
│   ├── team-5-type-checker-gaps.md
│   ├── team-5-gap-priorities.md
│   ├── team-6-fourslash-tests.md
│   ├── team-7-missing-features.md
│   ├── team-8-performance.md
│   ├── team-9-cross-file.md
│   └── team-10-testing.md
├── implementation-tasks/        # Detailed implementation guides
│   ├── phase-1-quick-wins.md
│   ├── phase-2-type-system.md
│   ├── phase-3-performance.md
│   ├── phase-4-advanced-features.md
│   └── phase-5-testing.md
└── roadmap/                     # Timeline and dependencies
    ├── timeline.md
    ├── dependencies.md
    └── success-metrics.md
```

---

## 🎯 Quick Start

### For Developers Starting Implementation

1. **Read the [README](../README.md)** - 5 minute overview
2. **Start with [Phase 1: Quick Wins](implementation-tasks/phase-1-quick-wins.md)** - Highest ROI tasks
3. **Review relevant research findings** - Team reports provide context
4. **Track progress** - Use task checklists in implementation guides

### For Project Managers

1. **Read the [README](../README.md)** - Executive summary and timeline
2. **Review [timeline](roadmap/timeline.md)** - 12-week implementation schedule
3. **Check [success metrics](roadmap/success-metrics.md)** - Progress tracking

---

## 📊 Research Team Reports

| Team | Focus | Key Finding | Report |
|------|-------|-------------|--------|
| **1** | Signature Help | Fallback infrastructure exists | [Read Report](research-findings/team-1-signature-help.md) |
| **2** | Inlay Hints | Clear 6-10 hour implementation path | [Read Report](research-findings/team-2-inlay-hints.md) |
| **3** | Hover | Infrastructure complete, needs wiring | [Read Report](research-findings/task-3-hover.md) |
| **4** | Completions | **ALREADY IMPLEMENTED** - just needs server wiring | [Read Report](research-findings/team-4-completions.md) |
| **5** | Type Checker Gaps | Control flow narrowing is #1 priority | [Read Report](research-findings/team-5-type-checker-gaps.md) |
| **6** | Fourslash Tests | Path to 50%+ pass rate in 2-3 weeks | [Read Report](research-findings/team-6-fourslash-tests.md) |
| **7** | Missing Features | 3-5 weeks to 95% feature parity | [Read Report](research-findings/team-7-missing-features.md) |
| **8** | Performance | SymbolIndex provides 100-1000x speedup | [Read Report](research-findings/team-8-performance.md) |
| **9** | Cross-File | O(N) scans, SymbolIndex is the solution | [Read Report](research-findings/team-9-cross-file.md) |
| **10** | Testing | Comprehensive testing strategy defined | [Read Report](research-findings/team-10-testing.md) |

---

## 🚀 Implementation Phases

### Phase 1: Quick Wins (Weeks 1-2)

**Goal:** Complete partially implemented features for maximum user value

**Duration:** 2 weeks
**Team:** 2-3 developers
**Outcome:** 85% LSP feature parity

**Tasks:**
1. Wire up Completions (2-4 hours) ⭐⭐⭐⭐⭐
2. Implement Inlay Hints type hints (6-10 hours) ⭐⭐⭐⭐
3. Wire up Hover (1-2 days) ⭐⭐⭐⭐⭐
4. Fix Signature Help (2-3 hours) ⭐⭐⭐⭐
5. Add Workspace Symbols (2 days) ⭐⭐⭐⭐⭐
6. Add Document Links (1 day) ⭐⭐
7. Add Standard Library Symbols (2-3 days) ⭐⭐⭐⭐

**Details:** [Phase 1 Implementation Guide](implementation-tasks/phase-1-quick-wins.md)

---

### Phase 2: Type System Foundation (Weeks 3-5)

**Goal:** Fix type checker gaps that limit LSP accuracy

**Duration:** 3 weeks
**Team:** 2-3 developers
**Outcome:** Accurate hover/completions in narrowed contexts

**Tasks:**
1. Control Flow Narrowing API (3-5 days) - Fixes hover + completions
2. Definite Assignment Analysis (5-7 days) - Enables diagnostics
3. TDZ Checking (6-9 days) - Filters completions
4. Module Resolution (4-6 days) - Cross-file features

**Details:** [Phase 2 Implementation Guide](implementation-tasks/phase-2-type-system.md)

---

### Phase 3: Performance Optimization (Weeks 6-7)

**Goal:** Activate SymbolIndex for 10-1000x speedup

**Duration:** 2 weeks
**Team:** 1-2 developers
**Outcome:** Find references <20ms (down from 500ms)

**Tasks:**
1. Integrate SymbolIndex (2-3 days) - 100-1000x faster references
2. Implement Incremental Type Cache (3-4 days) - 3-5x faster edits
3. Add Query Result Caching (1-2 days) - 10-100x for repeats
4. Add Region-Based Scope Caching (1-2 days) - 2-3x faster hover

**Details:** [Phase 3 Implementation Guide](implementation-tasks/phase-3-performance.md)

---

### Phase 4: Advanced LSP Features (Weeks 8-10)

**Goal:** Implement remaining LSP protocol features

**Duration:** 3 weeks
**Team:** 2-3 developers
**Outcome:** 95% LSP feature parity

**Tasks:**
1. Go to Implementation (2 weeks) - Critical TypeScript feature
2. Call Hierarchy (5 days) - Reuses FindReferences
3. Type Hierarchy (2 weeks) - Builds on Go to Implementation
4. Document Colors (1 day) - Color picker support
5. Inline Values (2 days) - Display constants inline

**Details:** [Phase 4 Implementation Guide](implementation-tasks/phase-4-advanced-features.md)

---

### Phase 5: Testing Infrastructure (Weeks 11-12)

**Goal:** Comprehensive testing coverage and CI/CD integration

**Duration:** 2 weeks
**Team:** 1-2 developers
**Outcome:** Quality assurance and regression prevention

**Tasks:**
1. Protocol Validation Tests (2-3 days)
2. Server Lifecycle Tests (1-2 days)
3. VS Code E2E Tests (3-5 days)
4. LSP Benchmark Suite (2-3 days)
5. Concurrency Tests (2-3 days)

**Details:** [Phase 5 Implementation Guide](implementation-tasks/phase-5-testing.md)

---

## 📈 Success Metrics

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

### Test Coverage

| Metric | Current | After Phase 5 | Target |
|--------|---------|---------------|--------|
| Fourslash Pass Rate | 6% (3/50) | 50%+ (25/50) | 50% |
| Unit Test Coverage | Good | Good | Maintain |
| Integration Tests | None | Comprehensive | 20+ scenarios |
| Protocol Tests | None | Full coverage | All messages |

---

## 🗓️ Timeline

### 12-Week Implementation Schedule

```
Week 1-2:  Phase 1 - Quick Wins                 → 85% parity
Week 3-5:  Phase 2 - Type System Foundation     → 90% parity (improved accuracy)
Week 6-7:  Phase 3 - Performance Optimization   → 90% parity (optimized)
Week 8-10: Phase 4 - Advanced LSP Features      → 95% parity
Week 11-12: Phase 5 - Testing Infrastructure    → 95% parity (quality assured)
```

**Parallelization Opportunities:**
- Phases 1 & 2 can overlap (Quick Wins + Type System Foundation)
- Phases 3 & 4 can partially overlap (Performance + Advanced Features)
- Phase 5 runs throughout (testing infrastructure built incrementally)

**Accelerated Timeline (4 developers):** 8 weeks total

**Details:** [Complete Timeline](roadmap/timeline.md)

---

## 🔗 Key Dependencies

### Phase Dependencies

```
Phase 1 (Quick Wins)
    ├─ Can start immediately
    └─ No dependencies

Phase 2 (Type System)
    ├─ Depends on: Nothing
    ├─ Blocks: Accurate hover/completions
    └─ Can run in parallel with Phase 1

Phase 3 (Performance)
    ├─ Depends on: Phase 1 (for SymbolIndex integration)
    ├─ Blocks: Large project usability
    └─ Can run in parallel with Phase 4

Phase 4 (Advanced Features)
    ├─ Depends on: Phase 2 (type system extensions)
    ├─ Blocks: Complete feature parity
    └─ Can run in parallel with Phase 3

Phase 5 (Testing)
    ├─ Depends on: All phases
    ├─ Blocks: Production readiness
    └─ Should run throughout (incremental)
```

**Details:** [Dependency Graph](roadmap/dependencies.md)

---

## 💡 Key Insights

### Surprising Discoveries

1. **Completions are ALREADY FULLY IMPLEMENTED** - Just needs wiring to LSP server (2-4 hours work)
2. **Hover infrastructure is COMPLETE** - TypeInterner integration proven via Project (1-2 days work)
3. **SymbolIndex exists but is unused** - 100-1000x performance improvement waiting to be activated
4. **Intersection type reduction IS complete** - Remove from gaps list

### Critical Blockers

1. **Type checker gaps** prevent hover/completions from showing accurate types (narrowing, TDZ, definite assignment)
2. **O(N) cross-file operations** make find references slow in large projects (500-2000ms for 10k files)
3. **Standard library symbols not in global scope** - breaks 15% of fourslash tests
4. **Testing infrastructure gaps** - no integration tests, protocol validation, or performance benchmarks

---

## 📞 Next Steps

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

## 📚 Additional Resources

### Documentation

- [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) - Protocol reference
- [TypeScript LSP Implementation](https://github.com/microsoft/TypeScript/tree/main/src/server) - Reference implementation
- [vscode-languageserver-node](https://github.com/microsoft/vscode-languageserver-node) - Server SDK

### Internal Documentation

- [`docs/walkthrough/08-lsp-gaps.md`](../../walkthrough/08-lsp-gaps.md) - Comprehensive LSP gap analysis
- [`docs/walkthrough/07-gaps-summary.md`](../../walkthrough/07-gaps-summary.md) - Type checker gaps affecting LSP
- [`docs/architecture/NORTH_STAR.md`](../../architecture/NORTH_STAR.md) - Target architecture

### Tools

- [`scripts/ask-gemini.mjs`](../../../../scripts/ask-gemini.mjs) - AI-assisted codebase exploration (use `--lsp` flag)
- [`scripts/run-fourslash.sh`](../../../../scripts/run-fourslash.sh) - Run fourslash test suite

---

## 🎓 Research Methodology

All research conducted using:
1. **Manual code analysis** - Comprehensive investigation of ~25,000 lines of Rust code
2. **Gemini AI analysis** - Via `./scripts/ask-gemini.mjs --lsp` for deep code analysis
3. **10 parallel research teams** - Each team focused on specific LSP aspect
4. **Cross-validation** - Findings verified across multiple teams and approaches

**Total Research Output:** ~500KB of analysis across 30+ documents

**Confidence Level:** High - All findings backed by code analysis and AI validation

---

**Last Updated:** 2026-01-30
**Status:** Research Complete - Ready for Implementation
**Confidence:** High
**Next Review:** After Phase 1 completion
