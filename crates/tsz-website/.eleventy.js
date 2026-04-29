import fs from "node:fs";

export default function (eleventyConfig) {
  eleventyConfig.addPassthroughCopy({ static: "." });
  eleventyConfig.addPassthroughCopy({ "src/lib": "lib" });
  eleventyConfig.addWatchTarget("../../artifacts");

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
