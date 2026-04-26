const sha = process.env.GITHUB_SHA || "";
export default {
  sha,
  sha7: sha.slice(0, 7),
};
