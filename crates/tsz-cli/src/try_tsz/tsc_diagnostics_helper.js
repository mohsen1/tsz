const path = require("node:path");
const { createRequire } = require("node:module");

const MIN_TYPESCRIPT_VERSION = "6.0.3";
const configPath = path.resolve(process.argv[1]);
const projectRoot = path.dirname(configPath);
const projectRequire = createRequire(path.join(projectRoot, "package.json"));

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
    candidates.push({
      label: packageJson,
      require: createRequire(packageJson),
    });
  }
  candidates.push({
    label: path.join(projectRoot, "package.json"),
    require: projectRequire,
  });
  return candidates;
}

function loadTypescript() {
  const rejected = [];
  const seen = new Set();
  for (const candidate of candidateRequires()) {
    if (seen.has(candidate.label)) continue;
    seen.add(candidate.label);
    try {
      const ts = candidate.require("typescript");
      if (versionAtLeast(ts.version, MIN_TYPESCRIPT_VERSION)) {
        return ts;
      }
      rejected.push(`${candidate.label} has TypeScript ${ts.version}`);
    } catch (error) {
      rejected.push(
        `${candidate.label} could not load TypeScript: ${error.message}`,
      );
    }
  }
  const details = rejected.join("; ");
  throw new Error(
    `try-tsz needs TypeScript ${MIN_TYPESCRIPT_VERSION} or newer for the tsc oracle. ${details}`,
  );
}

let ts;
try {
  ts = loadTypescript();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}

function flattenMessage(messageText) {
  return ts.flattenDiagnosticMessageText(messageText, "\n");
}

function categoryName(category) {
  switch (category) {
    case ts.DiagnosticCategory.Warning:
      return "warning";
    case ts.DiagnosticCategory.Error:
      return "error";
    case ts.DiagnosticCategory.Suggestion:
      return "suggestion";
    case ts.DiagnosticCategory.Message:
      return "message";
    default:
      return "message";
  }
}

function toComparableDiagnostic(diagnostic) {
  let file = null;
  let line = null;
  let column = null;
  if (diagnostic.file) {
    file = diagnostic.file.fileName;
    if (typeof diagnostic.start === "number") {
      const pos = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start);
      line = pos.line + 1;
      column = pos.character + 1;
    }
  }
  return {
    file,
    start: typeof diagnostic.start === "number" ? diagnostic.start : null,
    length: typeof diagnostic.length === "number" ? diagnostic.length : null,
    line,
    column,
    code: diagnostic.code,
    category: categoryName(diagnostic.category),
    message: flattenMessage(diagnostic.messageText),
  };
}

function hasConfigDeprecationDiagnostic(diagnostics) {
  return diagnostics.some(
    (diagnostic) => diagnostic.code === 5101 || diagnostic.code === 5107,
  );
}

const config = ts.readConfigFile(configPath, ts.sys.readFile);
let diagnostics = [];
if (config.error) {
  diagnostics.push(config.error);
} else {
  const parsed = ts.parseJsonConfigFileContent(
    config.config,
    ts.sys,
    projectRoot,
    { noEmit: true },
    configPath,
  );
  diagnostics.push(...parsed.errors);
  if (parsed.errors.length === 0) {
    const program = ts.createProgram({
      rootNames: parsed.fileNames,
      options: { ...parsed.options, noEmit: true },
      projectReferences: parsed.projectReferences,
    });
    const optionsDiagnostics = program.getOptionsDiagnostics();
    if (hasConfigDeprecationDiagnostic(optionsDiagnostics)) {
      diagnostics.push(...optionsDiagnostics);
    } else {
      diagnostics.push(...ts.getPreEmitDiagnostics(program));
    }
  }
}

process.stdout.write(JSON.stringify({
  typescript_version: ts.version,
  diagnostics: diagnostics.map(toComparableDiagnostic),
}));
