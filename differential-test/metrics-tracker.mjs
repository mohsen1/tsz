#!/usr/bin/env node
/**
 * Error Tracking Dashboard - Tracks conformance test metrics over time
 * Stores historical data and generates trend reports
 */

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, join, resolve } from 'path';
import { readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync } from 'fs';

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const CONFIG = {
  wasmPkgPath: resolve(__dirname, '../pkg'),
  conformanceDir: resolve(__dirname, '../../tests/cases/conformance'),
  metricsDir: resolve(__dirname, '../metrics-data'),
  historyFile: resolve(__dirname, '../metrics-data/history.json'),
};

const colors = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
};

function log(msg, color = '') {
  console.log(`${color}${msg}${colors.reset}`);
}

/**
 * Ensure metrics directory exists
 */
function ensureMetricsDir() {
  if (!existsSync(CONFIG.metricsDir)) {
    mkdirSync(CONFIG.metricsDir, { recursive: true });
  }
}

/**
 * Load historical metrics data
 */
function loadHistory() {
  if (existsSync(CONFIG.historyFile)) {
    try {
      const data = readFileSync(CONFIG.historyFile, 'utf-8');
      return JSON.parse(data);
    } catch (e) {
      console.error('Error loading history:', e.message);
      return { runs: [] };
    }
  }
  return { runs: [] };
}

/**
 * Save metrics data to history
 */
function saveHistory(history) {
  ensureMetricsDir();
  writeFileSync(CONFIG.historyFile, JSON.stringify(history, null, 2));
}

/**
 * Parse conformance output from a saved file
 * Expected format:
 *   Summary: { exactMatch, sameCount, missingErrors, extraErrors, ... }
 */
function parseConformanceOutput(outputText) {
  const lines = outputText.split('\n');
  const metrics = {
    timestamp: new Date().toISOString(),
    date: new Date().toLocaleDateString(),
  };

  // Parse key metrics from output
  for (const line of lines) {
    const match = line.match(/Exact Match:\s+(\d+)\s+\(([\d.]+)%\)/);
    if (match) {
      metrics.exactMatch = parseInt(match[1], 10);
      metrics.exactMatchPercent = parseFloat(match[2]);
    }

    const sameMatch = line.match(/Same Error Count:\s+(\d+)\s+\(([\d.]+)%\)/);
    if (sameMatch) {
      metrics.sameCount = parseInt(sameMatch[1], 10);
      metrics.sameCountPercent = parseFloat(sameMatch[2]);
    }

    const missingMatch = line.match(/Tests with missing errors:\s+(\d+)\s+\(([\d.]+)%\)/);
    if (missingMatch) {
      metrics.missingErrors = parseInt(missingMatch[1], 10);
      metrics.missingErrorsPercent = parseFloat(missingMatch[2]);
    }

    const extraMatch = line.match(/Tests with extra errors:\s+(\d+)\s+\(([\d.]+)%\)/);
    if (extraMatch) {
      metrics.extraErrors = parseInt(extraMatch[1], 10);
      metrics.extraErrorsPercent = parseFloat(extraMatch[2]);
    }

    const totalMatch = line.match(/Tests Run:\s+(\d+)/);
    if (totalMatch) {
      metrics.totalTests = parseInt(totalMatch[1], 10);
    }

    const crashedMatch = line.match(/WASM Crashed:\s+(\d+)/);
    if (crashedMatch) {
      metrics.crashed = parseInt(crashedMatch[1], 10);
    }
  }

  return metrics;
}

/**
 * Run conformance tests and capture metrics
 */
async function runConformanceTests(options = {}) {
  const { maxTests = 200, category = null } = options;

  log('Running conformance tests to capture metrics...', colors.cyan);

  // Import the conformance runner
  const { runTests } = await import('./conformance-embedded.mjs');
  const results = await runTests({ maxTests, category, verbose: false });

  return {
    timestamp: new Date().toISOString(),
    date: new Date().toLocaleDateString(),
    ...results,
  };
}

