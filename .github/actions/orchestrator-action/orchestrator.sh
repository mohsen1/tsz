#!/bin/bash
set -euo pipefail

# AI Orchestrator - Uses Claude Code to break down and execute tasks
#
# Modes:
#   plan    - Break down a goal into tasks
#   work    - Execute a single task
#   review  - Review open PRs
#   run     - Full orchestration loop (plan → work → review → merge)

MODE="${1:-run}"
GOAL="${2:-}"
BASE_BRANCH="${3:-main}"
STATE_FILE=".github/.orchestrator-state.json"

log() { echo "[orchestrator] $*" >&2; }
error() { echo "[orchestrator] ERROR: $*" >&2; exit 1; }

# Initialize state file if needed
init_state() {
  if [[ ! -f "$STATE_FILE" ]]; then
    echo '{"goal":"","tasks":[],"completed":[],"prs":[]}' > "$STATE_FILE"
  fi
}

# Save state
save_state() {
  local goal="$1"
  local tasks="$2"
  local completed="$3"
  local prs="$4"
  jq -n \
    --arg goal "$goal" \
    --argjson tasks "$tasks" \
    --argjson completed "$completed" \
    --argjson prs "$prs" \
    '{goal: $goal, tasks: $tasks, completed: $completed, prs: $prs}' > "$STATE_FILE"
}

# Strip markdown code fences from response
strip_markdown() {
  sed 's/^```[a-z]*$//' | sed 's/^```$//' | tr '\n' '\001' | sed 's/\001\001*/\001/g' | tr '\001' '\n'
}

# Extract JSON array from text (handles multiline)
extract_json_array() {
  local input="$1"
  # Remove markdown fences and extract JSON array
  echo "$input" | sed 's/^```[a-z]*//g' | sed 's/```$//g' | tr -d '\n' | grep -o '\[.*\]'
}

