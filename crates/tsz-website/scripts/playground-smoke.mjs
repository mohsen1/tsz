import playwright from "../../../TypeScript/node_modules/playwright/index.mjs";
import { playgroundExamples } from "../src/playground-app/examples.js";

const baseUrl = process.env.PLAYGROUND_URL || "http://127.0.0.1:8080/playground/";
const soundModeExampleKeys = playgroundExamples
  .map(example => example.key)
  .filter(key => key.startsWith("sound_mode"));
const examplesWithExpectedDiagnostics = new Set(["errors", ...soundModeExampleKeys]);

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
      soundChecked: Array.from(document.querySelectorAll(".toolbar-check"))
        .find(label => label.textContent?.trim() === "sound")
        ?.querySelector("input")
        ?.checked || false,
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

  for (const key of soundModeExampleKeys) {
    await selectExample(page, key);
    const soundOn = await getDiagnosticsSummary(page);
    console.log(`${key} sound on`, soundOn);
    assert(soundOn.soundChecked, `expected sound checkbox to be checked on ${key}`);
    assert(soundOn.count >= 1, `expected sound diagnostics on ${key}, got ${soundOn.count}`);
  }

  await selectExample(page, "sound_mode");
  await page.getByLabel("sound").uncheck();
  await waitForPlaygroundReady(page);
  const soundOff = await getDiagnosticsSummary(page);
  console.log("sound_mode sound off", soundOff);
  assert(!soundOff.soundChecked, "expected sound checkbox to be unchecked after toggling it off");
  assert(soundOff.count === 0, `expected sound diagnostics to clear when sound is off, got ${soundOff.count}`);

  for (const example of playgroundExamples) {
    if (examplesWithExpectedDiagnostics.has(example.key)) {
      continue;
    }

    await selectExample(page, example.key);
    const summary = await getDiagnosticsSummary(page);
    console.log(`${example.key} summary`, summary);
    assert(
      summary.count === 0,
      `expected 0 diagnostics on ${example.key} example, got ${summary.count}: ${JSON.stringify(summary.markers)}`
    );
  }

  await selectExample(page, "errors");
  const finalErrors = await getDiagnosticsSummary(page);
  console.log("final errors", finalErrors);
  assert(finalErrors.count >= 3, `expected diagnostics to survive navigation back to errors, got ${finalErrors.count}`);

  console.log("playground smoke passed");
} finally {
  await context.close();
  await browser.close();
}
