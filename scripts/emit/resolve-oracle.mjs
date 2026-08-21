#!/usr/bin/env node

import path from 'node:path';
import process from 'node:process';
import { resolvePinnedOracle } from './src/oracle.ts';

const args = process.argv.slice(2);
if (args.length !== 2 || args[0] !== '--root' || !args[1]) {
  throw new Error('usage: resolve-oracle.mjs --root <repository-root>');
}

const oracle = resolvePinnedOracle(path.resolve(args[1]));
process.stdout.write(`${JSON.stringify(oracle)}\n`);
