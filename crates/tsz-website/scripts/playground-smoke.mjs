import playwright from "../../../TypeScript/node_modules/playwright/index.mjs";

const baseUrl = process.env.PLAYGROUND_URL || "http://127.0.0.1:8080/playground/";

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

async function waitForPlaygroundReady(page) {
  await page.waitForSelector("#playground-status", { state: "visible" });
  await page.waitForFunction(() => {
    const status = document.querySelector("#playground-status");
    return status && !String(status.textContent || "").includes("loading") && !String(status.textContent || "").includes("checking");
  });
}

async function getDiagnosticsSummary(page) {
  return page.evaluate(() => {
    return {
      count: document.querySelectorAll(".diag-item").length,
      okText: document.querySelector(".diagnostics-ok")?.textContent?.trim() || "",
      status: document.querySelector("#playground-status")?.textContent?.trim() || "",
      url: window.location.href,
      selectedExample: document.querySelector("select")?.value || "",
      modelValue: window.monaco?.editor?.getModels?.()?.[0]?.getValue?.() || "",
      markers: window.monaco?.editor?.getModelMarkers?.({ owner: "tsz" })?.map(marker => ({
        code: marker.code,
        message: marker.message,
        startLineNumber: marker.startLineNumber,
        startColumn: marker.startColumn,
      })) || [],
    };
  });
}

async function selectExample(page, key) {
  await page.locator("select").selectOption(key);
  await page.waitForURL(url => url.toString().includes(`example=${key}`));
  await waitForPlaygroundReady(page);
}

const browser = await playwright.chromium.launch({ headless: true });
const context = await browser.newContext();
const page = await context.newPage();

page.on("console", message => {
  console.log(`[browser console:${message.type()}] ${message.text()}`);
});

try {
  await page.goto(`${baseUrl}?example=errors&debugDiagnostics=1`, { waitUntil: "domcontentloaded" });
  await waitForPlaygroundReady(page);

  const initialErrors = await getDiagnosticsSummary(page);
  console.log("initial errors", initialErrors);
  assert(initialErrors.count >= 3, `expected at least 3 diagnostics on errors example, got ${initialErrors.count}`);

  await selectExample(page, "hello");
  const helloSummary = await getDiagnosticsSummary(page);
  console.log("hello summary", helloSummary);
  assert(helloSummary.count === 0, `expected 0 diagnostics on hello example, got ${helloSummary.count}`);

  await selectExample(page, "modules");
  const modulesSummary = await getDiagnosticsSummary(page);
  console.log("modules summary", modulesSummary);
  assert(modulesSummary.count === 0, `expected 0 diagnostics on modules example, got ${modulesSummary.count}`);

  await selectExample(page, "dts");
  const dtsSummary = await getDiagnosticsSummary(page);
  console.log("dts summary", dtsSummary);
  assert(dtsSummary.count === 0, `expected 0 diagnostics on dts example, got ${dtsSummary.count}`);

  await selectExample(page, "errors");
  const finalErrors = await getDiagnosticsSummary(page);
  console.log("final errors", finalErrors);
  assert(finalErrors.count >= 3, `expected diagnostics to survive navigation back to errors, got ${finalErrors.count}`);

  console.log("playground smoke passed");
} finally {
  await context.close();
  await browser.close();
}