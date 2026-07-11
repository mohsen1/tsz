const fs = require("node:fs");
const path = require("node:path");
const { createRequire } = require("node:module");
const { pathToFileURL } = require("node:url");

const MIN_TYPESCRIPT_VERSION = "7.0.2";
const configPath = path.resolve(process.argv[1]);
const projectRoot = path.dirname(configPath);

function versionParts(version) {
  return version
    .split(".")
    .map((part) => Number.parseInt(part, 10))
    .map((part) => (Number.isFinite(part) ? part : 0));
}

function versionAtLeast(version, minimum) {
  const actual = versionParts(version);
  const required = versionParts(minimum);
  for (let index = 0; index < required.length; index += 1) {
    const left = actual[index] ?? 0;
    const right = required[index] ?? 0;
    if (left > right) return true;
    if (left < right) return false;
  }
  return true;
}

function candidateRequires() {
  const candidates = [];
  const packageJson = process.env.TRY_TSZ_TYPESCRIPT_PACKAGE_JSON;
  if (packageJson) {
    candidates.push({ label: packageJson, require: createRequire(packageJson) });
  }
  const projectPackageJson = path.join(projectRoot, "package.json");
  candidates.push({
    label: projectPackageJson,
    require: createRequire(projectPackageJson),
  });
  return candidates;
}

async function loadOracle() {
  const rejected = [];
  const seen = new Set();
  for (const candidate of candidateRequires()) {
    if (seen.has(candidate.label)) continue;
    seen.add(candidate.label);
    try {
      const version = candidate.require("typescript").version;
      if (!versionAtLeast(version, MIN_TYPESCRIPT_VERSION)) {
        rejected.push(`${candidate.label} has TypeScript ${version}`);
        continue;
      }
      const apiPath = candidate.require.resolve("typescript/unstable/sync");
      const apiModule = await import(pathToFileURL(apiPath).href);
      const jsonc = candidate.require("jsonc-parser");
      return { ...apiModule, jsonc, version };
    } catch (error) {
      rejected.push(
        `${candidate.label} could not load the TypeScript 7 API: ${error.message}`,
      );
    }
  }
  throw new Error(
    `try-tsz needs TypeScript ${MIN_TYPESCRIPT_VERSION} or newer for the tsc oracle. ${rejected.join("; ")}`,
  );
}

function objectProperties(node) {
  if (!node || node.type !== "object") return [];
  return node.children ?? [];
}

function propertyName(property) {
  return property?.children?.[0]?.value;
}

function createNoEmitOverlay(text, jsonc) {
  const errors = [];
  const root = jsonc.parseTree(text, errors, {
    allowTrailingComma: true,
    disallowComments: false,
  });
  if (errors.length !== 0 || !root || root.type !== "object") {
    throw new Error("try-tsz could not safely parse the root tsconfig JSONC");
  }

  const compilerProperties = objectProperties(root).filter(
    (property) => propertyName(property) === "compilerOptions",
  );
  if (compilerProperties.length > 1) {
    throw new Error("try-tsz refuses duplicate compilerOptions properties");
  }
  if (compilerProperties.length === 1) {
    const compilerOptions = compilerProperties[0].children?.[1];
    if (!compilerOptions || compilerOptions.type !== "object") {
      throw new Error("try-tsz requires compilerOptions to be an object");
    }
    const noEmitProperties = objectProperties(compilerOptions).filter(
      (property) => propertyName(property) === "noEmit",
    );
    if (noEmitProperties.length > 1) {
      throw new Error("try-tsz refuses duplicate compilerOptions.noEmit properties");
    }
    if (noEmitProperties.length === 1) {
      const noEmit = noEmitProperties[0].children?.[1];
      if (!noEmit || noEmit.type !== "boolean") {
        throw new Error("try-tsz requires compilerOptions.noEmit to be boolean");
      }
      if (noEmit.value === true) return { text, edits: [] };
    }
  }

  const edits = jsonc.modify(text, ["compilerOptions", "noEmit"], true, {
    formattingOptions: {
      insertSpaces: !/^\t/m.test(text),
      tabSize: 2,
      eol: text.includes("\r\n") ? "\r\n" : "\n",
    },
  });
  return { text: jsonc.applyEdits(text, edits), edits };
}

function inverseOverlayOffset(offset, edits) {
  let mapped = offset;
  for (const edit of [...edits].sort((a, b) => a.offset - b.offset)) {
    const overlayEnd = edit.offset + edit.content.length;
    if (mapped < edit.offset) continue;
    if (mapped <= overlayEnd) {
      mapped = edit.offset + Math.min(mapped - edit.offset, edit.length);
    } else {
      mapped += edit.length - edit.content.length;
    }
  }
  return mapped;
}

function flattenMessage(diagnostic) {
  const lines = [diagnostic.text];
  for (const child of diagnostic.messageChain ?? []) {
    lines.push(flattenMessage(child));
  }
  return lines.filter(Boolean).join("\n");
}

