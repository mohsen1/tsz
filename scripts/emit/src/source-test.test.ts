import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { parseSourceTest, readTypeScriptTestFile, selectHarnessRootFiles } from './source-test.js';

const exactSingleFile = [
  '// @target: es2015\r\n',
  '// @strict: false\r\n',
  '\r\n',
  '  /* leading comment */\r\n',
  'value\r\n',
  '(function () {})() // ASI-sensitive continuation  \r\n',
  '// @internal\r\n',
  '  ',
].join('');
const parsedSingleFile = parseSourceTest(exactSingleFile, 'case.ts');
assert.equal(parsedSingleFile.options.target, 'es2015');
assert.equal(parsedSingleFile.options.strict, false);
assert.equal(
  parsedSingleFile.sourceFiles[0].content,
  '\r\n  /* leading comment */\r\nvalue\r\n(function () {})() // ASI-sensitive continuation  \r\n// @internal\r\n  ',
  'header removal preserves leading/trailing whitespace, comments, ASI-sensitive text, and line endings',
);

const virtual = parseSourceTest(
  '// @filename: /src/a.ts\r\n  export const a = 1;  \r\n// @filename: /src/b.ts\n\nexport {}',
);
assert.deepEqual(virtual.sourceFiles, [
  { name: '/src/a.ts', content: '  export const a = 1;  \r\n' },
  { name: '/src/b.ts', content: '\nexport {}' },
]);
assert.deepEqual(
  selectHarnessRootFiles(virtual.sourceFiles, true),
  { rootFileNames: ['/src/b.ts'], reason: 'last-unit-no-implicit-references' },
  'noImplicitReferences stages every unit but passes only the last harness unit as a root',
);
assert.deepEqual(
  selectHarnessRootFiles(virtual.sourceFiles, false),
  { rootFileNames: ['/src/a.ts', '/src/b.ts'], reason: 'all-units' },
);

const discoveryRoots = selectHarnessRootFiles([
  { name: 'library.ts', content: 'export const value = 1;\n' },
  { name: 'entry.ts', content: 'const value = require("./library");\n' },
], false);
assert.deepEqual(discoveryRoots, { rootFileNames: ['entry.ts'], reason: 'last-unit-discovery' });

const configRoots = selectHarnessRootFiles([
  { name: 'tsconfig.json', content: '{"compilerOptions":{}}' },
  { name: 'entry.ts', content: 'export {};' },
], false);
assert.equal(configRoots.unsupportedReason, 'embedded-tsconfig-root-selection-not-modeled:tsconfig.json');

const unknownHeaderFlag = parseSourceTest('// @futureCompilerFlag\nconst value = 1;\n', 'flag.ts');
assert.equal(
  unknownHeaderFlag.options.futurecompilerflag,
  true,
  'a parsed header flag remains available for explicit unhandled-option accounting',
);

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tsz-emit-source-read-'));
try {
  const livePath = path.join(tempDir, 'tests/cases/case.ts');
  fs.mkdirSync(path.dirname(livePath), { recursive: true });
  fs.writeFileSync(livePath, exactSingleFile);
  assert.equal(await readTypeScriptTestFile(tempDir, 'tests/cases/case.ts'), exactSingleFile);
  await assert.rejects(
    readTypeScriptTestFile(tempDir, 'tests/cases/missing.ts'),
    (error: unknown) => (error as NodeJS.ErrnoException).code === 'ENOENT',
    'a missing live corpus source is terminal and has no revision fallback',
  );
} finally {
  fs.rmSync(tempDir, { recursive: true, force: true });
}

console.log('emit-source-test: staged source bytes and live-read failures are exact');
