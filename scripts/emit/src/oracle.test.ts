import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { CliTranspiler } from './cli-transpiler.js';
import { resolvePinnedOracle, sha256Directory, sha256File, verifyOracleExecutable } from './oracle.js';

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tsz-oracle-verification-'));
const fakeOracle = path.join(tempDir, 'fake-tsc.mjs');

try {
  fs.writeFileSync(fakeOracle, `#!/usr/bin/env node
if (process.argv[2] === '--version') console.log('Version 7.0.2');
else process.exit(1);
`, 'utf8');
  fs.chmodSync(fakeOracle, 0o755);
  const expectedHash = sha256File(fakeOracle);
  assert.deepEqual(
    verifyOracleExecutable(fakeOracle, 'Version 7.0.2', expectedHash),
    { versionOutput: 'Version 7.0.2', sha256: expectedHash },
  );
  assert.throws(
    () => verifyOracleExecutable(path.join(tempDir, 'missing-tsc'), 'Version 7.0.2'),
    /PINNED_TS7_ORACLE_MISSING/,
  );
  assert.throws(
    () => verifyOracleExecutable(fakeOracle, 'Version 7.0.3', expectedHash),
    /PINNED_TS7_ORACLE_MISMATCH: binary --version/,
  );
  assert.throws(
    () => verifyOracleExecutable(fakeOracle, 'Version 7.0.2', '0'.repeat(64)),
    /PINNED_TS7_ORACLE_MISMATCH: binary sha256/,
  );

  const __dirname = path.dirname(fileURLToPath(import.meta.url));
  const repoRoot = path.resolve(__dirname, '../../..');
  assert.throws(
    () => resolvePinnedOracle(path.join(tempDir, 'missing-root')),
    /PINNED_TS7_ORACLE_MISSING/,
  );
  const wrongRoot = path.join(tempDir, 'wrong-version-root');
  fs.mkdirSync(path.join(wrongRoot, 'scripts/emit'), { recursive: true });
  fs.symlinkSync(path.join(repoRoot, 'scripts/node_modules'), path.join(wrongRoot, 'scripts/node_modules'), 'dir');
  const manifest = JSON.parse(fs.readFileSync(path.join(repoRoot, 'scripts/emit/oracle-manifest.json'), 'utf8'));
  manifest.version = '7.0.3';
  fs.writeFileSync(
    path.join(wrongRoot, 'scripts/emit/oracle-manifest.json'),
    JSON.stringify(manifest),
    'utf8',
  );
  assert.throws(
    () => resolvePinnedOracle(wrongRoot),
    /PINNED_TS7_ORACLE_MISMATCH: wrapper package version/,
  );

  const pinned = resolvePinnedOracle();
  assert.equal(pinned.provenance.version, '7.0.2');
  assert.equal(pinned.provenance.binarySha256, sha256File(pinned.binaryPath));
  assert.equal(
    pinned.provenance.platformPackageTreeSha256,
    sha256Directory(path.dirname(path.dirname(pinned.binaryPath))),
  );
  assert.match(pinned.provenance.fingerprint, /^sha256:[0-9a-f]{64}$/);

  const pinnedCli = new CliTranspiler(5_000, {
    binaryPath: pinned.binaryPath,
    label: 'pinned-typescript-7-allow-js-witness',
  });
  const jsInput = {
    sourceFiles: [{ name: 'input.js', content: 'module.exports = 1;\n' }],
    outDir: 'out',
  };
  try {
    const absent = await pinnedCli.transpile('', undefined, undefined, jsInput);
    assert.equal(absent.outcome.exitCode, 2);
    assert.equal(absent.outcome.diagnosticCodes.includes('TS6504'), true);
    assert.deepEqual(absent.jsProducts, [], 'absent allowJs stays absent and emits no product');

    const enabled = await pinnedCli.transpile('', undefined, undefined, { ...jsInput, allowJs: true });
    assert.equal(enabled.outcome.exitCode, 0);
    assert.deepEqual(enabled.jsProducts, [
      { path: 'out/input.js', content: '"use strict";\nmodule.exports = 1;\n' },
    ]);

    const disabled = await pinnedCli.transpile('', undefined, undefined, { ...jsInput, allowJs: false });
    assert.equal(disabled.outcome.exitCode, 2);
    assert.equal(disabled.outcome.diagnosticCodes.includes('TS6504'), true);
    assert.deepEqual(disabled.jsProducts, [], 'explicit false remains distinct from explicit true');
  } finally {
    pinnedCli.terminate();
  }
} finally {
  fs.rmSync(tempDir, { recursive: true, force: true });
}

console.log('emit-oracle: provenance and authored allowJs witnesses fail closed');
