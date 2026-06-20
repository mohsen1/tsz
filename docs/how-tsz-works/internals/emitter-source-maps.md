# Source-Map Generation and the Mapped Output Pipeline

## Orientation

[emitter.md](emitter.md) introduces the emitter end to end and lists `SourceWriter`,
`SourceMapGenerator`, and "Source Map v3" among the moving parts, but treats source
maps as one line item in a much larger surface. This document fills that gap: it traces
how tsz produces `.js.map` (and `.d.ts.map`) output from the offset-mapped output buffer
all the way down to the VLQ base64 `mappings` string, and back up to where the driving
layer decides between inline and external maps and stamps the `//# sourceMappingURL=`
comment. It is the companion to [emitter-async-generator-decorators-modules.md](emitter-async-generator-decorators-modules.md),
which owns the lowering transforms whose *re-anchored* mappings this pipeline splices
in, and to [cli-surface-and-diagnostic-reporting.md](cli-surface-and-diagnostic-reporting.md)
and [lsp-and-wasm-surfaces.md](lsp-and-wasm-surfaces.md), which own the file-writing and
WASM entry points named here only where they touch source maps.

The hard architectural line is that source maps are an **emitter output concern, not a
semantic concern**. The map records "generated `(line, column)` corresponds to original
`(line, column)`"; it never consults types, never reads the printer's rendered type
output, and never patches already-emitted JavaScript to encode policy. Position
arithmetic lives in `tsz_common::source_map` and `tsz_common::position`; the wiring that
calls it lives in the emitter; the decision to *turn it on* and to *write files* lives in
the driving layer (CLI / WASM).

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| The Source Map v3 model: `Mapping`, `SourceMap`, `SourceMapGenerator`, the `vlq` module (`tsz_common::source_map`). | Any type, relation, or symbol decision — a mapping is a pure position pair. |
| UTF-16 column accounting and `(line, column)` lookup from a byte offset (`tsz_common::position::LineMap`, `output/source_writer.rs`). | Deciding *whether* to emit maps — that is a resolved-options/driver decision. |
| Tracking generated `(line, column)` as text is appended, and recording original positions per emitted token (`SourceWriter`, `Printer::queue_source_mapping`, `Printer::write_*` helpers). | Reading rendered output back to infer semantics (anti-hardcoding gate). |
| Re-anchoring nested-fragment mappings produced by lowering transforms at their splice offset (`offset_mapped_output.rs`, `add_offset_mappings`, `add_inline_capture_mappings`). | Writing `.js` / `.js.map` files or choosing inline vs external — the CLI/WASM driver owns that. |
| The `mappings` VLQ string, `sources`, `names`, `sourcesContent`, `sourceRoot`, `file`. | Module/path resolution beyond the relative-path math the driver hands it. |

## Module / file map

