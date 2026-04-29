(function () {
  const bars = Array.from(document.querySelectorAll(".benchmark-mean-card .bench-bar"));
  if (!bars.length) return;

  const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches;

  function formatDuration(ms, precision) {
    if (!Number.isFinite(ms)) return "";
    if (ms > 1000) {
      return `${Math.round(ms / 1000).toLocaleString("en-US")}s`;
    }
    return `${Math.round(ms).toLocaleString("en-US")}ms`;
  }

  const animations = bars.map((bar) => {
    const value = bar.querySelector(".bench-bar-value");
    const targetWidth = Number(bar.dataset.targetWidth);
    const targetMs = Number(bar.dataset.targetMs);
    const precision = Number(bar.dataset.durationPrecision || 0);
    const durationMs = Math.max(1, targetMs);
    return { bar, value, targetWidth, targetMs, precision, durationMs };
  }).filter(({ value, targetWidth, targetMs }) => value && Number.isFinite(targetWidth) && Number.isFinite(targetMs));

  if (!animations.length) return;

  if (reduceMotion) {
    for (const item of animations) {
      item.bar.style.width = `${item.targetWidth}%`;
      item.value.textContent = formatDuration(item.targetMs, item.precision);
    }
    return;
  }

  for (const item of animations) {
    item.bar.style.width = "0px";
    item.bar.style.minWidth = "0px";
    item.value.textContent = formatDuration(0, item.precision);
  }

  const startedAt = performance.now();

  function tick(now) {
    const elapsed = now - startedAt;
    let done = true;

    for (const item of animations) {
      const progress = Math.min(1, elapsed / item.durationMs);
      item.bar.style.width = `${item.targetWidth * progress}%`;
      item.value.textContent = formatDuration(item.targetMs * progress, item.precision);
      if (progress < 1) done = false;
    }

    if (!done) {
      requestAnimationFrame(tick);
      return;
    }

    for (const item of animations) {
      item.bar.style.width = `${item.targetWidth}%`;
      item.bar.style.minWidth = "";
      item.value.textContent = formatDuration(item.targetMs, item.precision);
    }
  }

  requestAnimationFrame(tick);
})();
