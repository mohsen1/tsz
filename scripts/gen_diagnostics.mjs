#!/usr/bin/env node
// Generate crates/tsz-core/data/diagnostics/data.rs and its split data files
// from TypeScript's diagnosticMessages.json plus typescript-go's native overlay.
//
// Each diagnostic is declared exactly once per part file as
// `(NAME, code, Category, "message")`; the hand-authored
// `define_diagnostics!` macro (diagnostics/table_macro.rs) expands that
// single declaration into the code constant, the message-template constant,
// and the lookup-table entry, so the three views cannot drift apart.
//
// Types and helper functions are hand-authored in diagnostics/mod.rs.
// Usage:
//   node scripts/gen_diagnostics.mjs [--check|--write] \
//     [path/to/diagnosticMessages.json] \
//     [path/to/extraDiagnosticMessages.json]

import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "fs";
import { dirname, join, relative } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

function usage(message) {
  if (message) {
    console.error(message);
  }
  console.error(
    "Usage: node scripts/gen_diagnostics.mjs [--check|--write] " +
      "[diagnosticMessages.json] [extraDiagnosticMessages.json]"
  );
  process.exit(2);
}

const args = process.argv.slice(2);
const flags = args.filter((arg) => arg.startsWith("--"));
const positional = args.filter((arg) => !arg.startsWith("--"));
const unknownFlags = flags.filter(
  (flag) => flag !== "--check" && flag !== "--write"
);

if (unknownFlags.length > 0) {
  usage(`Unknown option: ${unknownFlags.join(", ")}`);
}
if (flags.includes("--check") && flags.includes("--write")) {
  usage("Choose only one of --check or --write.");
}
if (positional.length > 2) {
  usage("Expected at most two input paths.");
}

const mode = flags.includes("--check") ? "check" : "write";
const inputPath =
  positional[0] ?? join(root, "TypeScript/src/compiler/diagnosticMessages.json");
const overlayPath =
  positional[1] ??
  join(
    root,
    "vendor/typescript-go/internal/diagnostics/extraDiagnosticMessages.json"
  );

const RUST_CATEGORIES = new Set(["Error", "Warning", "Message", "Suggestion"]);

function readMessageEntries(path, label) {
  let jsonText;
  try {
    jsonText = readFileSync(path, "utf8");
  } catch (error) {
    console.error(`Cannot read ${label} ${path}: ${error.message}`);
    if (label === "base diagnostics") {
      console.error(
        "Check out the pinned TypeScript submodule (see scripts/ci/typescript-submodule-ref)."
      );
    }
    process.exit(1);
  }

  let json;
  try {
    json = JSON.parse(jsonText);
  } catch (error) {
    console.error(`Cannot parse ${label} ${path}: ${error.message}`);
    process.exit(1);
  }

  if (json === null || typeof json !== "object" || Array.isArray(json)) {
    console.error(`${label} ${path} must contain a JSON object.`);
    process.exit(1);
  }

  const entries = [];
  const seenCodes = new Map();
  for (const [message, info] of Object.entries(json)) {
    if (info === null || typeof info !== "object" || Array.isArray(info)) {
      console.error(`${label} entry ${JSON.stringify(message)} must be an object.`);
      process.exit(1);
    }
    if (!Number.isInteger(info.code) || info.code <= 0) {
      console.error(
        `${label} entry ${JSON.stringify(message)} has invalid code ${JSON.stringify(info.code)}.`
      );
      process.exit(1);
    }
    if (!RUST_CATEGORIES.has(info.category)) {
      console.error(
        `${label} entry TS${info.code} has unsupported category ${JSON.stringify(info.category)}.`
      );
      process.exit(1);
    }

    const previous = seenCodes.get(info.code);
    if (previous !== undefined) {
      console.error(
        `${label} assigns TS${info.code} to both ${JSON.stringify(previous)} and ${JSON.stringify(message)}.`
      );
      process.exit(1);
    }
    seenCodes.set(info.code, message);
    entries.push({ message, code: info.code, category: info.category });
  }

  return entries.sort((a, b) => a.code - b.code);
}