| Path | Role |
| --- | --- |
| `crates/tsz-common/src/source_map/mod.rs` | The kernel: `Mapping`, `SourceMap` (serde), `SourceMapGenerator`, `pub mod vlq`, plus `escape_json` / `escape_js_string` / `base64_encode`. |
| `crates/tsz-common/src/position/mod.rs` | `LineMap`: byte-offset → `(line, column)` in UTF-16 units, honoring `\n`, `\r`, `\r\n`, U+2028, U+2029. |
| `crates/tsz-emitter/src/output/source_writer.rs` | `SourceWriter`: the output buffer that tracks generated `(line, column)` and calls into `SourceMapGenerator` per token; `LineMap<'a>` thin adapter; `compute_line_col`, `source_position_from_offset`. |
| `crates/tsz-emitter/src/emitter/core_setup.rs` | `Printer::enable_source_map`, `set_source_map_text`, `queue_source_mapping`, `fast_source_position`, `generate_source_map_json`. |
| `crates/tsz-emitter/src/emitter/helpers.rs` | `Printer::write`, `write_identifier`, `write_with_end_marker`, `write_char`, delimiter writers — the consumers of `pending_source_pos`. |
| `crates/tsz-emitter/src/emitter/offset_mapped_output.rs` | `Printer::write_with_offset_mappings`: splice a pre-rendered fragment and re-base its mappings. |
| `crates/tsz-emitter/src/transforms/ir_printer_source_map.rs` | `IRPrinter::enable_mapping_capture`, `set_source_map_source_index`, `take_mappings`, `record_ast_ref_mapping`. |
| `crates/tsz-emitter/src/transforms/ir_printer.rs` / `ir_printer_class_emit.rs` | Where the IR printer records mappings for `ASTRef` / `Positioned` nodes during downlevel. |
| `crates/tsz-emitter/src/transforms/async_es5.rs` | `set_capture_mappings`, `configure_printer_capture`, `take_mappings` for `async`/generator downlevel. |
| `crates/tsz-emitter/src/declaration_emitter/core/setup.rs` / `helpers/visibility.rs` | `.d.ts.map`: `enable_source_map[_without_sources_content]`, deferred activation in `reset_writer`. |
| `crates/tsz-cli/src/driver/emit.rs` / `emit/emit_output_helpers.rs` | The orchestration: `map_output_info`, `append_source_mapping_url`, `append_inline_source_mapping_url`, file writing. |
| `crates/tsz-wasm/src/wasm_api/emit.rs` | The WASM entry point: same inline/external decision, no filesystem. |

## The data model: `Mapping`, `SourceMapGenerator`, and VLQ

The entire map is built from one struct (`tsz_common::source_map::Mapping`):

```rust
pub struct Mapping {
    pub generated_line: u32,
    pub generated_column: u32,
    pub source_index: u32,
    pub original_line: u32,
    pub original_column: u32,
    pub name_index: Option<u32>,
}
```

All five position fields are **0-indexed**, which matches Source Map v3 and tsc's internal
representation. `SourceMapGenerator` (same module) accumulates:

- `file` — the generated output basename written into the `"file"` field;
- `source_root` — the `"sourceRoot"` field (empty by default; see parity notes);
- `sources` / `sources_content` — parallel vectors, where `sources_content[i]` is
  `Some(text)` only when content was attached;
- `names` — the de-duplicated identifier name table;
- `mappings` — the flat `Vec<Mapping>` before encoding.

It also holds five `prev_*` running deltas (`prev_generated_column`, `prev_original_line`,
`prev_original_column`, `prev_source_index`, `prev_name_index`) used only during VLQ
encoding.

### Sources, names, and content

`add_source` / `add_source_with_content` push onto `sources` and `sources_content` and
return the index; both `expect(...)` on `u32` overflow rather than silently saturating,
because a wrong index would corrupt the VLQ stream (see the `# Panics` docs). `add_name`
**de-duplicates**: it linearly scans `names` for an equal string and reuses the index, so
the same identifier mapped twice contributes one `names` entry. This matches tsc, whose
`names` array is a deduplicated set keyed by text.

`sourcesContent` is conditional: `generate()` builds the `sources_content` JSON array only
`if self.sources_content.iter().any(Option::is_some)`, and `SourceMap` marks the field
`#[serde(skip_serializing_if = "Option::is_none")]`. So a map with no attached content
omits the key entirely rather than emitting `null`s — again matching tsc, which omits
`sourcesContent` unless `inlineSources` (or equivalent) was requested.

### VLQ base64 encoding (`pub mod vlq`)

The `vlq` submodule implements the standard Source Map v3 base64-VLQ scheme with the
constants `VLQ_BASE_SHIFT = 5`, `VLQ_BASE = 32`, `VLQ_BASE_MASK = 31`,
`VLQ_CONTINUATION_BIT = 32`, and the 64-character alphabet
`A-Za-z0-9+/`. The hot path is `encode_to(value: i32, buf: &mut String)`, which avoids a
per-call allocation (the comment notes it is "3-5x faster than `encode()`"):

```
value -> sign-LSB form: v = value < 0 ? ((-value)<<1)+1 : value<<1
loop:
    digit = v & 31
    v >>= 5
    if v > 0 { digit |= 32 }      // continuation bit
    push BASE64_CHARS[digit]
    if v == 0 break
```

