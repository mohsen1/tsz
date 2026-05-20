const fs = require("fs");
const path = require("path");

function loadTypescript() {
  const cwd = process.cwd();
  const candidates = [
    path.join(cwd, "TypeScript", "node_modules", "typescript"),
    "typescript",
  ];
  for (const candidate of candidates) {
    try {
      return require(candidate);
    } catch {}
  }
  throw new Error("Cannot load TypeScript runtime");
}

function normalizePath(fileName) {
  return typeof fileName === "string" ? fileName.replace(/\\\\/g, "/") : fileName;
}

function getLineStarts(text) {
  const starts = [0];
  for (let i = 0; i < text.length; i++) {
    if (text.charCodeAt(i) === 10) starts.push(i + 1);
  }
  return starts;
}

function offsetToLocation(lineStarts, offset) {
  const bounded = Math.max(0, Math.min(offset, lineStarts[lineStarts.length - 1] + 10 ** 9));
  let low = 0;
  let high = lineStarts.length - 1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    const start = lineStarts[mid];
    const next = mid + 1 < lineStarts.length ? lineStarts[mid + 1] : Number.MAX_SAFE_INTEGER;
    if (bounded < start) {
      high = mid - 1;
    } else if (bounded >= next) {
      low = mid + 1;
    } else {
      return { line: mid + 1, offset: bounded - start + 1 };
    }
  }
  const line = Math.max(0, Math.min(low, lineStarts.length - 1));
  return { line: line + 1, offset: bounded - lineStarts[line] + 1 };
}

function locationToOffset(fileText, line, offset) {
  const lineStarts = getLineStarts(fileText);
  const lineIndex = Math.max(0, Math.min((line || 1) - 1, lineStarts.length - 1));
  const base = lineStarts[lineIndex];
  return base + Math.max(0, (offset || 1) - 1);
}

function nodeModulesPackageRoot(fileName) {
  const normalized = normalizePath(fileName || "");
  const marker = "/node_modules/";
  const idx = normalized.lastIndexOf(marker);
  if (idx < 0) return "";
  const rest = normalized.slice(idx + marker.length);
  if (!rest) return "";
  const parts = rest.split("/").filter(Boolean);
  if (parts.length === 0) return "";
  const pkgParts = parts[0].startsWith("@") && parts.length > 1
    ? [parts[0], parts[1]]
    : [parts[0]];
  return normalized.slice(0, idx + marker.length) + pkgParts.join("/");
}

function nodeModulesRenameError(requestedFile, definitionFiles) {
  const currentRoot = nodeModulesPackageRoot(requestedFile);
  for (const defFile of definitionFiles) {
    const defRoot = nodeModulesPackageRoot(defFile);
    if (!defRoot) continue;
    if (currentRoot && currentRoot !== defRoot) {
      return "You cannot rename elements that are defined in another 'node_modules' folder.";
    }
  }
  return "You cannot rename elements that are defined in a 'node_modules' folder.";
}

const TSZ_PERSISTENT_MODE = process.env.TSZ_NATIVE_TS_PERSISTENT === "1";

function runOne(ts, input) {
  const op = String(input.op || "");
  const requestedFile = normalizePath(input.file || "");
  const inputOpenFiles = input.openFiles && typeof input.openFiles === "object" ? input.openFiles : {};
  const files = {};
  for (const [key, value] of Object.entries(inputOpenFiles)) {
    files[normalizePath(key)] = String(value ?? "");
  }
  return runWithFiles(ts, input, op, requestedFile, files);
}