# Plan: Use Claude Code to break down goal into tasks
plan() {
  local goal="$1"
  log "Planning tasks for goal: $goal"

  local prompt="Break down this goal into 2-5 independent, atomic tasks. Each task should be completable in a single PR.

Goal: $goal

Output ONLY a JSON array of task objects, no markdown fences:
[{\"id\": \"short-id\", \"title\": \"Short title\", \"description\": \"What to do\"}]"

  local response
  response=$(claude --print "$prompt" 2>/dev/null) || error "Claude Code failed"

  # Extract JSON array from response (handles markdown fences)
  local tasks
  tasks=$(extract_json_array "$response") || error "No valid JSON in response"

  # Validate JSON
  echo "$tasks" | jq empty || error "Invalid JSON: $tasks"

  log "Planned $(echo "$tasks" | jq length) tasks"
  echo "$tasks"
}

# Work: Execute a single task using Claude Code
work() {
  local task_json="$1"
  local task_id task_title task_desc
  task_id=$(echo "$task_json" | jq -r '.id')
  task_title=$(echo "$task_json" | jq -r '.title')
  task_desc=$(echo "$task_json" | jq -r '.description')

  local branch="ai/${task_id}-$(date +%s)"

  log "Working on: $task_title"
  log "Branch: $branch"

  # Create and checkout branch
  git checkout -B "$branch" "origin/$BASE_BRANCH"

  # Run Claude Code to do the actual work
  local prompt="Complete this task. Make the necessary code changes.

Task: $task_title
Details: $task_desc

Important: Only make changes directly related to this task. Keep changes minimal and focused."

  claude --print "$prompt" || {
    log "Claude Code execution completed (may have made changes or not)"
  }

  # Check for changes
  if [[ -z $(git status --porcelain) ]]; then
    log "No changes made for task: $task_title"
    git checkout "$BASE_BRANCH"
    return 1
  fi

  # Commit and push
  git add -A
  git commit -m "AI: $task_title"
  git push -u origin "$branch"

  # Create PR
  local pr_url
  pr_url=$(gh pr create \
    --title "AI: $task_title" \
    --body "## Task
$task_desc

---
*Automated by AI Orchestrator*" \
    --base "$BASE_BRANCH" \
    --head "$branch" 2>/dev/null) || error "Failed to create PR"

  log "Created PR: $pr_url"
  git checkout "$BASE_BRANCH"

  echo "$pr_url"
}

# Review: Use Claude Code to review a PR
review() {
  local pr_number="$1"

  log "Reviewing PR #$pr_number"

  # Get PR diff
  local diff
  diff=$(gh pr diff "$pr_number" 2>/dev/null) || error "Failed to get PR diff"

  if [[ -z "$diff" ]]; then
    log "No diff for PR #$pr_number"
    return 0
  fi

  local prompt="Review this PR diff. Be concise.

If the changes look good (correct, safe, follows good practices), respond with exactly: APPROVE
If there are issues, respond with: REQUEST_CHANGES followed by a brief explanation.

Diff:
$diff"

  local response
  response=$(claude --print "$prompt" 2>/dev/null) || error "Claude Code failed"

  if echo "$response" | grep -q "^APPROVE"; then
    log "Approving PR #$pr_number"
    gh pr review "$pr_number" --approve --body "Looks good! ✓" 2>/dev/null || true
    echo "approved"
  else
    local comment
    comment=$(echo "$response" | sed 's/^REQUEST_CHANGES//')
    log "Requesting changes on PR #$pr_number"
    gh pr review "$pr_number" --request-changes --body "$comment" 2>/dev/null || true
    echo "changes_requested"
  fi
}

# Merge approved PRs
merge_approved() {
  log "Checking for approved PRs to merge..."

  local prs
  prs=$(gh pr list --state open --json number,reviews --jq '.[] | select(.reviews | map(select(.state == "APPROVED")) | length > 0) | .number')

  for pr in $prs; do
    log "Merging PR #$pr"
    gh pr merge "$pr" --squash --delete-branch 2>/dev/null || log "Could not merge PR #$pr"
  done
}

# Full orchestration loop
run_orchestrator() {
  local goal="$1"
  [[ -z "$goal" ]] && error "Goal is required"

  init_state

  log "=== Starting orchestration ==="
  log "Goal: $goal"
  log "Base branch: $BASE_BRANCH"

  # Phase 1: Plan
  log "=== Phase 1: Planning ==="
  local tasks
  tasks=$(plan "$goal")

  save_state "$goal" "$tasks" "[]" "[]"

  # Phase 2: Execute each task
  log "=== Phase 2: Executing tasks ==="
  local completed="[]"
  local prs="[]"
  local task_count
  task_count=$(echo "$tasks" | jq length)

  for i in $(seq 0 $((task_count - 1))); do
    local task
    task=$(echo "$tasks" | jq ".[$i]")
    local task_id
    task_id=$(echo "$task" | jq -r '.id')

    log "--- Task $((i + 1))/$task_count: $task_id ---"

    local pr_url
    if pr_url=$(work "$task"); then
      prs=$(echo "$prs" | jq --arg url "$pr_url" '. + [$url]')
      completed=$(echo "$completed" | jq --arg id "$task_id" '. + [$id]')
    fi

    save_state "$goal" "$tasks" "$completed" "$prs"
  done

  # Phase 3: Review PRs
  log "=== Phase 3: Reviewing PRs ==="
  local open_prs
  open_prs=$(gh pr list --state open --json number --jq '.[].number')

  for pr in $open_prs; do
    review "$pr"
  done

  # Phase 4: Merge approved PRs
  log "=== Phase 4: Merging ==="
  merge_approved

  log "=== Orchestration complete ==="
  log "Tasks completed: $(echo "$completed" | jq length)/$task_count"
}

# Main
case "$MODE" in
  plan)
    [[ -z "$GOAL" ]] && error "Goal required for plan mode"
    plan "$GOAL"
    ;;
  work)
    [[ -z "$GOAL" ]] && error "Task JSON required for work mode"
    work "$GOAL"
    ;;
  review)
    [[ -z "$GOAL" ]] && error "PR number required for review mode"
    review "$GOAL"
    ;;
  merge)
    merge_approved
    ;;
  run)
    [[ -z "$GOAL" ]] && error "Goal required for run mode"
    run_orchestrator "$GOAL"
    ;;
  *)
    error "Unknown mode: $MODE. Use: plan, work, review, merge, or run"
    ;;
esac
