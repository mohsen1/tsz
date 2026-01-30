const ts = require('typescript');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const testDir = '/home/user/tsz/TypeScript/tests/cases/conformance/async';
const tszBin = '/home/user/tsz/.target/release/tsz';

const tests = [
  'es5/asyncSetter_es5.ts', 'es5/asyncEnum_es5.ts', 'es5/asyncInterface_es5.ts',
  'es5/asyncModule_es5.ts', 'es5/asyncClass_es5.ts',
  'es6/asyncSetter_es6.ts', 'es6/asyncEnum_es6.ts', 'es6/asyncInterface_es6.ts',
  'es6/asyncModule_es6.ts', 'es6/asyncClass_es6.ts'
];

function getTargetEnum(target) {
  switch ((target || 'es5').toLowerCase()) {
    case 'es5': return ts.ScriptTarget.ES5;
    case 'es6': case 'es2015': return ts.ScriptTarget.ES2015;
    default: return ts.ScriptTarget.ES2020;
  }
}

for (const t of tests) {
  const filePath = path.join(testDir, t);
  const content = fs.readFileSync(filePath, 'utf8');

  // Parse directives
  const lines = content.split('\n');
  const opts = {};
  for (const line of lines) {
    const m = line.match(/^\/\/\s*@(\w+):\s*(.*)/);
    if (m) opts[m[1]] = m[2].trim();
  }

  // Strip directives
  const code = lines.filter(l => !l.match(/^\/\/\s*@/)).join('\n');

  // Compile with tsc
  const target = getTargetEnum(opts.target);
  const strictVal = opts.strict === 'false' ? false : true;
  const compilerOptions = { target, noEmit: true, strict: strictVal };
  const host = ts.createCompilerHost(compilerOptions);
  const origGetSF = host.getSourceFile;
  host.getSourceFile = (fn, lv) => {
    if (fn === 'input.ts') return ts.createSourceFile('input.ts', code, lv, true);
    return origGetSF.call(host, fn, lv);
  };
  const program = ts.createProgram(['input.ts'], compilerOptions, host);
  const diags = [...program.getSyntacticDiagnostics(), ...program.getSemanticDiagnostics()];
  const tscCodes = diags.map(d => d.code).sort((a,b) => a - b);

  // Run tsz
  let tszCodes = [];
  try {
    const out = execSync(tszBin + ' ' + filePath + ' 2>&1', { encoding: 'utf8' });
    const matches = [...out.matchAll(/error TS(\d+)/g)];
    tszCodes = matches.map(m => parseInt(m[1])).sort((a,b) => a - b);
  } catch (e) {
    const out = (e.stdout || '') + (e.stderr || '');
    const matches = [...out.matchAll(/error TS(\d+)/g)];
    tszCodes = matches.map(m => parseInt(m[1])).sort((a,b) => a - b);
  }

  const tscSet = new Set(tscCodes);
  const tszSet = new Set(tszCodes);
  const extra = tszCodes.filter(c => !tscSet.has(c));
  const missing = tscCodes.filter(c => !tszSet.has(c));

  const match = extra.length === 0 && missing.length === 0 ? 'MATCH' : 'DIFFER';
  console.log(t + ': ' + match);
  console.log('  tsc: [' + [...new Set(tscCodes)].map(c => 'TS' + c).join(', ') + '] (' + tscCodes.length + ' total)');
  console.log('  tsz: [' + [...new Set(tszCodes)].map(c => 'TS' + c).join(', ') + '] (' + tszCodes.length + ' total)');
  if (extra.length > 0) console.log('  EXTRA in tsz: ' + extra.map(c => 'TS' + c).join(', '));
  if (missing.length > 0) console.log('  MISSING in tsz: ' + missing.map(c => 'TS' + c).join(', '));
  console.log('');
}