`decode` is the inverse and exists for the round-trip tests in
`crates/tsz-common/tests/source_map.rs` and the `tsz-core` `source_map_tests_*` suites; it
is not used in the emit path.

### Assembling the `mappings` string

`generate()` is the single assembly point and runs in three steps:

1. **Sort.** `self.mappings.sort_by(...)` orders by `generated_line`, then
   `generated_column`. Mappings are appended in emit order, which is *mostly* monotonic but
   not guaranteed once nested fragments are re-anchored (a downleveled body can interleave
   re-based mappings); the sort makes the segment stream well-formed regardless.
2. **Encode.** `encode_mappings()` resets the five `prev_*` deltas to 0, walks the sorted
   mappings, and for each generated line gap pushes `';'` (resetting `prev_generated_column`
   to 0 — column deltas are **per-line**, while source index / original line / original
   column / name index deltas persist across the whole file). Within a line, segments are
   separated by `','`. Empty generated lines produce consecutive `';'`.
3. **Encode one segment.** `encode_segment` emits 4 or 5 VLQ values — generated column,
   source index, original line, original column, and (only if `name_index.is_some()`) the
   name index — each as a **delta** from the corresponding `prev_*`, then updates that
   `prev_*`. Every field is range-checked with `i32::try_from(...).expect(...)`; the
   `# Panics` doc explains that a value above `i32::MAX` cannot be represented and that
   failing loudly is preferred to a silently corrupt map.

`generate_json()` runs `generate()` then `serde_json::to_string`. `generate_inline()`
produces the full `//# sourceMappingURL=data:application/json;base64,<...>` comment using
the module's own `base64_encode`. These two are the only public exits, and both the CLI and
WASM drivers call them through the emitter's `generate_source_map_json` wrapper.

## Tracking generated and original positions: `SourceWriter`

`SourceWriter` (`output/source_writer.rs`) is the single chokepoint through which the
`Printer` writes all text, and the only place that knows the current generated `(line,
column)`. It owns the `output: String`, a 0-indexed `line` / `column`, indentation state,
the `new_line` string, and an `Option<SourceMapGenerator>`.

### UTF-16 columns

`raw_write` (and `raw_write_char`) advance `column` in **UTF-16 code units**, not bytes:
ASCII text takes a fast path (`segment.len()`), while non-ASCII text sums
`c.len_utf16()` per char, so a non-BMP code point (emoji, astral identifiers) advances the
column by 2. Newlines are found with `memchr::memchr(b'\n', ...)`; on each `\n` the writer
bumps `line` and resets `column` to 0. This UTF-16 accounting is mandatory: Source Map v3
columns are UTF-16 units and tsc's scanner uses the same, so byte counting would desync the
map on any file with multibyte characters.

### Per-token mapping writers

The writer exposes layered write methods. The ones that *record a mapping* all consult the
current generated `(self.line, self.column)` (already advanced past indentation by
`ensure_indent`) and then call into the generator:

| Method | Emits | Used for |
| --- | --- | --- |
| `write` / `write_raw_text` | text, **no** mapping | syntactic glue (operators, keywords without a source token, indentation) |
| `write_node(text, src_pos)` | `add_simple_mapping` then text | a token derived from a source node |
| `write_node_with_end(text, src_pos)` | start mapping, text, **then** an end mapping at `src_pos.column + text.len()` | single-char tokens (`;`, `{`, `}`) where tsc emits both a start and an end marker |
| `write_node_usize(value, src_pos)` | mapping then digits | numeric literals, written without allocating |
| `write_node_with_name(text, src_pos, name)` | `add_name(name)` then `add_mapping(..., Some(name_idx))` | identifiers (so the `names` array and name-indexed segments are populated) |
| `write_open_delimiter_node` / `write_close_delimiter_node` | `write_node` of the bracket char | source-mapped `(` `)` `[` `]` `{` `}` |

