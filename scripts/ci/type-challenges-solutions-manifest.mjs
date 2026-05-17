#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [tsvPath, manifestPath] = process.argv.slice(2);

if (!tsvPath || !manifestPath) {
  console.error(
    "usage: type-challenges-solutions-manifest.mjs <entries.tsv> <manifest.json>",
  );
  process.exit(2);
}

const repository = process.env.TYPE_CHALLENGES_SOLUTIONS_REPO;
const ref = process.env.TYPE_CHALLENGES_SOLUTIONS_REF;
const expectedGenerated = Number(
  process.env.TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED,
);

if (!repository || !ref || !Number.isInteger(expectedGenerated)) {
  console.error(
    "error: missing Type Challenges solutions repository, ref, or expected count",
  );
  process.exit(1);
}

const manifestDir = path.dirname(manifestPath);
const lines = fs.readFileSync(tsvPath, "utf8").trimEnd().split(/\r?\n/);
const header = lines.shift();

if (header !== "output\tsource\tid\tlevel\ttitle") {
  console.error(`error: unexpected manifest TSV header: ${header ?? "<empty>"}`);
  process.exit(1);
}

const entries = lines
  .filter((line) => line.length > 0)
  .map((line, index) => {
    const [output, source, id, level, ...titleParts] = line.split("\t");
    const title = titleParts.join("\t");

    if (!output || !source || !id || !level || !title) {
      console.error(`error: incomplete manifest row ${index + 2}: ${line}`);
      process.exit(1);
    }

    const outputPath = path.join(manifestDir, output);
    if (!fs.existsSync(outputPath)) {
      console.error(`error: manifest output does not exist: ${output}`);
      process.exit(1);
    }

    return {
      output,
      source,
      challenge: {
        id,
        level,
        title,
      },
    };
  });

if (entries.length !== expectedGenerated) {
  console.error(
    `error: manifest has ${entries.length} entries; expected ${expectedGenerated} for ${ref}`,
  );
  process.exit(1);
}

const manifest = {
  fixture: "type-challenges-solutions-project",
  source: {
    repository,
    ref,
    path: "en/*.md",
  },
  expectedGenerated,
  generated: entries.length,
  entries,
};

fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
