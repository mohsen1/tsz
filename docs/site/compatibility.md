---
title: Compatibility
layout: layouts/base.njk
page_class: compatibility
permalink: /compatibility/index.html
---

# Compatibility

The clean-slate compiler targets the pinned TypeScript `7.0.2` implementation.
R0 is a capability gate, not a broad compatibility percentage.

## Current rewrite capability

The exact seed oracle currently covers:

- declarations, literal inference, and `let`/`var` widening;
- explicit annotations and assignment diagnostics;
- function calls, argument checking, and return diagnostics;
- object properties and a bounded union subset;
- JavaScript emit for the seed syntax;
- stable diagnostic output over ten runs and reversed root-file order.

Codes, spans, messages, order, exit status, and emitted bytes must match the
TypeScript `7.0.2` oracle. Those supported seed families form the R0 floor.

## Broad-suite status

Current full-corpus conformance, JavaScript/declaration emit, fourslash, and
project percentages are unavailable as compatibility claims. The retained
runners may publish full-corpus observations, including failures and unsupported
cases, but an observation does not expand the declared R0 capability surface.

Substantial parser recovery, project configuration, module resolution, library
loading, classes, generics, flow analysis, advanced and recursive types,
declaration emit, source maps, incremental services, full LSP/fourslash, and the
real-project corpus remain unsupported.

## Frozen legacy checkpoint

These values belong to the retired implementation at parent checkpoint
`2770da88d4` on 2026-08-20. They are historical evidence only—not rewrite
results and not an R0 floor.

| Retired suite | Frozen result |
| --- | ---: |
| Diagnostic conformance | 11,667 / 12,043 runnable cases (96.9%) |
| JavaScript emit | 11,562 / 11,563 cases |
| Declaration emit | 1,377 / 1,390 cases (99.1%) |
| Fourslash | 6,562 / 6,562 cases |

The compatibility page will regain live percentages family by family only when
the rewrite can evaluate the underlying domain honestly.