`add_simple_mapping` and `add_mapping` just push a `Mapping` with the writer's
`current_source_index` baked in; no encoding happens until `generate()`.

### Building the original position (`LineMap` and `fast_source_position`)

The "original" side of every mapping is computed from a byte offset into the source text.
`Printer::set_source_text` builds a per-file `LineMap` once (`core_setup.rs`: `self.line_map
= Some(LineMap::new(text))`), so every lookup is O(log n) binary search rather than an O(n)
re-scan. `Printer::fast_source_position(pos)` returns `LineMap::source_position(pos)` when
the map is present, falling back to `source_position_from_offset` (which rebuilds the table)
otherwise — the comment in `source_writer.rs` calls out that the standalone
`compute_line_col` "rebuilds the line table on every call," so the cached path matters for
large files.

`LineMap` (`tsz_common::position`) builds `line_starts` by scanning once for `\n`, `\r`,
`\r\n`, U+2028, and U+2029, then `line_col_utf16(offset, source)` binary-searches the line
and walks the line prefix summing `len_utf16()`. It even handles a byte offset that lands
**mid-character** by pro-rating the UTF-16 units, so a clamped/odd offset still yields a
sane column rather than panicking.

### The `pending_source_pos` handshake

The `Printer` does not call `write_node` directly from every emit site. Instead it uses a
one-slot "pending" register, so generic syntactic writes can opportunistically attach a
mapping:

- `Printer::queue_source_mapping(node)` (`core_setup.rs`) sets
  `self.pending_source_pos = self.fast_source_position(node.pos)` — but only
  `if self.writer.has_source_map()`, otherwise it clears the slot. It is called at the top
  of the central `emit` dispatch (`core.rs` ~line 1095), wrapped so the previous pending
  value is saved and restored around each node, keeping the register correctly scoped to the
  subtree.
- The `Printer::write*` helpers in `helpers.rs` then **consume** it:
  `take_pending_source_pos()` `.take()`s the slot; if it is `Some`, `write` routes to
  `writer.write_node`, `write_identifier` to `writer.write_node_with_name`,
  `write_with_end_marker` to `writer.write_node_with_end`, and so on; if it is `None`, the
  same text is written with no mapping. This is why the *first* token a node emits carries
  the node's start mapping while later glue does not — exactly the granularity tsc emits.

`helpers.rs` also offers a family of precise re-aimers (`pending_source_pos = ...` to the
opening `{`, closing `}`, trailing `;`, a scanned token byte, etc.) so a statement can map
its terminator to the right source character instead of the node start. These are pure
offset arithmetic over `source_text`; they own no semantics.

## The end-to-end JS path

```
 CLI driver (driver/emit.rs)
   │  options.source_map || options.inline_source_map ?
   ▼
 map_info = map_output_info(js_path)        // (map_path, "foo.js.map", "foo.js")
   │
   ▼
 Printer::set_source_text(src)              // builds LineMap
 Printer::set_source_map_text(src)          // text used for sourcesContent
 Printer::enable_source_map("foo.js", "src/foo.ts")
   │   └─ writer.enable_source_map("foo.js")       -> SourceMapGenerator::new
   │   └─ writer.add_source("src/foo.ts", Some(src))  // content attached
   ▼
 Printer::emit(source_file)                 // walks AST
   │   per node: queue_source_mapping(node)
   │   per token: write / write_identifier / ...  -> writer.write_node*
   │              (transforms splice via write_with_offset_mappings)
   ▼
 map_json = printer.generate_source_map_json()  -> SourceMapGenerator::generate_json
 contents = printer.take_output()
   │
   ├─ inline_source_map ? append_inline_source_mapping_url(contents, map_json)
   │                       //# sourceMappingURL=data:...;base64,<map>
   └─ else               ? append_source_mapping_url(contents, "foo.js.map")
                           //# sourceMappingURL=foo.js.map
                           + emit OutputFile { foo.js.map, map_json }
```

