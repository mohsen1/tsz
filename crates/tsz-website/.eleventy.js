import fs from "node:fs";

export default function (eleventyConfig) {
  eleventyConfig.addPassthroughCopy({ static: "." });
  eleventyConfig.addPassthroughCopy({ "src/lib": "lib" });

  // Curl-installable scripts served at https://tsz.dev/install (no extension,
  // for `curl ... | sh`), /install.sh, and /install.ps1. Single source of
  // truth lives in repo-root scripts/. addPassthroughCopy treats values like
  // "install" (no extension) as a destination directory, so we copy explicitly
  // in an `eleventy.after` hook instead.
  eleventyConfig.addPassthroughCopy({ "../../scripts/install.sh": "install.sh" });
  eleventyConfig.addPassthroughCopy({ "../../scripts/install.ps1": "install.ps1" });

  eleventyConfig.on("eleventy.after", ({ dir }) => {
    const src = "../../scripts/install.sh";
    const dst = `${dir.output}/install`;
    fs.copyFileSync(src, dst);
  });

  eleventyConfig.addWatchTarget("../../artifacts");
  eleventyConfig.addWatchTarget("../../scripts/install.sh");
  eleventyConfig.addWatchTarget("../../scripts/install.ps1");

  const benchmarkArtifacts = [
    "../../artifacts/bench-vs-tsgo-github-latest.json",
    "../../artifacts/bench-vs-tsgo-gcs-latest.json",
    "../../artifacts/bench-vs-tsgo-latest.json",
    "bench-snapshot.json",
  ];
  const latestBenchmarkArtifact = benchmarkArtifacts.find((file) => fs.existsSync(file));
  if (latestBenchmarkArtifact) {
    eleventyConfig.addPassthroughCopy({
      [latestBenchmarkArtifact]: "benchmark-data/latest.json",
    });
  }

  eleventyConfig.setServerOptions({
    watch: ["static/playground-app.js", "static/playground-app.js.map"],
  });

  if (fs.existsSync("../../pkg/web")) {
    eleventyConfig.addPassthroughCopy({ "../../pkg/web": "wasm" });
  }

  return {
    dir: {
      input: "src",
      includes: "_includes",
      data: "_data",
      output: "dist",
    },
    markdownTemplateEngine: "njk",
    htmlTemplateEngine: "njk",
  };
}
