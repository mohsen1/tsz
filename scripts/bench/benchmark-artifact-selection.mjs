import fs from "node:fs";

export function readBenchmarkArtifact(file) {
  try {
    const data = JSON.parse(fs.readFileSync(file, "utf8"));
    return Array.isArray(data?.results) && data.results.length > 0 ? data : null;
  } catch {
    return null;
  }
}

export function benchmarkGeneratedAtMs(data) {
  const timestamp = Date.parse(data?.generated_at ?? "");
  return Number.isFinite(timestamp) ? timestamp : Number.NEGATIVE_INFINITY;
}

export function selectLatestBenchmarkArtifact(files) {
  const candidates = [];
  for (const [index, file] of files.entries()) {
    const data = readBenchmarkArtifact(file);
    if (!data) continue;
    candidates.push({
      file,
      data,
      generatedAtMs: benchmarkGeneratedAtMs(data),
      index,
    });
  }

  candidates.sort((a, b) => {
    if (a.generatedAtMs !== b.generatedAtMs) {
      return b.generatedAtMs - a.generatedAtMs;
    }
    return a.index - b.index;
  });

  const selected = candidates[0];
  return selected ? { file: selected.file, data: selected.data } : null;
}