/**
 * Add new metrics run to history
 */
function addMetricsRun(metrics) {
  const history = loadHistory();
  history.runs.push(metrics);

  // Keep only last 100 runs to manage file size
  if (history.runs.length > 100) {
    history.runs = history.runs.slice(-100);
  }

  saveHistory(history);
  return history;
}

/**
 * Calculate trend between two runs
 */
function calculateTrend(current, previous) {
  if (!previous) return null;

  const trend = {
    exactMatchChange: current.exactMatch - previous.exactMatch,
    exactMatchPercentChange: current.exactMatchPercent - previous.exactMatchPercent,
    missingErrorsChange: current.missingErrors - previous.missingErrors,
    extraErrorsChange: current.extraErrors - previous.extraErrors,
  };

  return trend;
}

/**
 * Detect regression based on thresholds
 */
function detectRegression(current, previous, thresholds = {}) {
  const {
    extraErrorIncreaseThreshold = 10,
    missingErrorIncreaseThreshold = 10,
    exactMatchDecreaseThreshold = 0.5,
  } = thresholds;

  if (!previous) return { hasRegression: false };

  const issues = [];

  if (current.extraErrors - previous.extraErrors > extraErrorIncreaseThreshold) {
    issues.push({
      type: 'extra_errors',
      message: `Extra errors increased by ${current.extraErrors - previous.extraErrors} (threshold: ${extraErrorIncreaseThreshold})`,
      severity: 'high',
    });
  }

  if (current.missingErrors - previous.missingErrors > missingErrorIncreaseThreshold) {
    issues.push({
      type: 'missing_errors',
      message: `Missing errors increased by ${current.missingErrors - previous.missingErrors} (threshold: ${missingErrorIncreaseThreshold})`,
      severity: 'high',
    });
  }

  if (previous.exactMatchPercent - current.exactMatchPercent > exactMatchDecreaseThreshold) {
    issues.push({
      type: 'exact_match_decrease',
      message: `Exact match decreased by ${(previous.exactMatchPercent - current.exactMatchPercent).toFixed(2)}% (threshold: ${exactMatchDecreaseThreshold}%)`,
      severity: 'medium',
    });
  }

  return {
    hasRegression: issues.length > 0,
    issues,
  };
}

/**
 * Generate terminal dashboard showing trends
 */
