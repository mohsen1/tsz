import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const repoRoot = path.resolve(root, "..", "..");

function spawnChild(command, args) {
  return spawn(command, args, {
    cwd: root,
    stdio: "inherit",
    env: process.env,
  });
}

function runOnce(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawnChild(command, args);
    child.on("exit", code => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`command failed: ${command} ${args.join(" ")} (${code ?? "signal"})`));
    });
  });
}

const children = [];

const eleventyArgs = ["eleventy", "--serve", "--watch"];
const cliArgs = process.argv.slice(2);
for (let i = 0; i < cliArgs.length; i += 1) {
  const arg = cliArgs[i];
  if (arg === "--port" && cliArgs[i + 1]) {
    eleventyArgs.push(arg, cliArgs[i + 1]);
    i += 1;
    continue;
  }
  if (arg.startsWith("--port=")) {
    eleventyArgs.push(arg);
  }
}

function shutdown(code) {
  for (const child of children) {
    child.kill("SIGTERM");
  }
  process.exit(code);
}

process.on("SIGINT", () => shutdown(130));
process.on("SIGTERM", () => shutdown(143));

if (process.env.TSZ_WEBSITE_SKIP_BENCH_PREPARE !== "1") {
  await runOnce("bash", [path.join(repoRoot, "scripts", "start-website.sh"), "--prepare-only"]);
}

await runOnce(process.execPath, [path.join(root, "scripts", "sync-docs.mjs")]);

children.push(spawnChild(process.execPath, [path.join(root, "scripts", "build-playground.mjs"), "--watch"]));
children.push(
  spawnChild(
    process.platform === "win32" ? "npx.cmd" : "npx",
    eleventyArgs
  )
);

await Promise.race(
  children.map(
    child =>
      new Promise(resolve => {
        child.on("exit", code => resolve(code ?? 1));
      })
  )
).then(code => shutdown(code));
