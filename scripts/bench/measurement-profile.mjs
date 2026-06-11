export function measurementProfileStatus(artifact) {
  const profile = artifact?.measurement_profile;
  if (!profile || typeof profile !== "object") {
    return {
      present: false,
      mode: null,
      tsz_binary_source: null,
      pgo_requested: null,
      pgo_required: null,
      pgo_optimized: null,
      profile_fingerprint: null,
      training_fingerprint: null,
      rust_target_cpu: null,
      training_input_count: null,
      training_failure_count: null,
      warning: "measurement_profile missing",
    };
  }

  const mode = typeof profile.mode === "string" && profile.mode.trim()
    ? profile.mode.trim()
    : null;
  const pgo = profile.profile_guided_optimization && typeof profile.profile_guided_optimization === "object"
    ? profile.profile_guided_optimization
    : {};
  const warning = (() => {
    if (!mode) return "measurement_profile.mode missing";
    if (mode === "release-pgo") {
      const missing = [];
      if (pgo.optimized !== true) missing.push("pgo optimized flag");
      if (!pgo.profile_fingerprint) missing.push("profile fingerprint");
      if (!pgo.training_fingerprint) missing.push("training fingerprint");
      if (missing.length > 0) return `release-pgo metadata missing ${missing.join(", ")}`;
    }
    return null;
  })();

  return {
    present: true,
    mode,
    tsz_binary_source: profile.tsz_binary_source ?? null,
    pgo_requested: pgo.requested ?? null,
    pgo_required: pgo.required ?? null,
    pgo_optimized: pgo.optimized ?? null,
    profile_fingerprint: pgo.profile_fingerprint ?? null,
    training_fingerprint: pgo.training_fingerprint ?? null,
    rust_target_cpu: pgo.rust_target_cpu ?? null,
    training_input_count: pgo.training_input_count ?? null,
    training_failure_count: pgo.training_failure_count ?? null,
    warning,
  };
}
