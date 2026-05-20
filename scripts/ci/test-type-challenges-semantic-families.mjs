#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  normalizePath,
  semanticFamiliesForFile,
  semanticFamiliesForText,
} from "./type-challenges-semantic-families.mjs";

function withTempDir(fn) {
  const dir = fs.mkdtempSync(
    path.join(os.tmpdir(), "tsz-type-challenges-families-"),
  );
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeFile(file, text) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text, "utf8");
}

assert.equal(normalizePath("assertions\\nested\\case.ts"), "assertions/nested/case.ts");

const cases = [
  {
    text: "type Split<S> = S extends `${infer Head}_${infer Tail}` ? Head : never;",
    families: [
      "template literal inference",
      "recursive conditionals",
      "distributive conditionals",
      "inference cache/session behavior",
    ],
  },
  {
    text: "type Remap<T> = { [K in keyof T as K]: T[K] };",
    families: ["mapped/key-remapped types", "indexed access"],
  },
  {
    text: "type First<T extends unknown[]> = T extends [infer Head, ...infer Rest] ? Head : never;",
    families: [
      "indexed access",
      "tuple recursion",
      "recursive conditionals",
      "distributive conditionals",
      "inference cache/session behavior",
    ],
  },
  {
    text: "type PickValue<T, K extends keyof T> = T[K];",
    families: [
      "indexed access",
      "recursive conditionals",
      "inference cache/session behavior",
    ],
  },
  {
    text: "type Plain = { value: string };",
    families: ["unclassified"],
  },
];

for (const { text, families } of cases) {
  assert.deepEqual(semanticFamiliesForText(text), families);
}

withTempDir((dir) => {
  const assertionPath = path.join(dir, "assertions", "case.ts");
  writeFile(assertionPath, "type Remap<T> = { [K in keyof T as K]: T[K] };\n");

  const cache = new Map();
  assert.deepEqual(
    semanticFamiliesForFile("assertions\\case.ts", dir, cache),
    ["mapped/key-remapped types", "indexed access"],
  );
  assert.equal(cache.size, 1);

  assert.deepEqual(
    semanticFamiliesForFile("./assertions/case.ts", dir, cache),
    ["mapped/key-remapped types", "indexed access"],
  );
  assert.equal(cache.size, 1);

  assert.deepEqual(semanticFamiliesForFile("../case.ts", dir, cache), ["unknown"]);
  assert.deepEqual(semanticFamiliesForFile("missing.ts", dir, cache), ["unknown"]);
  assert.deepEqual(semanticFamiliesForFile("assertions", dir, cache), ["unknown"]);
  assert.deepEqual(semanticFamiliesForFile("", dir, cache), ["unknown"]);
  assert.deepEqual(semanticFamiliesForFile("assertions/case.ts", "", cache), ["unknown"]);
});
