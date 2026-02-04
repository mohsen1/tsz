# TSZ Tracing Quick Reference

## Quick Commands

```bash
# Human-readable tree output (best for debugging)
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Narrowing only
TSZ_LOG="tsz::solver::narrowing=trace" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Checker only  
TSZ_LOG="tsz::checker=debug" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# Solver only
TSZ_LOG="tsz::solver=debug" TSZ_LOG_FORMAT=tree cargo run -- file.ts

# JSON output (for tooling)
TSZ_LOG=debug TSZ_LOG_FORMAT=json cargo run -- file.ts
```

## Log Levels

- `trace` - Most verbose, includes all details
- `debug` - Detailed debugging info
- `info` - General operational info
- `warn` - Warnings only
- `error` - Errors only

## Env Variables

| Variable | Description |
|----------|-------------|
| `TSZ_LOG` | Filter expression (same as RUST_LOG) |
| `TSZ_LOG_FORMAT` | `tree`, `json`, or `text` |

## Filter Syntax

```bash
# Single module
TSZ_LOG="tsz::solver=debug"

# Multiple modules
TSZ_LOG="tsz::solver=trace,tsz::checker=debug"

# All at level
TSZ_LOG=debug
```

## Output Tips

```bash
# Capture to file
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts 2> trace.log

# First 100 lines
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts 2>&1 | head -100

# Search for type ID
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -- file.ts 2>&1 | grep "type_id=42"
```
