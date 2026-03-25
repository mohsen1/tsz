# Stuck Process Cleanup Prompt

Use this prompt with Claude Code to safely clean up hung processes without killing active work.

```
Find and kill stuck tsz-related processes. Follow these rules carefully:

1. IDENTIFY candidates — run `ps -eo pid,etime,pcpu,rss,command | grep -E '(tsz|cargo|rustc)' | grep -v grep` and categorize:
   - `tsz --batch` processes: these are conformance worker children. If running >30 minutes, they are hung (normal runs finish in seconds).
   - `tsz-conformance` processes: parent conformance runners. If running >60 minutes, likely stuck.
   - `tsz` (standalone, not --batch): type-checker invocations. If running >15 minutes, likely infinite-looping on a recursive type.
   - `rustc` processes: compilation. If a SINGLE crate has been compiling >90 minutes, it's stuck. Fresh builds (<10 min) are legitimate. Release builds of tsz_checker/tsz_solver may take 5-10 min normally.
   - `cargo` processes: build orchestrators. Only kill if their child rustc processes are all stuck.
   - `samply`/profiler processes: if running >2 hours, the profilee is stuck — kill both.
   - `git index-pack`: if running >30 minutes, stuck.
   - `tail -f` on agent output files: if running >24 hours, stale.

2. PROTECT legitimate work:
   - Never kill processes younger than 5 minutes unless CPU is 0% (zombie).
   - Never kill processes from the MAIN worktree (/Users/mohsen/code/tsz/) without asking — those may be user-initiated.
   - When killing conformance runners, also kill their `tsz --batch` children (they'll respawn otherwise).
   - When killing a stuck rustc, also kill its parent cargo process.
   - The perf-optimizer agent (agent-a60bfca3 or similar) may be running long benchmarks — check if hyperfine is the parent before killing tsz processes.

3. EXECUTION order:
   a. Kill `tsz --batch` workers first (most numerous, highest total CPU)
   b. Kill their parent `tsz-conformance` processes (prevents respawn)
   c. Kill standalone stuck `tsz` processes
   d. Kill stuck `rustc` processes
   e. Kill stale `tail -f`, `samply`, `git index-pack`
   f. Run `pkill -f` patterns for each worktree cleaned, to catch stragglers

4. VERIFY after cleanup:
   - Show remaining process count and total CPU
   - Show `sysctl -n vm.loadavg` (note: load avg lags 1-5 min)
   - List any processes you intentionally left alive and why

5. REPORT what you killed:
   - Group by category (batch workers, conformance runners, stuck tsz, stuck rustc, stale misc)
   - Include count, age range, and estimated CPU freed per category

6. CLEAN BUILD ARTIFACTS — after killing stuck processes, reclaim disk space:
   a. Run `cargo clean` in the main worktree (`/Users/mohsen/code/tsz/`)
   b. Run `cargo clean` in every worktree listed by `git worktree list`
   c. Run these in parallel where possible for speed
   d. For prunable worktrees (shown by `git worktree list`), skip them — they have no target dirs
   e. Also clean stale git-ignored artifacts:
      - Remove `.target-bench/` contents ONLY if no benchmark is currently running (check for `hyperfine` or `bench-vs-tsgo` processes first)
      - Remove `.target-debug/` if present
      - Remove any `/tmp/bench-test/` or `/tmp/jsx-crash*` temp dirs
      - Run `git worktree prune` to clean up stale worktree references
   f. Report total disk space freed (sum the "Removed X files, Y total" lines from cargo clean)

7. VERIFY system health:
   - Show `sysctl -n vm.loadavg`
   - Show `df -h /Users/mohsen` for disk space
   - Show remaining tsz-related process count
```
