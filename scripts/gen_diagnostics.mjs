#!/usr/bin/env node
// Generate crates/tsz-common/src/diagnostics.rs from TypeScript's diagnosticMessages.json
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

// Generate the Rust file
let output = `//! Diagnostic codes and message templates for the type checker.
//!
//! AUTO-GENERATED from TypeScript's diagnosticMessages.json.
//! Do not edit manually - run \`node scripts/gen_diagnostics.mjs\` to regenerate.
//!
//! When a locale is set via \`--locale\`, messages are translated using
//! TypeScript's official locale files.

use serde::Serialize;

// =============================================================================
// Diagnostic Types
// =============================================================================

/// Diagnostic category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DiagnosticCategory {
    Warning = 0,
    Error = 1,
    Suggestion = 2,
    Message = 3,
}

/// Related information for a diagnostic (e.g., "see also" locations).
#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticRelatedInformation {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
}

/// A type-checking diagnostic message with optional related information.
#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub file: String,
    pub start: u32,
    pub length: u32,
    pub message_text: String,
    pub category: DiagnosticCategory,
    pub code: u32,
    /// Related information spans (e.g., where a type was declared)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_information: Vec<DiagnosticRelatedInformation>,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    pub fn error(file: String, start: u32, length: u32, message: String, code: u32) -> Self {
        Diagnostic {
            file,
            start,
            length,
            message_text: message,
            category: DiagnosticCategory::Error,
            code,
            related_information: Vec::new(),
        }
    }

    /// Add related information to this diagnostic.
    pub fn with_related(mut self, file: String, start: u32, length: u32, message: String) -> Self {
        self.related_information.push(DiagnosticRelatedInformation {
            file,
            start,
            length,
            message_text: message,
            category: DiagnosticCategory::Message,
            code: 0,
        });
        self
    }
}

/// Format a diagnostic message by replacing {0}, {1}, etc. with arguments.
pub fn format_message(template: &str, args: &[&str]) -> String {
    let mut result = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{}}}", i), arg);
    }
    result
}

/// A diagnostic message definition with code, category, and message template.
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticMessage {
    pub code: u32,
    pub category: DiagnosticCategory,
    pub message: &'static str,
}

/// Look up a diagnostic message definition by code.
///
/// Returns the DiagnosticMessage with template string containing \`{0}\`, \`{1}\`, etc. placeholders.
/// Use \`format_message()\` to fill in the placeholders.
pub fn get_diagnostic_message(code: u32) -> Option<&'static DiagnosticMessage> {
    DIAGNOSTIC_MESSAGES.iter().find(|m| m.code == code)
}

/// Get the message template for a diagnostic code.
///
/// Returns the template string with \`{0}\`, \`{1}\`, etc. placeholders.
/// Use \`format_message()\` to fill in the placeholders.
pub fn get_message_template(code: u32) -> Option<&'static str> {
    get_diagnostic_message(code).map(|m| m.message)
}

/// Get the category for a diagnostic code.
pub fn get_diagnostic_category(code: u32) -> Option<DiagnosticCategory> {
    get_diagnostic_message(code).map(|m| m.category)
}

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

writeFileSync(join(root, "crates/tsz-common/src/diagnostics.rs"), output);

console.log(`Generated ${codeEntries.length} diagnostic entries`);
console.log(
  `Output: crates/tsz-common/src/diagnostics.rs`
);
