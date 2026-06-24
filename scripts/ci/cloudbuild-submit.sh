#!/usr/bin/env bash
# Submit a Cloud Build job with bounded retry/backoff around *submission*
# failures only — never around a genuine build verdict.
#
# Why: the "Submit ... to Cloud Build pool" steps in ci.yml intermittently end
# with the GitHub work step `conclusion: null` because the submission is
# interrupted before the build runs (Cloud Build pool/staging hiccup, transient
# transport error). That fails the job → fails CI Summary → trips the main-red
# sentinel for a commit that has no real regression (issue #14688).
#
# Safety invariant: once `gcloud builds submit` has *created* a build (the build
# actually ran), its non-zero exit is a real verdict on the tree and MUST NOT be
# retried — retrying would both mask a genuine regression and burn another full
# build. We therefore retry ONLY when the attempt failed without ever creating a
# build (i.e. the submission itself was interrupted). The "Created [.../builds/
# <id>]" marker that gcloud prints once the build exists is the discriminator.
#
# Usage:
#   scripts/ci/cloudbuild-submit.sh <gcloud builds submit args...>
#
# Env knobs:
#   CLOUDBUILD_SUBMIT_ATTEMPTS   max attempts (default 3)
#   CLOUDBUILD_SUBMIT_BACKOFF    base backoff seconds, doubled each retry (default 15)
#   CLOUDBUILD_SUBMIT_TIMEOUT    per-attempt wall timeout passed to `timeout` (default 45m)
set -euo pipefail

attempts="${CLOUDBUILD_SUBMIT_ATTEMPTS:-3}"
backoff="${CLOUDBUILD_SUBMIT_BACKOFF:-15}"
per_attempt_timeout="${CLOUDBUILD_SUBMIT_TIMEOUT:-45m}"

# A build was created (and therefore ran) if gcloud printed the canonical
# "Created [https://cloudbuild.googleapis.com/.../builds/<id>]" line. Once that
# is present, the exit code is a build verdict, not a submission failure.
build_created_marker='Created \[https://cloudbuild\.googleapis\.com/.*/builds/'

log="$(mktemp)"
trap 'rm -f "${log}"' EXIT

attempt=1
while :; do
  echo "::group::Cloud Build submit (attempt ${attempt}/${attempts})"
  # Tee so the full gcloud output still streams to the CI log while we inspect
  # it for the build-created marker. PIPESTATUS[0] is gcloud's real exit code;
  # capture it on the very next line (errexit off so a failing attempt does not
  # abort before we can classify and retry it).
  submit=(gcloud builds submit "$@")
  if [[ -n "${per_attempt_timeout}" ]]; then
    submit=(timeout "${per_attempt_timeout}" "${submit[@]}")
  fi
  set +e
  "${submit[@]}" 2>&1 | tee "${log}"
  status="${PIPESTATUS[0]}"
  set -e
  echo "::endgroup::"

  if [[ "${status}" -eq 0 ]]; then
    exit 0
  fi

  # If a build was created, the failure is a real verdict — do not retry.
  if grep -Eq "${build_created_marker}" "${log}"; then
    echo "Cloud Build verdict is a real build failure (build was created); not retrying." >&2
    exit "${status}"
  fi

  if [[ "${attempt}" -ge "${attempts}" ]]; then
    echo "Cloud Build submission failed before creating a build after ${attempts} attempt(s)." >&2
    exit "${status}"
  fi

  delay=$(( backoff * (2 ** (attempt - 1)) ))
  echo "Submission interrupted before a build was created (exit ${status}); retrying in ${delay}s." >&2
  sleep "${delay}"
  attempt=$(( attempt + 1 ))
done
