export function benchReadinessMessages(readiness, winnerReport = null) {
  if (readiness?.artifact_absent) {
    return [
      "No recent benchmark artifact - compatibility data shown from repository snapshot and may be stale.",
    ];
  }

  const messages = [];
  if (readiness) {
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
  }

  const target = winnerReport?.two_x_target;
  const measurementWarning = winnerReport?.measurement_profile?.warning;
  if (typeof measurementWarning === "string" && measurementWarning.length > 0) {
    messages.push(
      `Benchmark companion report measurement profile warning: ${measurementWarning}; 2x target evidence may not be comparable.`,
    );
  }

  const targetGaps = Number(target?.rows_below_target ?? 0);
  if (targetGaps > 0) {
    const eligibleRows = Number(target?.eligible_green_rows ?? 0);
    const denominator = eligibleRows > 0 ? `/${eligibleRows}` : "";
    messages.push(
      `Benchmark companion report has ${targetGaps}${denominator} green row(s) below the 2x tsgo target; public speed claims are not launch-ready.`,
    );
  }

  const missingAttribution = Array.isArray(target?.missing_attribution_rows)
    ? target.missing_attribution_rows.length
    : 0;
  if (missingAttribution > 0) {
    messages.push(
      `Benchmark companion report is missing attribution for ${missingAttribution} 2x target gap row(s).`,
    );
  }

  return messages;
}