The decision points live entirely in the driver. `map_output_info(output_path)`
(`emit_output_helpers.rs`) derives `output_name` (the `.js` basename), `map_name`
(`"{output_name}.map"`), and the sibling `map_path`. `enable_source_map(output_name,
source_name)` registers the *output basename* as `file` and the *input file name* as the
single source, attaching the source text as content (so external `.js.map` files include
`sourcesContent` by default, matching tsc when sources are available).

`append_source_mapping_url` and `append_inline_source_mapping_url` are byte-for-byte the
tsc footer: each ensures the contents end with a newline, then pushes
`//# sourceMappingURL=` followed by either the relative `.map` filename or the base64
data-URI (the inline path re-base64-encodes the JSON via the helper's own `base64_encode`).
For external maps the driver additionally pushes an `OutputFile { path: map_path, contents:
map_json, ... }` so the `.js.map` lands beside the `.js`.

The WASM entry point (`wasm_api/emit.rs`) mirrors this exactly minus the filesystem:
`want_external_map` / `want_inline_map` gate whether `printer.enable_source_map(...)` runs;
on success it appends the same `//# sourceMappingURL=` footer (inline data-URI or
`{output_name}.map`) and returns the JSON as `source_map_text` for the host to write.

## Nested fragments: re-anchoring lowered output

The interesting part is that several lowerings (ES5 class IIFEs, `async`/generator
`__generator` bodies, downleveled modules, private-field receivers) render their text with a
**nested** emitter whose generated positions start at `(0, 0)`, then splice that string into
the outer writer at some arbitrary `(base_line, base_column)`. Their captured mappings must
be shifted to the splice point. Two mechanisms exist.

### Fragment + relative `Vec<Mapping>` (the IR printer path)

`IRPrinter` (the IR-to-text printer for ES5/async lowering) captures mappings into its own
`mappings: Vec<Mapping>` while `capture_mappings` is set:

- The owning transform calls `enable_mapping_capture()` and
  `set_source_map_source_index(self.writer.current_source_index())` so the captured mappings
  carry the same source index as the outer writer (see e.g. `es5/helpers_async.rs` ~line
  645, `async_es5.rs::configure_printer_capture`).
- `record_ast_ref_mapping(idx)` (`ir_printer_source_map.rs`) is the recorder. For an
  `ASTRef`/`Positioned` IR node it: skips leading trivia with `skip_trivia_forward` to find
  the token start, computes the *original* `(line, column)` with `compute_line_col(text,
  token_start)`, computes the *generated* `(line, column)` with `compute_line_col(&self.output,
  self.output.len())` — i.e. relative to the **fragment's** own output buffer — and pushes a
  `Mapping`. These generated coordinates are deliberately fragment-relative.
- `IRNode::Positioned { source, inner }` records the mapping at the start of the lowered node
  and then sets `suppress_ast_ref_mapping_at_output_len = Some(self.output.len())` so the
  inner `ASTRef`, if it lands at the same output length, does **not** double-record a mapping
  at the identical generated offset (`ir_printer_class_emit.rs::emit_ast_ref_node` checks and
  clears this guard). This prevents two segments pointing at the same generated column.

The transform then does `let mappings = es5_emitter.take_mappings();` followed by
`self.write_with_offset_mappings(&output, &mappings)` (the call sites are in
`transform_dispatch*.rs`, `module_emission/*`, `es5/helpers*.rs`,
`declarations/class/emit_declaration.rs`). `write_with_offset_mappings`
(`offset_mapped_output.rs`) is small but load-bearing:

```rust
if !mappings.is_empty() && self.writer.has_source_map() {
    self.writer.write("");                 // flush pending indent
    let base_line = self.writer.current_line();
    let base_column = self.writer.current_column();
    self.writer.add_offset_mappings(base_line, base_column, mappings);
    self.writer.write(rendered);
} else {
    self.write(rendered);                  // no map active: plain splice
}
```

`SourceWriter::add_offset_mappings` (`source_writer.rs`) does the re-basing: for each
fragment mapping it adds `base_line + mapping.generated_line`, and — crucially — applies the
`base_column` offset **only when `generated_line == 0`** (the first fragment line continues
the outer line; subsequent fragment lines start at column 0 in the output). The original
position, source index, and name index are passed through unchanged. A sibling
`add_mappings_with_line_column_offset` applies a column offset to *every* line, used by one
async helper path (`es5/helpers_async.rs` ~1225) where the fragment is uniformly indented.

### Scratch writer capture (the inline-capture path)

A second mechanism handles cases where a snippet is rendered through a *clone* of the live
`SourceWriter` rather than the IR printer — notably private-field receiver capture
(`expressions/core/private_fields.rs::capture_private_receiver_inline`). Here
`SourceWriter::inline_capture_from(cap)` builds a scratch writer that inherits the live
indentation, new-line, source index, and a `clone_for_inline_capture()` of the
`SourceMapGenerator` (same `sources` / `names` tables, **empty** `mappings`). The caller
swaps it in, emits, swaps the real writer back, and pulls `(String, Option<SourceMapGenerator>)`
via `take_output_and_source_map()`. Splicing then goes through
`SourceWriter::add_inline_capture_mappings(base_line, base_column, capture_map)`, which:

1. `sync_names_from_inline_capture(capture)` appends any names the scratch generator added,
   in order, `debug_assert!`-ing the shared prefix matches — this keeps captured `name_index`
   values valid after the mappings move to the main generator;
2. re-bases each captured mapping with the same first-line-only column rule as
   `add_offset_mappings`.

Both paths converge on the same invariant: **fragment-relative generated coordinates are
shifted to the splice offset; original coordinates are never touched.**

## The `.d.ts.map` path

Declaration maps reuse the *same* `SourceWriter` / `SourceMapGenerator` infrastructure but
activate it lazily, because the declaration emitter resets its writer mid-run. The
`DeclarationEmitter` stores a `source_map_state: Option<SourceMapState>` (with
`output_name`, `source_name`, `include_sources_content`) rather than an already-armed
writer. `enable_source_map` sets `include_sources_content = true`; the CLI uses
`enable_source_map_without_sources_content` instead, so `.d.ts.map` files omit
`sourcesContent` (only `sources` paths) — matching tsc, which does not inline source text
into declaration maps. The state is materialized in `reset_writer` (`helpers/visibility.rs`):
it constructs a fresh `SourceWriter::with_capacity`, then `writer.enable_source_map(output_name)`
and `writer.add_source(source_name, content)` where `content` is `Some(text)` only when the
state requested sources content. During emit, the declaration emitter records mappings with
the same `pending_source_pos` + `writer.write_node` handshake as the JS printer
(`helpers/comments_source.rs`), and the driver wires it with `map_output_info(&dts_path)`,
`declaration_map_source_name` (a relative path computed from the `.map` directory to the
input), and `append_source_mapping_url`.

## Caches and invariants

- **Per-file `LineMap` cache.** Built once in `Printer::set_source_text`; reused for every
  `fast_source_position`. Invalidation is trivial — it is rebuilt per source file because
  each `Printer` is per file. The standalone `compute_line_col` / `source_position_from_offset`
  rebuild the table each call and are only fallbacks.
- **`names` de-duplication.** `add_name` returns the existing index for an equal string, so
  the `names` table is a set; many segments can share one name index.
- **`prev_*` deltas are encode-local.** They are reset at the top of `encode_mappings` and on
  every new generated line for the column delta. They are state of the encoder, not of the
  map, so `generate()` is idempotent on the mapping set (you can call `generate_json` more
  than once; the CLI does call `generate_source_map_json` after emit).
- **Sort-before-encode.** Mappings need not be appended in generated order (re-anchored
  fragments interleave), so `generate()` sorts by `(generated_line, generated_column)` first;
  the VLQ delta stream is only valid against sorted input.
- **Overflow is loud, not silent.** `add_source`, `add_name`, and `encode_segment` all
  `expect(...)` on `u32`/`i32` overflow. A corrupt index would desync every later delta, so
  the kernel panics rather than emit a syntactically valid but wrong map.
- **Inline-capture name prefix invariant.** `clone_for_inline_capture` copies the `names`
  prefix; `sync_names_from_inline_capture` `debug_assert_eq!`s that the prefix is unchanged
  before appending, so captured `name_index` deltas stay valid after the merge.
- **Double-mapping guard.** `suppress_ast_ref_mapping_at_output_len` stops `Positioned` and
  its inner `ASTRef` from emitting two segments at the same generated offset.
- **Generated insert shifting.** When the emitter injects a whole generated line after the
  fact (e.g. hoisted `var` lines), `SourceWriter::insert_line_at` calls
  `SourceMapGenerator::shift_generated_lines(at_line, 1)` so existing mappings at/after the
  insertion line move down by one. (A plain `insert_at` that does not add a newline relies on
  the insert point preceding all mapped content on that line.)

## Edge cases and tsc parity

- **UTF-16 everywhere.** Both the generated column (`SourceWriter::raw_write`) and the
  original column (`LineMap::line_col_utf16`) count UTF-16 code units and treat `\n`, `\r`,
  `\r\n`, U+2028, U+2029 as line breaks, matching tsc's scanner line map. A file with emoji
  or CRLF endings still maps correctly.
- **Token-start mapping, not node-start.** `record_ast_ref_mapping` and the precise re-aimers
  use `skip_trivia_forward` so mappings point at the first real token character rather than at
  leading whitespace/comments, which is what tsc records.
- **Start + end markers for single-char tokens.** `write_node_with_end` emits a second mapping
  at `column + text.len()` for `;`, `{`, `}`; tsc emits these end markers so debuggers can
  step onto the punctuation.
- **`sourcesContent` presence.** External `.js.map` includes `sourcesContent` (content
  attached in `enable_source_map`); `.d.ts.map` omits it
  (`enable_source_map_without_sources_content`); a generator with no attached content omits the
  key entirely via the `skip_serializing_if` + `any(is_some)` guard.
- **`sourceRoot`.** The field is serialized (as `"sourceRoot"`) and defaults to the empty
  string. `SourceMapGenerator::set_source_root` exists, but the CLI emit path does not thread
  the `sourceRoot` compiler option through to it today, so emitted maps carry an empty
  `sourceRoot` regardless of the option. This is a known partial: the option is parsed and
  recognized (`config/mod.rs`, the `typescript.d.ts` surface) but not yet applied to the
  generator. Likewise `mapRoot` and `inlineSources` are recognized as options but not wired
  into generation — only `sourceMap`, `inlineSourceMap`, and `declarationMap` drive behavior
  in `driver/emit.rs`.
- **Inline vs external is a pure footer choice.** The *map content* is identical for both;
  only the trailing comment differs (`data:application/json;base64,...` vs `foo.js.map`), and
  external mode additionally writes the sibling file. `--inlineSourceMap` and `--sourceMap`
  are mutually exclusive at the option level (the config layer rejects the combination), so
  the driver's `if inline_source_map { ... } else { ... }` branch is unambiguous.
- **Version is always 3, `file` is the basename.** `SourceMap.version = 3`; `file` is the
  output basename (`foo.js`), not a path — matching tsc.

## Where to look next

- Lowering transforms whose fragments feed `write_with_offset_mappings`:
  [emitter-async-generator-decorators-modules.md](emitter-async-generator-decorators-modules.md).
- The broader emitter buffer, indentation, and IR printer:
  [emitter.md](emitter.md).
- File ordering, output writing, and option resolution that gate map emission:
  [cli-surface-and-diagnostic-reporting.md](cli-surface-and-diagnostic-reporting.md) and
  [driver-project-references-and-build-mode.md](driver-project-references-and-build-mode.md).
- The WASM emit surface that reuses the same inline/external decision:
  [lsp-and-wasm-surfaces.md](lsp-and-wasm-surfaces.md).
- Where emit sits in the full compile sequence:
  [end-to-end-timeline.md](end-to-end-timeline.md).
