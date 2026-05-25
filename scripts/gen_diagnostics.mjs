#!/usr/bin/env node
// Generate crates/tsz-common/src/diagnostics/data.rs and its split data files
// from TypeScript's diagnosticMessages.json.
// Types and helper functions are hand-authored in diagnostics/mod.rs.
// Usage: node scripts/gen_diagnostics.mjs

import { mkdirSync, readFileSync, rmSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

const json = JSON.parse(
  readFileSync(
    join(root, "TypeScript/src/compiler/diagnosticMessages.json"),
    "utf8"
  )
);

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
const codeEntries = []; // { code, category, message, codeName, msgName }

for (const entry of entries) {
  // Generate code name
  let codeName = messageToConstName(entry.message);

  // Resolve conflicts
  let finalCodeName = codeName;
  let suffix = 2;
  while (usedNames.has(finalCodeName)) {
    finalCodeName = `${codeName}_${suffix}`;
    suffix++;
  }
  usedNames.add(finalCodeName);

  // Message name is the same as code name for simplicity
  codeEntries.push({
    ...entry,
    codeName: finalCodeName,
    codeNameParser:
      entry.message === "Import statement expects a 'from' clause."
        ? "IMPORT_EXPECTS_FROM_CLAUSE"
        : undefined,
  });
}

// Map category to Rust enum variant
function categoryToRust(cat) {
  switch (cat) {
    case "Error":
      return "DiagnosticCategory::Error";
    case "Warning":
      return "DiagnosticCategory::Warning";
    case "Message":
      return "DiagnosticCategory::Message";
    case "Suggestion":
      return "DiagnosticCategory::Suggestion";
    default:
      return "DiagnosticCategory::Error";
  }
}

// Escape a string for Rust
function escapeRust(s) {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, "\\n");
}

const generatedHeader = `//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run \`node scripts/gen_diagnostics.mjs\` to regenerate.
`;
const generatedIncludeHeader = `// Auto-generated diagnostic message data.
//
// DO NOT EDIT MANUALLY - run \`node scripts/gen_diagnostics.mjs\` to regenerate.
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

function cleanDir(dir) {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
}

function messageEntry(entry) {
  return [
    "    DiagnosticMessage {",
    `        code: ${entry.code},`,
    `        category: ${categoryToRust(entry.category)},`,
    `        message: "${escapeRust(entry.message)}",`,
    "    },",
  ].join("\n");
}

function messageConst(entry) {
  return `    pub const ${entry.codeName}: &str = "${escapeRust(entry.message)}";`;
}

function codeConst(entry) {
  const constName = entry.codeNameParser || entry.codeName;
  return `    pub const ${constName}: u32 = ${entry.code};`;
}

const dataRoot = join(root, "crates/tsz-common/src/diagnostics/data");
const messagesDir = join(dataRoot, "messages");
const diagnosticMessagesDir = join(dataRoot, "diagnostic_messages");
const diagnosticCodesDir = join(dataRoot, "diagnostic_codes");

cleanDir(dataRoot);
mkdirSync(messagesDir, { recursive: true });
mkdirSync(diagnosticMessagesDir, { recursive: true });
mkdirSync(diagnosticCodesDir, { recursive: true });

const messageChunks = chunks(codeEntries.map(messageEntry), 275);
const diagnosticMessageChunks = chunks(codeEntries.map(messageConst), 650);
const diagnosticCodeChunks = chunks(codeEntries.map(codeConst), 850);

for (const [index, chunk] of messageChunks.entries()) {
  writeFileSync(
    join(messagesDir, `${partName(index)}.rs`),
    `${generatedHeader}use crate::diagnostics::{DiagnosticCategory, DiagnosticMessage};

pub static MESSAGES: &[DiagnosticMessage] = &[
${chunk.join("\n")}
];
`,
  );
}

for (const [index, chunk] of diagnosticMessageChunks.entries()) {
  writeFileSync(
    join(diagnosticMessagesDir, `${partName(index)}.rs`),
    `${generatedIncludeHeader}${chunk.join("\n")}\n`,
  );
}

for (const [index, chunk] of diagnosticCodeChunks.entries()) {
  writeFileSync(
    join(diagnosticCodesDir, `${partName(index)}.rs`),
    `${generatedIncludeHeader}${chunk.join("\n")}\n`,
  );
}

writeFileSync(
  join(dataRoot, "message_tables.rs"),
  `${generatedHeader}use crate::diagnostics::DiagnosticMessage;

${messageChunks.map((_, index) => `#[path = "messages/${partName(index)}.rs"]\npub mod ${partName(index)};`).join("\n")}

pub static DIAGNOSTIC_MESSAGE_SECTIONS: &[&[DiagnosticMessage]] = &[
${messageChunks.map((_, index) => `    ${partName(index)}::MESSAGES,`).join("\n")}
];
`,
);

const outPath = join(root, "crates/tsz-common/src/diagnostics/data.rs");
writeFileSync(
  outPath,
  `${generatedHeader}
mod message_tables;

pub use message_tables::DIAGNOSTIC_MESSAGE_SECTIONS;

pub fn iter_diagnostic_messages() -> impl Iterator<Item = crate::diagnostics::DiagnosticMessage> {
    DIAGNOSTIC_MESSAGE_SECTIONS.iter().flat_map(|section| section.iter().copied())
}

/// Diagnostic message templates matching TypeScript exactly.
/// Use \`format_message()\` to fill in placeholders.
pub mod diagnostic_messages {
${diagnosticMessageChunks.map((_, index) => `    include!("data/diagnostic_messages/${partName(index)}.rs");`).join("\n")}
}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's \`diagnosticMessages.json\`.
pub mod diagnostic_codes {
${diagnosticCodeChunks.map((_, index) => `    include!("data/diagnostic_codes/${partName(index)}.rs");`).join("\n")}
}
`,
);

console.log(`Generated ${codeEntries.length} diagnostic entries`);
console.log(`Output: crates/tsz-common/src/diagnostics/data.rs + split data parts`);
