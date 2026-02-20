#!/bin/bash
set -e

TEST_FILE="TypeScript/tests/cases/compiler/declarationEmitDestructuringObjectLiteralPattern.ts"

# Run the conformance test
/Users/mohsen/code/tsz-7/scripts/conformance.sh run --filter "${TEST_FILE}"

# Check if the declaration file exists
DTS_FILE="TypeScript/tests/cases/compiler/declarationEmitDestructuringObjectLiteralPattern.d.ts"
if [ -f "${DTS_FILE}" ]; then
  echo "Declaration file exists"
  # Check for the problematic line
  if grep -q "var {}" "${DTS_FILE}"; then
    echo "Declaration contains "var {}""
    exit 1 # Indicate failure
  else
    echo "Declaration does not contain "var {}""
    exit 0 # Indicate success
  fi
else
  echo "Declaration file does not exist"
  exit 0 # Indicate Success
fi