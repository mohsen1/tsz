export function benchReadinessMessages(readiness) {
  if (!readiness) return [];
  if (readiness.artifact_absent) {
    return [
      "No recent benchmark artifact - compatibility data shown from repository snapshot and may be stale.",
    ];
  }

  const messages = [];
  const missing = Number(readiness.missing);
  if (missing > 0) {
    messages.push(
      `Benchmark artifact is missing ${missing} required row(s); shown data may be incomplete.`,
    );
  }

  const duplicateRows = Array.isArray(readiness.duplicate_rows)
    ? readiness.duplicate_rows
    : [];
  if (duplicateRows.length > 0) {
    messages.push(
      `Benchmark artifact has ${duplicateRows.length} duplicate required row(s); shown data may be unreliable.`,
    );
  }

  if (readiness.source_freshness?.current === false) {
    const warning = readiness.source_freshness.warning
      ? `: ${readiness.source_freshness.warning}`
      : "";
    messages.push(
      `Benchmark artifact source is stale${warning}; shown data is not current release truth.`,
    );
  }

  const metadataWarnings = Number(readiness.metadata_warnings_total ?? 0);
  if (readiness.metadata_clean === false || metadataWarnings > 0) {
    const count = metadataWarnings > 0 ? `${metadataWarnings} ` : "";
    messages.push(
      `Benchmark artifact metadata has ${count}warning(s); shown data may not be comparable.`,
    );
  }

  return messages;
}
