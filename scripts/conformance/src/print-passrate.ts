#!/usr/bin/env node

import figlet from 'figlet';

const rate = process.argv[2];

if (!rate) {
  console.error('Usage: print-passrate.ts <rate>');
  process.exit(1);
}

// Color based on pass rate
const numVal = parseInt(rate.split('.')[0]);
let colorCode = 32; // green
if (numVal < 95) colorCode = 33; // yellow
if (numVal < 80) colorCode = 31; // red

const ANSI_COLOR = `\x1b[${colorCode}m`;
const ANSI_RESET = '\x1b[0m';

figlet.text(`${rate}%`, {
  font: 'Standard',
  width: 200,
  whitespaceBreak: true
}, (err, data) => {
  if (err || !data) {
    console.log(`${rate}%`);
    return;
  }

  // Colorize each line
  const colored = data
    .split('\n')
    .map(line => `${ANSI_COLOR}${line}${ANSI_RESET}`)
    .join('\n');

  console.log('\n' + colored + '\n');
});
