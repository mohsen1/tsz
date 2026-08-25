#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";

function hashFile(aggregate, file, prefix) {
  if (file.length === 0) throw new Error("source-tree listing contains an empty path");
  const fileDigest = crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
  const relative = file.subarray(0, prefix.length).equals(prefix)
    ? file.subarray(prefix.length)
    : file;
  aggregate.update(fileDigest, "ascii");
  aggregate.update("  ", "ascii");
  aggregate.update(relative);
  aggregate.update("\n", "ascii");
}

async function sourceTreeHash(tree, listing) {
  const aggregate = crypto.createHash("sha256");
  const prefix = Buffer.from(tree.endsWith("/") ? tree : `${tree}/`);
  let pending = Buffer.alloc(0);

  for await (const chunk of fs.createReadStream(listing)) {
    const input = pending.length === 0 ? chunk : Buffer.concat([pending, chunk]);
    let start = 0;
    for (let newline = input.indexOf(0x0a, start); newline !== -1; newline = input.indexOf(0x0a, start)) {
      hashFile(aggregate, input.subarray(start, newline), prefix);
      start = newline + 1;
    }
    pending = Buffer.from(input.subarray(start));
  }
  if (pending.length !== 0) hashFile(aggregate, pending, prefix);
  return aggregate.digest("hex");
}

const [tree, listing] = process.argv.slice(2);
if (!tree || !listing) {
  console.error("usage: project-source-tree-hash.mjs <tree> <sorted-listing>");
  process.exitCode = 2;
} else {
  sourceTreeHash(tree, listing)
    .then((digest) => process.stdout.write(digest))
    .catch((error) => {
      console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
    });
}
