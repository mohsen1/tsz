# TSZ Tracing Quickref

```bash
TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo run -p tsz-cli -- file.ts
TSZ_LOG=debug TSZ_LOG_FORMAT=json cargo run -p tsz-cli -- file.ts
TSZ_LOG="tsz_solver=debug" TSZ_LOG_FORMAT=tree cargo run -p tsz-cli -- file.ts
TSZ_LOG="tsz_checker=debug" TSZ_LOG_FORMAT=tree cargo run -p tsz-cli -- file.ts
TSZ_LOG="tsz_solver::narrowing=trace" TSZ_LOG_FORMAT=tree cargo run -p tsz-cli -- file.ts
```

Levels: `trace`, `debug`, `info`, `warn`, `error`.
Format: `tree`, `json`, `text`.

Tips:
- Capture: `... 2> trace.log`.
- Trim: `... 2>&1 | head -100`.
- Search ids: `rg "type_id=42" trace.log`.