function generateDashboard(history, runs = 10) {
  const recentRuns = history.runs.slice(-runs);
  if (recentRuns.length === 0) {
    log('No metrics data available.', colors.yellow);
    return;
  }

  log('\n' + '═'.repeat(70), colors.bold);
  log('  ERROR TRACKING DASHBOARD', colors.bold);
  log('═'.repeat(70), colors.bold);

  // Latest run
  const latest = recentRuns[recentRuns.length - 1];
  const previous = recentRuns.length > 1 ? recentRuns[recentRuns.length - 2] : null;

  log('\n  Latest Run Summary:', colors.cyan);
  log(`    Date:        ${latest.date}`, colors.dim);
  log(`    Timestamp:   ${latest.timestamp}`, colors.dim);
  log(`    Total Tests: ${latest.totalTests || 'N/A'}`);
  if (latest.exactMatch !== undefined) {
    const color = latest.exactMatchPercent > 90 ? colors.green : latest.exactMatchPercent > 70 ? colors.yellow : colors.red;
    log(`    Exact Match: ${latest.exactMatch} (${latest.exactMatchPercent}%)`, color);
  }
  if (latest.sameCount !== undefined) {
    log(`    Same Count:  ${latest.sameCount} (${latest.sameCountPercent}%)`, colors.blue);
  }
  if (latest.missingErrors !== undefined) {
    const color = latest.missingErrors > 0 ? colors.red : colors.green;
    log(`    Missing:     ${latest.missingErrors} (${latest.missingErrorsPercent}%)`, color);
  }
  if (latest.extraErrors !== undefined) {
    const color = latest.extraErrors > 0 ? colors.yellow : colors.green;
    log(`    Extra:       ${latest.extraErrors} (${latest.extraErrorsPercent}%)`, color);
  }

  // Trend analysis
  if (previous) {
    log('\n  Trend Analysis (vs previous run):', colors.cyan);
    const trend = calculateTrend(latest, previous);

    if (trend.exactMatchChange !== undefined) {
      const arrow = trend.exactMatchChange > 0 ? '↑' : trend.exactMatchChange < 0 ? '↓' : '→';
      const color = trend.exactMatchChange > 0 ? colors.green : trend.exactMatchChange < 0 ? colors.red : colors.dim;
      log(`    Exact Match: ${trend.exactMatchChange > 0 ? '+' : ''}${trend.exactMatchChange} (${arrow})`, color);
    }
    if (trend.missingErrorsChange !== undefined) {
      const arrow = trend.missingErrorsChange > 0 ? '↑' : trend.missingErrorsChange < 0 ? '↓' : '→';
      const color = trend.missingErrorsChange < 0 ? colors.green : trend.missingErrorsChange > 0 ? colors.red : colors.dim;
      log(`    Missing:     ${trend.missingErrorsChange > 0 ? '+' : ''}${trend.missingErrorsChange} (${arrow})`, color);
    }
    if (trend.extraErrorsChange !== undefined) {
      const arrow = trend.extraErrorsChange > 0 ? '↑' : trend.extraErrorsChange < 0 ? '↓' : '→';
      const color = trend.extraErrorsChange < 0 ? colors.green : trend.extraErrorsChange > 0 ? colors.red : colors.dim;
      log(`    Extra:       ${trend.extraErrorsChange > 0 ? '+' : ''}${trend.extraErrorsChange} (${arrow})`, color);
    }

    // Regression detection
    const regression = detectRegression(latest, previous);
    if (regression.hasRegression) {
      log('\n  ⚠️  REGRESSION DETECTED:', colors.red + colors.bold);
      for (const issue of regression.issues) {
        const severityColor = issue.severity === 'high' ? colors.red : colors.yellow;
        log(`    • ${issue.message}`, severityColor);
      }
    } else {
      log('\n  ✓ No regression detected', colors.green);
    }
  }

  // Historical trend table
  if (recentRuns.length > 1) {
    log('\n  Historical Trend:', colors.cyan);
    log(`    ${'Date'.padEnd(12)} ${'Exact'.padEnd(8)} ${'Same'.padEnd(8)} ${'Missing'.padEnd(8)} ${'Extra'.padEnd(8)}`, colors.dim);
    log(`    ${'─'.repeat(12)} ${'─'.repeat(8)} ${'─'.repeat(8)} ${'─'.repeat(8)} ${'─'.repeat(8)}`, colors.dim);

    for (const run of recentRuns) {
      const shortDate = run.date.split('/').slice(0, 2).join('/');
      const exact = run.exactMatch !== undefined ? `${run.exactMatch} (${run.exactMatchPercent}%)` : 'N/A';
      const same = run.sameCount !== undefined ? `${run.sameCount} (${run.sameCountPercent}%)` : 'N/A';
      const missing = run.missingErrors !== undefined ? `${run.missingErrors}` : 'N/A';
      const extra = run.extraErrors !== undefined ? `${run.extraErrors}` : 'N/A';

      log(`    ${shortDate.padEnd(12)} ${exact.padEnd(8)} ${same.padEnd(8)} ${missing.padEnd(8)} ${extra.padEnd(8)}`);
    }
  }

  log('\n' + '═'.repeat(70) + '\n', colors.bold);
}

/**
 * Generate HTML dashboard
 */
