import assert from 'node:assert/strict';
import * as path from 'node:path';
import {
  corpusPhysicalPath,
  corpusRelativePath,
  isStagedInputPath,
  physicalDirectoryCompilerOptions,
  physicalPathIdentity,
  resolveDirectoryCompilerOptions,
} from './harness-config.js';

const directJsOptions = resolveDirectoryCompilerOptions({
  variant: {},
  directives: { outdir: './out' },
  embeddedConfig: {},
});
assert.equal(directJsOptions.outDir, './out', 'argumentsPropertyNameInJsMode1 direct @outDir');

const conditionalPackageOptions = resolveDirectoryCompilerOptions({
  variant: {},
  directives: { outdir: 'out', rootdir: '.' },
  embeddedConfig: {},
});
assert.deepEqual(
  conditionalPackageOptions,
  { outDir: 'out', rootDir: '.' },
  'nodeModulesAllowJsConditionalPackageExports direct directory options',
);

const precedenceOptions = resolveDirectoryCompilerOptions({
  variant: { outdir: 'variant-out' },
  directives: { outdir: 'directive-out', declarationdir: '/pkg/types', rootdir: '/pkg/src' },
  embeddedConfig: { outDir: 'config-out', declarationDir: 'config-types', rootDir: 'config-src' },
});
assert.deepEqual(precedenceOptions, {
  outDir: 'variant-out',
  declarationDir: '/pkg/types',
  rootDir: '/pkg/src',
});
assert.deepEqual(
  resolveDirectoryCompilerOptions({
    variant: {},
    directives: {},
    embeddedConfig: { outDir: 'config-out', declarationDir: 'config-types', rootDir: 'config-src' },
  }),
  { outDir: 'config-out', declarationDir: 'config-types', rootDir: 'config-src' },
  'embedded config is the fallback after variant and direct options',
);

const stagedRoot = path.resolve('/tmp/tsz-emit-witness');
const physicalStagedRoot = physicalPathIdentity(stagedRoot);
assert.equal(corpusRelativePath('/pkg/src/index.ts'), 'pkg/src/index.ts');
assert.equal(corpusPhysicalPath(physicalStagedRoot, '/pkg/dist'), path.join(physicalStagedRoot, 'pkg/dist'));
assert.equal(corpusPhysicalPath(physicalStagedRoot, '/pkg/src'), path.join(physicalStagedRoot, 'pkg/src'));
assert.equal(corpusPhysicalPath(physicalStagedRoot, 'dist'), path.join(physicalStagedRoot, 'dist'));
assert.equal(corpusPhysicalPath(physicalStagedRoot, '../dist'), path.join(physicalStagedRoot, '../dist'));
assert.equal(corpusPhysicalPath(physicalStagedRoot, 'A:/'), path.join(physicalStagedRoot, 'A:'));
const nodeNextOptions = resolveDirectoryCompilerOptions({
  variant: {},
  directives: { outdir: '/pkg/dist', declarationdir: '/pkg/types', rootdir: '/pkg/src' },
  embeddedConfig: {},
});
assert.deepEqual(
  physicalDirectoryCompilerOptions(physicalStagedRoot, nodeNextOptions),
  {
    outDir: path.join(physicalStagedRoot, 'pkg/dist'),
    declarationDir: path.join(physicalStagedRoot, 'pkg/types'),
    rootDir: path.join(physicalStagedRoot, 'pkg/src'),
  },
  'nodeNextPackageSelfNameWithOutDirDeclDirRootDir physical CLI options',
);

const stagedMjs = corpusPhysicalPath(physicalStagedRoot, '/e.mjs');
const inputPathSet = new Set([physicalPathIdentity(stagedMjs)]);
assert.equal(
  isStagedInputPath(inputPathSet, corpusPhysicalPath(physicalStagedRoot, '/e.mjs')),
  true,
  'impliedNodeFormatEmit1 staged MJS input is not emit output',
);
assert.equal(
  isStagedInputPath(inputPathSet, corpusPhysicalPath(physicalStagedRoot, 'dist/e.mjs')),
  false,
  'impliedNodeFormatEmit1 distinct outDir product remains collectable',
);

console.log('emit-harness-config: option and path witnesses passed');
