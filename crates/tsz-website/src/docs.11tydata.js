export default {
  layout: "layouts/base.njk",
  page_class: "docs-page",
  eleventyComputed: {
    title: (data) => {
      if (data.title) return data.title;
      if (data.page?.fileSlug) {
        return data.page.fileSlug.replace(/[-_]/g, " ");
      }
      return "Docs";
    },
  },
};