function run() {
  const ts = loadTypescript();
  if (TSZ_PERSISTENT_MODE) {
    // Persistent-worker protocol: newline-delimited JSON requests on
    // stdin, newline-delimited JSON responses on stdout. The TypeScript
    // module is loaded once and reused for every request, which turns
    // the per-call cost from 1–2 s (fresh module load) into tens of ms.
    let buffer = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      buffer += chunk;
      let nl;
      while ((nl = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, nl);
        buffer = buffer.slice(nl + 1);
        let result;
        try {
          const input = line ? JSON.parse(line) : {};
          result = runOne(ts, input);
        } catch (err) {
          result = { __error: String(err && err.message ? err.message : err) };
        }
        try {
          process.stdout.write(JSON.stringify(result === undefined ? null : result) + "\n");
        } catch {
          // Broken pipe — parent exited.
        }
      }
    });
    process.stdin.on("end", () => process.exit(0));
    return;
  }

  const raw = fs.readFileSync(0, "utf8");
  const input = raw ? JSON.parse(raw) : {};
  process.stdout.write(JSON.stringify(runOne(ts, input)));
}

function runWithFiles(ts, input, op, requestedFile, files) {

  function ensureFileText(fileName) {
    const normalized = normalizePath(fileName);
    if (!normalized) return "";
    if (Object.prototype.hasOwnProperty.call(files, normalized)) {
      return files[normalized];
    }
    try {
      const text = fs.readFileSync(normalized, "utf8");
      files[normalized] = text;
      return text;
    } catch {
      return "";
    }
  }

  if (requestedFile && !Object.prototype.hasOwnProperty.call(files, requestedFile)) {
    ensureFileText(requestedFile);
  }

  const realpathAliases = new Map();

  function packageInfoFromPath(fileName) {
    const normalized = normalizePath(fileName || "");
    const prefix = "/packages/";
    if (!normalized.startsWith(prefix)) return null;
    const rest = normalized.slice(prefix.length);
    const segments = rest.split("/").filter(Boolean);
    if (segments.length < 2) return null;
    let packageName = segments[0];
    let restStart = 1;
    if (packageName.startsWith("@")) {
      if (segments.length < 3) return null;
      packageName = `${segments[0]}/${segments[1]}`;
      restStart = 2;
    }
    const packageRelativePath = segments.slice(restStart).join("/");
    if (!packageRelativePath) return null;
    return { packageName, packageRelativePath };
  }

  function packageNameFromSpecifier(specifier) {
    const text = String(specifier || "").trim();
    if (!text || text.startsWith(".") || text.startsWith("/")) return null;
    const parts = text.split("/").filter(Boolean);
    if (parts.length === 0) return null;
    if (parts[0].startsWith("@") && parts.length > 1) {
      return `${parts[0]}/${parts[1]}`;
    }
    return parts[0];
  }

  function barePackageSpecifiers(fileText) {
    const source = String(fileText || "");
    const out = new Set();
    const patterns = [
      /from\s+["']([^"']+)["']/g,
      /require\(\s*["']([^"']+)["']\s*\)/g,
      /import\(\s*["']([^"']+)["']\s*\)/g,
    ];
    for (const pattern of patterns) {
      let match;
      while ((match = pattern.exec(source)) !== null) {
        const packageName = packageNameFromSpecifier(match[1]);
        if (packageName) out.add(packageName);
      }
    }
    return out;
  }

  const packageFiles = new Map();
  for (const [fileName, fileText] of Object.entries(files)) {
    const info = packageInfoFromPath(fileName);
    if (!info) continue;
    if (!packageFiles.has(info.packageName)) {
      packageFiles.set(info.packageName, []);
    }
    packageFiles.get(info.packageName).push({
      sourcePath: normalizePath(fileName),
      packageRelativePath: info.packageRelativePath,
      content: String(fileText ?? ""),
    });
  }

  for (const [consumerFile, fileText] of Object.entries(files)) {
    const consumerInfo = packageInfoFromPath(consumerFile);
    if (!consumerInfo) continue;
    const consumerRoot = `/packages/${consumerInfo.packageName}`;
    const specifiers = barePackageSpecifiers(fileText);
    for (const packageName of specifiers) {
      const entries = packageFiles.get(packageName);
      if (!entries || entries.length === 0) continue;
      for (const entry of entries) {
        const linkedPath = normalizePath(
          `${consumerRoot}/node_modules/${packageName}/${entry.packageRelativePath}`,
        );
        if (!Object.prototype.hasOwnProperty.call(files, linkedPath)) {
          files[linkedPath] = entry.content;
        }
        realpathAliases.set(linkedPath, entry.sourcePath);
      }
    }
  }

  function isScriptFile(fileName) {
    return /\.(d\.ts|ts|tsx|js|jsx|mts|cts)$/i.test(fileName || "");
  }

  const scriptFileNames = Object.keys(files).filter(isScriptFile);
  if (requestedFile && isScriptFile(requestedFile) && !scriptFileNames.includes(requestedFile)) {
    scriptFileNames.push(requestedFile);
  }

  const virtualFiles = new Set(Object.keys(files).map(normalizePath));
  const virtualDirs = new Set();

  function addVirtualDir(dirName) {
    if (!dirName) return;
    virtualDirs.add(normalizePath(dirName));
  }

  function addVirtualPath(fileName) {
    const normalized = normalizePath(fileName);
    if (!normalized) return;
    let current = path.posix.dirname(normalized);
    while (current && current !== "." && current !== "/") {
      addVirtualDir(current);
      const parent = path.posix.dirname(current);
      if (!parent || parent === current) break;
      current = parent;
    }
    if (current === "/") addVirtualDir("/");
  }

  for (const fileName of virtualFiles) {
    addVirtualPath(fileName);
  }

  function virtualDirectoryExists(dirName) {
    const normalized = normalizePath(dirName || "");
    if (!normalized) return false;
    if (virtualDirs.has(normalized)) return true;
    const prefix = normalized.endsWith("/") ? normalized : `${normalized}/`;
    for (const fileName of virtualFiles) {
      if (fileName.startsWith(prefix)) return true;
    }
    return false;
  }

  function virtualDirectoriesOf(dirName) {
    const normalized = normalizePath(dirName || "");
    if (!normalized) return [];
    const out = new Set();
    for (const dir of virtualDirs) {
      if (dir === normalized) continue;
      if (path.posix.dirname(dir) === normalized) {
        out.add(path.posix.basename(dir));
      }
    }
    return Array.from(out.values());
  }

  function virtualReadDirectory(rootDir, extensions) {
    const normalizedRoot = normalizePath(rootDir || "");
    if (!normalizedRoot) return [];
    const prefix = normalizedRoot.endsWith("/") ? normalizedRoot : `${normalizedRoot}/`;
    const extensionList = Array.isArray(extensions) ? extensions : [];
    const out = new Set();
    for (const fileName of virtualFiles) {
      if (fileName !== normalizedRoot && !fileName.startsWith(prefix)) continue;
      if (
        extensionList.length > 0
        && !extensionList.some(ext => fileName.toLowerCase().endsWith(String(ext || "").toLowerCase()))
      ) {
        continue;
      }
      out.add(fileName);
    }
    return Array.from(out.values());
  }

  const fullProgramOps = new Set(["rename", "encodedSemanticClassifications"]);
  const compilerOptions = fullProgramOps.has(op)
    ? {
        allowJs: true,
        checkJs: true,
        target: ts.ScriptTarget.Latest,
        module: ts.ModuleKind.CommonJS,
        moduleResolution: ts.ModuleResolutionKind.NodeJs,
        jsx: ts.JsxEmit.Preserve,
        allowImportingTsExtensions: true,
      }
    : {
        allowJs: true,
        checkJs: false,
        target: ts.ScriptTarget.Latest,
        module: ts.ModuleKind.ESNext,
        jsx: ts.JsxEmit.Preserve,
        noResolve: true,
        noLib: true,
        allowNonTsExtensions: true,
      };

  const versions = new Map(scriptFileNames.map(file => [file, "1"]));

  const host = {
    getCompilationSettings: () => compilerOptions,
    getCurrentDirectory: () => "/",
    getDefaultLibFileName: options => ts.getDefaultLibFilePath(options),
    getScriptFileNames: () => scriptFileNames,
    getScriptVersion: fileName => versions.get(normalizePath(fileName)) || "1",
    getScriptSnapshot: fileName => {
      const text = ensureFileText(fileName);
      if (text === "") {
        if (ts.sys.fileExists(fileName)) {
          const fromFs = ts.sys.readFile(fileName);
          return fromFs !== undefined ? ts.ScriptSnapshot.fromString(fromFs) : undefined;
        }
        return undefined;
      }
      return ts.ScriptSnapshot.fromString(text);
    },
    readFile: fileName => {
      const normalized = normalizePath(fileName);
      if (Object.prototype.hasOwnProperty.call(files, normalized)) {
        return files[normalized];
      }
      return ts.sys.readFile(fileName);
    },
    fileExists: fileName => {
      const normalized = normalizePath(fileName);
      return Object.prototype.hasOwnProperty.call(files, normalized) || ts.sys.fileExists(fileName);
    },
    readDirectory: (rootDir, extensions, excludes, includes, depth) => {
      const diskEntries = ts.sys.readDirectory
        ? ts.sys.readDirectory(rootDir, extensions, excludes, includes, depth)
        : [];
      const merged = new Set((diskEntries || []).map(normalizePath));
      for (const virtualEntry of virtualReadDirectory(rootDir, extensions)) {
        merged.add(virtualEntry);
      }
      return Array.from(merged.values());
    },
    directoryExists: fileName => {
      if (virtualDirectoryExists(fileName)) return true;
      return ts.sys.directoryExists ? ts.sys.directoryExists(fileName) : false;
    },
    getDirectories: dirName => {
      const diskDirs = ts.sys.getDirectories ? ts.sys.getDirectories(dirName) : [];
      const merged = new Set(diskDirs || []);
      for (const virtualDir of virtualDirectoriesOf(dirName)) {
        merged.add(virtualDir);
      }
      return Array.from(merged.values());
    },
    realpath: fileName => {
      const normalized = normalizePath(fileName);
      if (realpathAliases.has(normalized)) {
        return realpathAliases.get(normalized);
      }
      if (Object.prototype.hasOwnProperty.call(files, normalized) || virtualDirectoryExists(normalized)) {
        return normalized;
      }
      return ts.sys.realpath ? ts.sys.realpath(fileName) : fileName;
    },
  };

  const ls = ts.createLanguageService(host, ts.createDocumentRegistry());

  function spanToProtocol(fileName, span) {
    const text = ensureFileText(fileName);
    const lineStarts = getLineStarts(text);
    const start = span?.start || 0;
    const length = span?.length || 0;
    return {
      start: offsetToLocation(lineStarts, start),
      end: offsetToLocation(lineStarts, start + length),
    };
  }

  function navTreeToProtocol(fileName, item) {
    return {
      text: item.text,
      kind: item.kind,
      kindModifiers: item.kindModifiers || "",
      spans: (item.spans || []).map(span => spanToProtocol(fileName, span)),
      nameSpan: item.nameSpan ? spanToProtocol(fileName, item.nameSpan) : undefined,
      childItems: item.childItems && item.childItems.length
        ? item.childItems.map(child => navTreeToProtocol(fileName, child))
        : undefined,
    };
  }

  function navBarToProtocol(fileName, item) {
    if (!item || typeof item !== "object") {
      return {
        text: "",
        kind: "",
        kindModifiers: "",
        spans: [],
        indent: 0,
      };
    }
    if (!globalThis.__tszNavBarSeen) {
      globalThis.__tszNavBarSeen = new WeakSet();
    }
    const seen = globalThis.__tszNavBarSeen;
    if (seen.has(item)) {
      return {
        text: item.text,
        kind: item.kind,
        kindModifiers: item.kindModifiers || "",
        spans: (item.spans || []).map(span => spanToProtocol(fileName, span)),
        indent: item.indent || 0,
      };
    }
    seen.add(item);
    return {
      text: item.text,
      kind: item.kind,
      kindModifiers: item.kindModifiers || "",
      spans: (item.spans || []).map(span => spanToProtocol(fileName, span)),
      childItems: item.childItems && item.childItems.length
        ? item.childItems.map(child => navBarToProtocol(fileName, child))
        : undefined,
      indent: item.indent || 0,
    };
  }

  let result = null;
  switch (op) {
    case "navtree": {
      const tree = ls.getNavigationTree(requestedFile);
      result = tree ? navTreeToProtocol(requestedFile, tree) : null;
      break;
    }
    case "navbar": {
      const items = ls.getNavigationBarItems(requestedFile) || [];
      result = items.map(item => navBarToProtocol(requestedFile, item));
      break;
    }
    case "navto": {
      const searchValue = String(input.searchValue || "");
      const items = ls.getNavigateToItems(searchValue) || [];
      result = items.map(item => {
        const fileName = normalizePath(item.fileName || requestedFile);
        const span = spanToProtocol(fileName, item.textSpan || { start: 0, length: 0 });
        return {
          name: item.name,
          kind: item.kind,
          matchKind: item.matchKind,
          isCaseSensitive: !!item.isCaseSensitive,
          kindModifiers: item.kindModifiers || "",
          containerName: item.containerName || "",
          containerKind: item.containerKind || "",
          file: fileName,
          start: span.start,
          end: span.end,
        };
      });
      break;
    }
    case "rename": {
      const text = ensureFileText(requestedFile);
      const line = Number(input.line) || 1;
      const offset = Number(input.offset) || 1;
      const position = locationToOffset(text, line, offset);
      const findInStrings = !!input.findInStrings;
      const findInComments = !!input.findInComments;
      const preferences =
        input.preferences && typeof input.preferences === "object"
          ? { ...input.preferences }
          : {};
      if (
        preferences.providePrefixAndSuffixTextForRename === undefined
        && input.providePrefixAndSuffixTextForRename !== undefined
        && input.providePrefixAndSuffixTextForRename !== null
      ) {
        preferences.providePrefixAndSuffixTextForRename = !!input.providePrefixAndSuffixTextForRename;
      }
      if (
        preferences.allowRenameOfImportPath === undefined
        && input.allowRenameOfImportPath !== undefined
        && input.allowRenameOfImportPath !== null
      ) {
        preferences.allowRenameOfImportPath = !!input.allowRenameOfImportPath;
      }
      if (preferences.allowRenameOfImportPath === undefined) {
        preferences.allowRenameOfImportPath = true;
      }

      const renameInfo = ls.getRenameInfo(
        requestedFile,
        position,
        preferences,
        findInStrings,
        findInComments,
      );

      const definitions = ls.getDefinitionAtPosition(requestedFile, position) || [];
      const definitionFiles = definitions
        .map(def => normalizePath(def && def.fileName ? def.fileName : ""))
        .filter(Boolean);
      const hasNodeModulesDefinition = definitionFiles.some(fileName =>
        fileName.includes("/node_modules/")
      );

      if (!renameInfo || !renameInfo.canRename) {
        let message =
          (renameInfo && renameInfo.localizedErrorMessage) || "You cannot rename this element.";
        if (message === "You cannot rename this element." && hasNodeModulesDefinition) {
          message = nodeModulesRenameError(requestedFile, definitionFiles);
        }
        result = {
          info: {
            canRename: false,
            localizedErrorMessage: message,
          },
          locs: [],
        };
        break;
      }

      if (
        preferences
        && preferences.allowRenameOfImportPath === false
        && renameInfo.fileToRename !== undefined
      ) {
        result = {
          info: {
            canRename: false,
            localizedErrorMessage: "You cannot rename this element.",
          },
          locs: [],
        };
        break;
      }

      const locations =
        ls.findRenameLocations(
          requestedFile,
          position,
          findInStrings,
          findInComments,
          preferences,
        ) || [];

      const grouped = new Map();
      for (const loc of locations) {
        const fileName = normalizePath(loc.fileName || requestedFile);
        const span = spanToProtocol(fileName, loc.textSpan || { start: 0, length: 0 });
        const entry = { start: span.start, end: span.end };
        if (loc.contextSpan) {
          const contextSpan = spanToProtocol(fileName, loc.contextSpan);
          entry.contextStart = contextSpan.start;
          entry.contextEnd = contextSpan.end;
        }
        if (loc.prefixText !== undefined) entry.prefixText = loc.prefixText;
        if (loc.suffixText !== undefined) entry.suffixText = loc.suffixText;
        const current = grouped.get(fileName);
        if (current) current.push(entry);
        else grouped.set(fileName, [entry]);
      }

      const triggerSpan = renameInfo.triggerSpan || { start: position, length: 0 };
      const trigger = spanToProtocol(requestedFile, triggerSpan);
      const info = {
        canRename: true,
        displayName: renameInfo.displayName,
        fullDisplayName: renameInfo.fullDisplayName,
        kind: renameInfo.kind,
        kindModifiers: renameInfo.kindModifiers || "",
        triggerSpan: {
          start: trigger.start,
          length: triggerSpan.length || 0,
        },
      };
      if (renameInfo.fileToRename !== undefined) {
        info.fileToRename = normalizePath(renameInfo.fileToRename);
      }

      const locs = Array.from(grouped.entries()).map(([fileName, fileLocs]) => ({
        file: fileName,
        locs: fileLocs,
      }));

      result = { info, locs };
      break;
    }
    case "format": {
      const options = input.options && typeof input.options === "object" ? input.options : {};
      const text = ensureFileText(requestedFile);
      const startLine = Number(input.line);
      const startOffset = Number(input.offset);
      const endLine = Number(input.endLine);
      const endOffset = Number(input.endOffset);
      const hasRange =
        Number.isFinite(startLine)
        && Number.isFinite(startOffset)
        && Number.isFinite(endLine)
        && Number.isFinite(endOffset);
      const edits = hasRange
        ? (ls.getFormattingEditsForRange(
            requestedFile,
            Math.min(
              locationToOffset(text, startLine, startOffset),
              locationToOffset(text, endLine, endOffset),
            ),
            Math.max(
              locationToOffset(text, startLine, startOffset),
              locationToOffset(text, endLine, endOffset),
            ),
            options,
          ) || [])
        : (ls.getFormattingEditsForDocument(requestedFile, options) || []);
      result = edits.map(edit => {
        const span = spanToProtocol(requestedFile, edit.span || { start: 0, length: 0 });
        return { start: span.start, end: span.end, newText: edit.newText || "" };
      });
      break;
    }
    case "formatOnKey": {
      const options = input.options && typeof input.options === "object" ? input.options : {};
      const text = ensureFileText(requestedFile);
      const position = locationToOffset(text, Number(input.line) || 1, Number(input.offset) || 1);
      const key = String(input.key || "");
      const edits = ls.getFormattingEditsAfterKeystroke(requestedFile, position, key, options) || [];
      result = edits.map(edit => {
        const span = spanToProtocol(requestedFile, edit.span || { start: 0, length: 0 });
        return { start: span.start, end: span.end, newText: edit.newText || "" };
      });
      break;
    }
    case "encodedSemanticClassifications": {
      const text = ensureFileText(requestedFile);
      const rawStart = Number(input.start);
      const start = Number.isFinite(rawStart) && rawStart >= 0 ? rawStart : 0;
      const rawLength = Number(input.length);
      const length = Number.isFinite(rawLength) && rawLength > 0 ? rawLength : text.length;
      const rawFormat = input.format;
      const format = rawFormat === "2020"
        || rawFormat === "1"
        || rawFormat === 2020
        || rawFormat === 1
        ? ts.SemanticClassificationFormat.TwentyTwenty
        : ts.SemanticClassificationFormat.Original;
      result = ls.getEncodedSemanticClassifications(
        requestedFile,
        { start, length },
        format,
      );
      break;
    }
    default:
      result = null;
      break;
  }

  return result;
}

try {
  run();
} catch (err) {
  process.stdout.write(JSON.stringify({ __error: String(err && err.message ? err.message : err) }));
}
