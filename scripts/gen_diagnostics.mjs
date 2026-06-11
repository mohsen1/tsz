#!/usr/bin/env node
// Generate crates/tsz-common/src/diagnostics/data.rs and its split data files
// from TypeScript's diagnosticMessages.json.
//
// Each diagnostic is declared exactly once per part file as
// `(NAME, code, Category, "message")`; the hand-authored
// `define_diagnostics!` macro (diagnostics/table_macro.rs) expands that
// single declaration into the code constant, the message-template constant,
// and the lookup-table entry, so the three views cannot drift apart.
//
// Types and helper functions are hand-authored in diagnostics/mod.rs.
// Usage: node scripts/gen_diagnostics.mjs [path/to/diagnosticMessages.json]

import { mkdirSync, readFileSync, rmSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

const inputPath =
  process.argv[2] ??
  join(root, "TypeScript/src/compiler/diagnosticMessages.json");

let jsonText;
try {
  jsonText = readFileSync(inputPath, "utf8");
} catch (error) {
  console.error(`Cannot read ${inputPath}: ${error.message}`);
  console.error(
    "Check out the pinned TypeScript submodule (see scripts/ci/typescript-submodule-ref)"
  );
  console.error(
    "or pass an explicit path: node scripts/gen_diagnostics.mjs <diagnosticMessages.json>"
  );
  process.exit(1);
}
let json;
try {
  json = JSON.parse(jsonText);
} catch (error) {
  console.error(`Cannot parse ${inputPath}: ${error.message}`);
  process.exit(1);
}

// Build entries sorted by code
const entries = Object.entries(json)
  .map(([message, info]) => ({
    message,
    code: info.code,
    category: info.category,
  }))
  .sort((a, b) => a.code - b.code);

// Convert a message to a SCREAMING_SNAKE_CASE constant name
function messageToConstName(message) {
  let name = message
    // Remove placeholders
    .replace(/\{(\d+)\}/g, "")
    // Remove quotes
    .replace(/[''""]/g, "")
    // Remove special characters but keep spaces/letters/digits
    .replace(/[^a-zA-Z0-9\s]/g, " ")
    // Collapse whitespace
    .replace(/\s+/g, " ")
    .trim()
    // To upper snake case
    .replace(/ /g, "_")
    .toUpperCase();

  // Truncate very long names
  if (name.length > 80) {
    name = name.substring(0, 80).replace(/_$/, "");
  }

  // Ensure doesn't start with a digit
  if (/^\d/.test(name)) {
    name = "D_" + name;
  }

  return name || "UNKNOWN";
}

// Generate constant names, resolving conflicts
const usedNames = new Set();
const codeEntries = []; // { code, category, message, codeName }

for (const entry of entries) {
  const codeName = messageToConstName(entry.message);

  // Resolve conflicts
  let finalCodeName = codeName;
  let suffix = 2;
  while (usedNames.has(finalCodeName)) {
    finalCodeName = `${codeName}_${suffix}`;
    suffix++;
  }
  usedNames.add(finalCodeName);

  codeEntries.push({ ...entry, codeName: finalCodeName });
}

// Map category to the Rust enum variant name
const RUST_CATEGORIES = new Set(["Error", "Warning", "Message", "Suggestion"]);
function categoryToRust(cat) {
  return RUST_CATEGORIES.has(cat) ? cat : "Error";
}

// Escape a string for Rust
function escapeRust(s) {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, "\\n");
}

const generatedHeader = `//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run \`node scripts/gen_diagnostics.mjs\` to regenerate.
`;

function chunks(items, size) {
  const out = [];
  for (let index = 0; index < items.length; index += size) {
    out.push(items.slice(index, index + size));
  }
  return out;
}

function partName(index) {
  return `part_${String(index).padStart(3, "0")}`;
}

function diagnosticDeclaration(entry) {
  return `    (${entry.codeName}, ${entry.code}, ${categoryToRust(entry.category)}, "${escapeRust(entry.message)}"),`;
}

const dataRoot = join(root, "crates/tsz-common/src/diagnostics/data");
const partsDir = join(dataRoot, "parts");

rmSync(dataRoot, { recursive: true, force: true });
mkdirSync(partsDir, { recursive: true });

// 650 declarations plus the file header keep each generated part well under
// the repo's 2000-physical-line shard cap.
const partChunks = chunks(codeEntries, 650);

for (const [index, chunk] of partChunks.entries()) {
  writeFileSync(
    join(partsDir, `${partName(index)}.rs`),
    `${generatedHeader}
crate::diagnostics::table_macro::define_diagnostics! {
${chunk.map(diagnosticDeclaration).join("\n")}
}
`
  );
}

const partNames = partChunks.map((_, index) => partName(index));

function reExportAll(submodule) {
  return partNames
    .map((name) => `    pub use super::${name}::${submodule}::*;`)
    .join("\n");
}

writeFileSync(
  join(root, "crates/tsz-common/src/diagnostics/data.rs"),
  `${generatedHeader}
${partNames.map((name) => `#[path = "data/parts/${name}.rs"]\nmod ${name};`).join("\n")}

pub static DIAGNOSTIC_MESSAGE_SECTIONS: &[&[crate::diagnostics::DiagnosticMessage]] = &[
${partNames.map((name) => `    ${name}::MESSAGES,`).join("\n")}
];

pub fn iter_diagnostic_messages() -> impl Iterator<Item = crate::diagnostics::DiagnosticMessage> {
    DIAGNOSTIC_MESSAGE_SECTIONS
        .iter()
        .flat_map(|section| section.iter().copied())
}

/// Diagnostic message templates matching TypeScript exactly.
/// Use \`format_message()\` to fill in placeholders.
pub mod diagnostic_messages {
${reExportAll("templates")}
}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's \`diagnosticMessages.json\`.
pub mod diagnostic_codes {
${reExportAll("codes")}
}
`
);

console.log(`Generated ${codeEntries.length} diagnostic entries`);
console.log(
  `Output: crates/tsz-common/src/diagnostics/data.rs + ${partNames.length} part files`
);
