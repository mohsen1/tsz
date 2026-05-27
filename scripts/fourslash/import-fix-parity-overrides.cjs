"use strict";

/**
 * Import-fix parity overrides for the fourslash test harness.
 *
 * Each entry is a file-path suffix (matched via `String.prototype.includes`)
 * that identifies a test where tsz's import-fix result should win over the
 * native TypeScript language service.
 *
 * Add a new entry here when tsz correctly derives a module specifier that the
 * native LS cannot (e.g. because the fixture is missing a parent package.json,
 * uses a subpath export, or relies on tsz-specific resolution logic).
 */
module.exports = [
  // Nested package.json subpath (e.g. preact/hooks): tsz correctly derives
  // "preact/hooks" via parent-dir traversal; the native LS cannot resolve the
  // specifier without the parent preact/package.json and returns nothing or
  // the wrong specifier.
  "/importFixesWithPackageJsonInSideAnotherPackage.ts",
];
