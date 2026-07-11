import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { resolveTypeScriptLibDir } from "./resolve-typescript-lib-dir.mjs";

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value)}\n`);
}

function writeCompiledLibs(libDir) {
  fs.mkdirSync(libDir, { recursive: true });
  fs.writeFileSync(path.join(libDir, "lib.d.ts"), "/// <reference lib=\"es5\" />\n");
  fs.writeFileSync(path.join(libDir, "lib.es5.d.ts"), "interface Array<T> {}\n");
}

function withTempFixture(callback) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-typescript-lib-resolver-"));
  try {
    callback(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function writeWrapper(root, version = "7.0.2") {
  const packageJson = path.join(root, "node_modules", "typescript", "package.json");
  writeJson(packageJson, { name: "typescript", version });
  fs.mkdirSync(path.join(path.dirname(packageJson), "lib"), { recursive: true });
  return packageJson;
}

function writePlatformPackage(root, platform, arch, version = "7.0.2") {
  const packageRoot = path.join(
    root,
    "node_modules",
    "@typescript",
    `typescript-${platform}-${arch}`,
  );
  writeJson(path.join(packageRoot, "package.json"), {
    name: `@typescript/typescript-${platform}-${arch}`,
    version,
  });
  writeCompiledLibs(path.join(packageRoot, "lib"));
  return path.join(packageRoot, "lib");
}

test("resolves legacy wrapper-owned libraries", () => {
  withTempFixture((root) => {
    const wrapperPackageJson = writeWrapper(root, "6.0.3");
    const expected = path.join(root, "node_modules", "typescript", "lib");
    writeCompiledLibs(expected);
    assert.equal(resolveTypeScriptLibDir(wrapperPackageJson), fs.realpathSync(expected));
  });
});

for (const [platform, arch] of [
  ["darwin", "arm64"],
  ["linux", "x64"],
  ["win32", "arm64"],
]) {
  test(`resolves ${platform}-${arch} TypeScript 7 platform libraries`, () => {
    withTempFixture((root) => {
      const wrapperPackageJson = writeWrapper(root);
      const expected = writePlatformPackage(root, platform, arch);
      assert.equal(
        resolveTypeScriptLibDir(wrapperPackageJson, { platform, arch }),
        fs.realpathSync(expected),
      );
    });
  });
}

test("rejects mismatched wrapper and platform versions", () => {
  withTempFixture((root) => {
    const wrapperPackageJson = writeWrapper(root);
    writePlatformPackage(root, "linux", "x64", "7.0.1");
    assert.throws(
      () => resolveTypeScriptLibDir(wrapperPackageJson, { platform: "linux", arch: "x64" }),
      /wrapper\/platform version mismatch/,
    );
  });
});

test("rejects launcher-only wrapper directories", () => {
  withTempFixture((root) => {
    const wrapperPackageJson = writeWrapper(root);
    assert.throws(
      () => resolveTypeScriptLibDir(wrapperPackageJson, { platform: "linux", arch: "x64" }),
      /requires its platform optional dependency/,
    );
  });
});

test("resolves a typescript-go built/npm platform sibling", () => {
  withTempFixture((root) => {
    const wrapperPackageJson = path.join(root, "built", "npm", "typescript", "package.json");
    writeJson(wrapperPackageJson, { name: "typescript", version: "7.0.2" });
    const platformRoot = path.join(root, "built", "npm", "typescript-linux-x64");
    writeJson(path.join(platformRoot, "package.json"), {
      name: "@typescript/typescript-linux-x64",
      version: "7.0.2",
    });
    writeCompiledLibs(path.join(platformRoot, "lib"));
    assert.equal(
      resolveTypeScriptLibDir(wrapperPackageJson, { platform: "linux", arch: "x64" }),
      fs.realpathSync(path.join(platformRoot, "lib")),
    );
  });
});
