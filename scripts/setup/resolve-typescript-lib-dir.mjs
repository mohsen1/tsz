#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

function readPackageJson(packageJsonPath) {
  const text = fs.readFileSync(packageJsonPath, "utf8");
  return JSON.parse(text);
}

function isFile(file) {
  try {
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}

function hasCompiledStandardLibs(libDir) {
  return isFile(path.join(libDir, "lib.d.ts"))
    && isFile(path.join(libDir, "lib.es5.d.ts"));
}

function assertCompiledStandardLibs(libDir, label) {
  if (!hasCompiledStandardLibs(libDir)) {
    throw new Error(
      `${label} does not contain both lib.d.ts and lib.es5.d.ts: ${libDir}`,
    );
  }
}

export function resolveTypeScriptLibDir(
  wrapperPackageJson,
  { platform = process.platform, arch = process.arch } = {},
) {
  const packageJsonPath = path.resolve(wrapperPackageJson);
  if (!isFile(packageJsonPath)) {
    throw new Error(`TypeScript package.json not found: ${packageJsonPath}`);
  }

  const wrapper = readPackageJson(packageJsonPath);
  const wrapperRoot = path.dirname(packageJsonPath);
  const wrapperLib = path.join(wrapperRoot, "lib");

  // TypeScript <=6 ships the standard libraries in the wrapper package.
  if (hasCompiledStandardLibs(wrapperLib)) {
    return fs.realpathSync(wrapperLib);
  }

  const platformPackageBase = `typescript-${platform}-${arch}`;
  const sourceBuildLib = path.resolve(
    wrapperRoot,
    "..",
    `${platformPackageBase}`,
    "lib",
  );
  if (hasCompiledStandardLibs(sourceBuildLib)) {
    const sourceBuildPackageJson = path.join(path.dirname(sourceBuildLib), "package.json");
    if (isFile(sourceBuildPackageJson)) {
      const sourceBuildPackage = readPackageJson(sourceBuildPackageJson);
      if (sourceBuildPackage.version !== wrapper.version) {
        throw new Error(
          `TypeScript wrapper/source-build version mismatch: ${wrapper.version ?? "<missing>"} `
            + `!= ${sourceBuildPackage.version ?? "<missing>"} (${sourceBuildPackageJson})`,
        );
      }
    }
    return fs.realpathSync(sourceBuildLib);
  }

  const platformPackageName = `@typescript/${platformPackageBase}`;
  let platformPackageJson;
  try {
    const wrapperRequire = createRequire(packageJsonPath);
    platformPackageJson = wrapperRequire.resolve(`${platformPackageName}/package.json`);
  } catch (error) {
    throw new Error(
      `Unable to resolve ${platformPackageName} from ${packageJsonPath}. `
        + "TypeScript 7 requires its platform optional dependency; reinstall without --omit=optional. "
        + `(${error.message})`,
    );
  }

  const platformPackage = readPackageJson(platformPackageJson);
  if (platformPackage.version !== wrapper.version) {
    throw new Error(
      `TypeScript wrapper/platform version mismatch: ${wrapper.version ?? "<missing>"} `
        + `!= ${platformPackage.version ?? "<missing>"} (${platformPackageName})`,
    );
  }

  const platformLib = path.join(path.dirname(platformPackageJson), "lib");
  assertCompiledStandardLibs(platformLib, platformPackageName);
  return fs.realpathSync(platformLib);
}

function usage() {
  console.error(
    "Usage: resolve-typescript-lib-dir.mjs <typescript-package.json> [--platform NAME] [--arch NAME]",
  );
}

function main(argv) {
  let packageJson = "";
  let platform = process.platform;
  let arch = process.arch;

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--platform") {
      platform = argv[index + 1] ?? "";
      index += 1;
    } else if (arg === "--arch") {
      arch = argv[index + 1] ?? "";
      index += 1;
    } else if (!packageJson && !arg.startsWith("-")) {
      packageJson = arg;
    } else {
      usage();
      throw new Error(`Unknown or duplicate argument: ${arg}`);
    }
  }

  if (!packageJson || !platform || !arch) {
    usage();
    process.exitCode = 2;
    return;
  }

  console.log(resolveTypeScriptLibDir(packageJson, { platform, arch }));
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    console.error(`resolve-typescript-lib-dir: ${error.message}`);
    process.exitCode = 1;
  }
}