// Convert a message to a SCREAMING_SNAKE_CASE constant name.
function messageToConstName(message) {
  let name = message
    // Remove placeholders.
    .replace(/\{(\d+)\}/g, "")
    // Remove quotes.
    .replace(/[''"]/g, "")
    // Remove special characters but keep spaces/letters/digits.
    .replace(/[^a-zA-Z0-9\s]/g, " ")
    // Collapse whitespace.
    .replace(/\s+/g, " ")
    .trim()
    // Convert to upper snake case.
    .replace(/ /g, "_")
    .toUpperCase();

  // Truncate very long names.
  if (name.length > 80) {
    name = name.substring(0, 80).replace(/_$/, "");
  }

  // Ensure the identifier does not start with a digit.
  if (/^\d/.test(name)) {
    name = "D_" + name;
  }

  return name || "UNKNOWN";
}

const usedNames = new Set();
function assignCodeName(entry) {
  const codeName = messageToConstName(entry.message);
  let finalCodeName = codeName;
  let suffix = 2;
  while (usedNames.has(finalCodeName)) {
    finalCodeName = `${codeName}_${suffix}`;
    suffix++;
  }
  usedNames.add(finalCodeName);
  return { ...entry, codeName: finalCodeName };
}

const baseEntries = readMessageEntries(inputPath, "base diagnostics");
const overlayEntries = readMessageEntries(overlayPath, "native diagnostics overlay");

// Assign names to the legacy catalog first. When typescript-go replaces a
// diagnostic by numeric code, use the native message-derived name and retain
// the legacy name as a generated compatibility alias.
const mergedByCode = new Map(
  baseEntries.map(assignCodeName).map((entry) => [entry.code, entry])
);
const compatibilityAliases = [];
let replacedCount = 0;
let addedCount = 0;
for (const overlay of overlayEntries) {
  const existing = mergedByCode.get(overlay.code);
  if (existing) {
    const merged = {
      ...existing,
      message: overlay.message,
      category: overlay.category,
    };
    if (
      messageToConstName(overlay.message) !==
      messageToConstName(existing.message)
    ) {
      const renamed = assignCodeName(overlay);
      merged.codeName = renamed.codeName;
      compatibilityAliases.push({
        code: overlay.code,
        legacyName: existing.codeName,
        nativeName: renamed.codeName,
      });
    }
    mergedByCode.set(overlay.code, merged);
    replacedCount++;
  } else {
    mergedByCode.set(overlay.code, assignCodeName(overlay));
    addedCount++;
  }
}

const codeEntries = [...mergedByCode.values()].sort((a, b) => a.code - b.code);

// Escape a string for Rust.
function escapeRust(s) {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, "\\n");
}

const generatedHeader = `//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run \`node scripts/setup/sync-typescript-diagnostics.mjs --write\` to regenerate.
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
  return `    (${entry.codeName}, ${entry.code}, ${entry.category}, "${escapeRust(entry.message)}"),`;
}

// 650 declarations plus the file header keep each generated part well under
// the repo's 2000-physical-line shard cap.
const partChunks = chunks(codeEntries, 650);
const partNames = partChunks.map((_, index) => partName(index));
const partByCode = new Map();
for (const [index, chunk] of partChunks.entries()) {
  for (const entry of chunk) {
    partByCode.set(entry.code, partName(index));
  }
}

function reExportAll(submodule) {
  return partNames
    .map((name) => `    pub use super::${name}::${submodule}::*;`)
    .join("\n");
}

function reExportCompatibilityAliases(submodule) {
  if (compatibilityAliases.length === 0) {
    return "";
  }
  return (
    "\n\n    // Pre-overlay identifiers retained for source compatibility.\n" +
    compatibilityAliases
      .toSorted((left, right) => {
        const leftPart = partByCode.get(left.code);
        const rightPart = partByCode.get(right.code);
        return (
          leftPart.localeCompare(rightPart) ||
          left.nativeName.localeCompare(right.nativeName)
        );
      })
      .map(
        (alias) =>
          `    #[doc(hidden)]\n    pub use super::${partByCode.get(alias.code)}::${submodule}::${alias.nativeName} as ${alias.legacyName};`
      )
      .join("\n")
  );
}

const dataRoot = join(root, "crates/tsz-core/data/diagnostics/data");
const partsDir = join(dataRoot, "parts");
const generatedFiles = new Map();

for (const [index, chunk] of partChunks.entries()) {
  generatedFiles.set(
    join(partsDir, `${partName(index)}.rs`),
    `${generatedHeader}
crate::diagnostics::table_macro::define_diagnostics! {
${chunk.map(diagnosticDeclaration).join("\n")}
}
`
  );
}

generatedFiles.set(
  join(root, "crates/tsz-core/data/diagnostics/data.rs"),
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
${reExportAll("templates")}${reExportCompatibilityAliases("templates")}
}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's merged diagnostic catalogs.
pub mod diagnostic_codes {
${reExportAll("codes")}${reExportCompatibilityAliases("codes")}
}
`
);

if (mode === "check") {
  const stale = [];
  for (const [path, expected] of generatedFiles) {
    if (!existsSync(path) || readFileSync(path, "utf8") !== expected) {
      stale.push(relative(root, path));
    }
  }

  const expectedParts = new Set(
    [...generatedFiles.keys()]
      .filter((path) => dirname(path) === partsDir)
      .map((path) => relative(partsDir, path))
  );
  const existingParts = existsSync(partsDir)
    ? readdirSync(partsDir).filter((name) => name.endsWith(".rs"))
    : [];
  for (const part of existingParts) {
    if (!expectedParts.has(part)) {
      stale.push(relative(root, join(partsDir, part)));
    }
  }

  if (stale.length > 0) {
    console.error("Generated diagnostics are stale:");
    for (const path of stale.sort()) {
      console.error(`  ${path}`);
    }
    console.error(
      "Run node scripts/setup/sync-typescript-diagnostics.mjs --write."
    );
    process.exit(1);
  }
} else {
  rmSync(dataRoot, { recursive: true, force: true });
  mkdirSync(partsDir, { recursive: true });
  for (const [path, contents] of generatedFiles) {
    writeFileSync(path, contents);
  }
}

const action = mode === "check" ? "Checked" : "Generated";
console.log(
  `${action} ${codeEntries.length} diagnostic entries (${baseEntries.length} base, ${replacedCount} replaced, ${addedCount} added, ${compatibilityAliases.length} compatibility aliases)`
);
console.log(
  `Output: crates/tsz-core/data/diagnostics/data.rs + ${partNames.length} part files`
);
