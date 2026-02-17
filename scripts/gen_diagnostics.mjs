#!/usr/bin/env node
// Generate crates/tsz-common/src/diagnostics/data.rs from TypeScript's diagnosticMessages.json
// Types and helper functions are hand-authored in diagnostics/mod.rs.
// Usage: node scripts/gen_diagnostics.mjs

import { readFileSync, writeFileSync } from "fs";
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

// Generate the data file only (types live in mod.rs, which is hand-authored).
// Output goes to diagnostics/data.rs.
let output = `//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY — run \`node scripts/gen_diagnostics.mjs\` to regenerate.
use super::DiagnosticCategory;
use super::DiagnosticMessage;

/// All diagnostic messages from TypeScript's diagnosticMessages.json.
pub static DIAGNOSTIC_MESSAGES: &[DiagnosticMessage] = &[
`;

for (const entry of codeEntries) {
  output += `    DiagnosticMessage { code: ${entry.code}, category: ${categoryToRust(entry.category)}, message: "${escapeRust(entry.message)}" },\n`;
}

output += `];

/// Diagnostic message templates matching TypeScript exactly.
/// Use format_message() to fill in placeholders.
pub mod diagnostic_messages {
`;

// Generate message constants
for (const entry of codeEntries) {
  output += `    pub const ${entry.codeName}: &str = "${escapeRust(entry.message)}";\n`;
}

output += `}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's diagnosticMessages.json
pub mod diagnostic_codes {
`;

// Generate code constants
for (const entry of codeEntries) {
  output += `    pub const ${entry.codeName}: u32 = ${entry.code};\n`;
}


output += `}
`;

const outPath = join(root, "crates/tsz-common/src/diagnostics/data.rs");
writeFileSync(outPath, output);

console.log(`Generated ${codeEntries.length} diagnostic entries`);
console.log(`Output: crates/tsz-common/src/diagnostics/data.rs`);