function generateHtmlDashboard(history) {
  const runs = history.runs.slice(-50); // Last 50 runs
  if (runs.length === 0) return '';

  const rows = runs.map((run, i) => {
    const prev = i > 0 ? runs[i - 1] : null;
    let trendHtml = '';
    if (prev) {
      const exactChange = run.exactMatchPercent - prev.exactMatchPercent;
      const trend = exactChange >= 0 ? '+%.2f'.replace('%.2f', exactChange.toFixed(2)) : '%.2f'.replace('%.2f', exactChange.toFixed(2));
      const color = exactChange >= 0 ? 'green' : 'red';
      trendHtml = `<span style="color: ${color}">${trend}%</span>`;
    } else {
      trendHtml = '-';
    }
    return `
      <tr>
        <td>${run.date}</td>
        <td>${run.exactMatch || 'N/A'} (${run.exactMatchPercent || 'N/A'}%)</td>
        <td>${run.sameCount || 'N/A'} (${run.sameCountPercent || 'N/A'}%)</td>
        <td style="color: ${run.missingErrors > 0 ? 'red' : 'green'}">${run.missingErrors || 'N/A'}</td>
        <td style="color: ${run.extraErrors > 0 ? 'orange' : 'green'}">${run.extraErrors || 'N/A'}</td>
        <td>${trendHtml}</td>
      </tr>
    `;
  }).join('');

  const html = `<!DOCTYPE html>
<html>
<head>
  <title>Error Tracking Dashboard</title>
  <style>
    body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; margin: 20px; background: #f5f5f5; }
    h1 { color: #333; }
    .container { max-width: 1200px; margin: 0 auto; background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); }
    table { width: 100%; border-collapse: collapse; margin-top: 20px; }
    th, td { padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }
    th { background: #4CAF50; color: white; }
    tr:hover { background: #f5f5f5; }
    .summary { display: flex; gap: 20px; margin-bottom: 20px; }
    .summary-card { flex: 1; padding: 15px; background: #f9f9f9; border-radius: 5px; border-left: 4px solid #4CAF50; }
    .summary-card h3 { margin: 0 0 10px 0; color: #666; font-size: 14px; }
    .summary-card .value { font-size: 24px; font-weight: bold; color: #333; }
    .regression { background: #ffebee; border-left-color: #f44336; }
    .improvement { background: #e8f5e9; border-left-color: #4CAF50; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Error Tracking Dashboard</h1>
    <p>Showing last ${runs.length} runs</p>
    <div class="summary">
      <div class="summary-card">
        <h3>Latest Exact Match</h3>
        <div class="value">${runs[runs.length - 1].exactMatch || 'N/A'} (${runs[runs.length - 1].exactMatchPercent || 'N/A'}%)</div>
      </div>
      <div class="summary-card">
        <h3>Latest Missing Errors</h3>
        <div class="value" style="color: ${runs[runs.length - 1].missingErrors > 0 ? 'red' : 'green'}">${runs[runs.length - 1].missingErrors || 'N/A'}</div>
      </div>
      <div class="summary-card">
        <h3>Latest Extra Errors</h3>
        <div class="value" style="color: ${runs[runs.length - 1].extraErrors > 0 ? 'orange' : 'green'}">${runs[runs.length - 1].extraErrors || 'N/A'}</div>
      </div>
    </div>
    <table>
      <thead>
        <tr>
          <th>Date</th>
          <th>Exact Match</th>
          <th>Same Count</th>
          <th>Missing Errors</th>
          <th>Extra Errors</th>
          <th>Trend</th>
        </tr>
      </thead>
      <tbody>
        ${rows}
      </tbody>
    </table>
  </div>
</body>
</html>`;

  ensureMetricsDir();
  const htmlPath = join(CONFIG.metricsDir, 'dashboard.html');
  writeFileSync(htmlPath, html);
  return htmlPath;
}

/**
 * Print error type distribution (for future implementation)
 */