function categoryName(category, DiagnosticCategory) {
  switch (category) {
    case DiagnosticCategory.Warning:
      return "warning";
    case DiagnosticCategory.Error:
      return "error";
    case DiagnosticCategory.Suggestion:
      return "suggestion";
    case DiagnosticCategory.Message:
      return "message";
    default:
      return "message";
  }
}

function lineAndColumn(text, offset) {
  const before = text.slice(0, offset);
  const line = before.split(/\r\n|\r|\n/).length;
  const lastLf = Math.max(before.lastIndexOf("\n"), before.lastIndexOf("\r"));
  return { line, column: offset - lastLf };
}

function utf16ToUtf8Offset(text, offset) {
  return Buffer.byteLength(text.slice(0, offset), "utf8");
}

function collectDiagnostics(project, DiagnosticCategory) {
  const program = project.program;
  const diagnostics = [...program.getConfigFileParsingDiagnostics()];
  const syntactic = [...program.getSyntacticDiagnostics()];
  diagnostics.push(...syntactic);
  if (syntactic.length !== 0) return diagnostics;

  const programDiagnostics = [...program.getProgramDiagnostics()];
  diagnostics.push(...programDiagnostics);
  program.getBindDiagnostics();
  const globalDiagnostics = [...program.getGlobalDiagnostics()];
  diagnostics.push(...globalDiagnostics);
  if (
    !programDiagnostics.some((diagnostic) => diagnostic.category === DiagnosticCategory.Error) &&
    !globalDiagnostics.some((diagnostic) => diagnostic.category === DiagnosticCategory.Error)
  ) {
    diagnostics.push(...program.getSemanticDiagnostics());
    diagnostics.push(...program.getGlobalDiagnostics());
  }
  const options = project.compilerOptions;
  if (
    !diagnostics.some((diagnostic) => diagnostic.category === DiagnosticCategory.Error) &&
    options.noEmit &&
    (options.declaration || options.composite)
  ) {
    diagnostics.push(...program.getDeclarationDiagnostics());
  }
  const seen = new Set();
  return diagnostics.filter((diagnostic) => {
    const key = [
      diagnostic.fileName ?? "",
      diagnostic.pos,
      diagnostic.end,
      diagnostic.code,
      diagnostic.category,
      flattenMessage(diagnostic),
    ].join("\u0000");
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

async function main() {
  const oracle = await loadOracle();
  const originalConfig = fs.readFileSync(configPath, "utf8");
  const overlay = createNoEmitOverlay(originalConfig, oracle.jsonc);
  const api = new oracle.API({
    cwd: projectRoot,
    fs: {
      readFile(fileName) {
        return path.resolve(fileName) === configPath ? overlay.text : undefined;
      },
    },
  });
  let snapshot;
  try {
    snapshot = api.updateSnapshot({ openProjects: [configPath] });
    const project =
      snapshot.getProject(configPath) ??
      snapshot
        .getProjects()
        .find((candidate) => path.resolve(candidate.configFileName) === configPath);
    if (!project) throw new Error(`TypeScript 7 did not load ${configPath}`);

    const sourceCache = new Map();
    const diagnostics = collectDiagnostics(project, oracle.DiagnosticCategory).map(
      (diagnostic) => {
        const file = diagnostic.fileName ? path.resolve(diagnostic.fileName) : null;
        let start16 = Number.isFinite(diagnostic.pos) ? diagnostic.pos : null;
        let end16 = Number.isFinite(diagnostic.end) ? diagnostic.end : start16;
        let source = null;
        if (file) {
          if (!sourceCache.has(file)) {
            let source = null;
            try {
              source = file === configPath ? originalConfig : fs.readFileSync(file, "utf8");
            } catch {}
            sourceCache.set(file, source);
          }
          source = sourceCache.get(file);
          if (file === configPath && start16 !== null) {
            start16 = inverseOverlayOffset(start16, overlay.edits);
            end16 = inverseOverlayOffset(end16, overlay.edits);
          }
        }
        const location = source && start16 !== null ? lineAndColumn(source, start16) : {};
        const start = source && start16 !== null ? utf16ToUtf8Offset(source, start16) : null;
        const end = source && end16 !== null ? utf16ToUtf8Offset(source, end16) : start;
        return {
          file,
          start,
          length: start !== null && end !== null ? Math.max(0, end - start) : null,
          line: location.line ?? null,
          column: location.column ?? null,
          code: diagnostic.code,
          category: categoryName(diagnostic.category, oracle.DiagnosticCategory),
          message: flattenMessage(diagnostic),
        };
      },
    );
    process.stdout.write(
      JSON.stringify({ typescript_version: oracle.version, diagnostics }),
    );
  } finally {
    snapshot?.dispose();
    api.close();
  }
}

main().catch((error) => {
  console.error(error.stack ?? error.message);
  process.exit(1);
});