function printErrorDistribution(errorCodeCounts) {
  const sorted = Object.entries(errorCodeCounts).sort((a, b) => b[1] - a[1]).slice(0, 20);
  log('\n  Top Error Codes:', colors.cyan);
  for (const [code, count] of sorted) {
    log(`    TS${code}: ${count} occurrences`, colors.yellow);
  }
}

/**
 * Main CLI interface
 */
async function main() {
  const args = process.argv.slice(2);
  const command = args[0] || 'dashboard';

  switch (command) {
    case 'run': {
      // Run conformance tests and save metrics
      const maxTests = parseInt(args.find(a => a.startsWith('--max='))?.split('=')[1] || '200', 10);
      const category = args.find(a => !a.startsWith('--') && a !== 'run');
      const metrics = await runConformanceTests({ maxTests, category });
      addMetricsRun(metrics);
      log('\nMetrics saved successfully.', colors.green);
      break;
    }

    case 'dashboard': {
      // Show terminal dashboard
      const history = loadHistory();
      const runs = parseInt(args.find(a => a.startsWith('--runs='))?.split('=')[1] || '10', 10);
      generateDashboard(history, runs);
      break;
    }

    case 'html': {
      // Generate HTML dashboard
      const history = loadHistory();
      const htmlPath = generateHtmlDashboard(history);
      log(`\nHTML dashboard generated: ${htmlPath}`, colors.green);
      break;
    }

    case 'parse': {
      // Parse existing output file
      const file = args[1];
      if (!file) {
        log('Error: Please specify a file to parse', colors.red);
        log('Usage: node metrics-tracker.mjs parse <file>', colors.dim);
        return;
      }
      const outputPath = resolve(__dirname, file);
      if (!existsSync(outputPath)) {
        log(`Error: File not found: ${file}`, colors.red);
        return;
      }
      const output = readFileSync(outputPath, 'utf-8');
      const metrics = parseConformanceOutput(output);
      addMetricsRun(metrics);
      log('Metrics parsed and saved.', colors.green);
      break;
    }

    case 'history': {
      // Show all history
      const history = loadHistory();
      log(`\nTotal runs: ${history.runs.length}`, colors.dim);
      for (const run of history.runs) {
        log(`  ${run.timestamp}: Exact=${run.exactMatch} Missing=${run.missingErrors} Extra=${run.extraErrors}`);
      }
      break;
    }

    case 'regression': {
      // Check for regression
      const history = loadHistory();
      if (history.runs.length < 2) {
        log('Need at least 2 runs to check for regression', colors.yellow);
        return;
      }
      const latest = history.runs[history.runs.length - 1];
      const previous = history.runs[history.runs.length - 2];
      const regression = detectRegression(latest, previous);

      log('\nRegression Check:', colors.cyan);
      log(`  Latest:   ${latest.date}`, colors.dim);
      log(`  Previous: ${previous.date}`, colors.dim);

      if (regression.hasRegression) {
        log('\n⚠️  REGRESSION DETECTED:', colors.red + colors.bold);
        for (const issue of regression.issues) {
          log(`  • ${issue.message}`, issue.severity === 'high' ? colors.red : colors.yellow);
        }
        process.exit(1); // Exit with error code for CI/CD
      } else {
        log('\n✓ No regression detected', colors.green);
      }
      break;
    }

    default:
      log('Error: Unknown command', colors.red);
      log('\nAvailable commands:', colors.dim);
      log('  run [--max=N] [category]     Run conformance tests and save metrics');
      log('  parse <file>                 Parse existing output file');
      log('  dashboard [--runs=N]         Show terminal dashboard');
      log('  html                         Generate HTML dashboard');
      log('  history                      Show all history');
      log('  regression                   Check for regression (exits 1 if found)');
  }
}

main().catch(e => {
  console.error('Error:', e);
  process.exit(1);
});
