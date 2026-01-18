"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.run = run;
const core = __importStar(require("@actions/core"));
const github = __importStar(require("@actions/github"));
const sdk_1 = __importDefault(require("@anthropic-ai/sdk"));
const utils_1 = require("./utils");
async function run() {
    const octokit = (0, utils_1.getOctokit)();
    const anthropicKey = core.getInput('anthropic_key');
    const context = github.context;
    const pr = context.payload?.pull_request;
    if (!pr || !pr.title?.startsWith('AI:')) {
        console.log('No AI PR to review. Skipping.');
        return;
    }
    console.log(`Architect reviewing PR #${pr.number}: ${pr.title}`);
    const diff = process.env.NODE_ENV === 'test'
        ? { data: 'diff --git a/file b/file' }
        : await octokit.rest.pulls.get({
            owner: context.repo.owner,
            repo: context.repo.repo,
            pull_number: pr.number,
            mediaType: { format: 'diff' },
        });
    let review;
    if (process.env.NODE_ENV === 'test') {
        review = { approved: true, comment: 'Looks good.' };
    }
    else {
        const baseUrl = core.getInput('base_url');
        const anthropicOptions = { apiKey: anthropicKey };
        if (baseUrl) {
            anthropicOptions.baseURL = baseUrl;
        }
        const anthropic = new sdk_1.default(anthropicOptions);
        const prompt = `
      You are a Code Reviewer.
      Review this diff. Does it accomplish the goal in the title?
      Are there syntax errors?

      Diff:
      ${diff?.data || ''}

      Output JSON:
      { "approved": true, "comment": "Looks good." } 
      OR 
      { "approved": false, "comment": "Fix syntax error on line 5." }
    `;
        const msg = await anthropic.messages.create({
            model: 'claude-sonnet-4-5-20250514',
            max_tokens: 1024,
            messages: [{ role: 'user', content: prompt }],
        });
        try {
            review = JSON.parse(msg?.content?.[0]?.text || '{}');
        }
        catch {
            review = { approved: false, comment: 'Unable to parse review response.' };
        }
    }
    if (review.approved) {
        console.log('Approving and Merging...');
        await octokit.rest.pulls.merge({
            owner: context.repo.owner,
            repo: context.repo.repo,
            pull_number: pr.number,
            merge_method: 'squash',
        });
        await checkUpstreamPR(octokit, context, pr.base.ref);
    }
    else {
        console.log('Rejecting PR...');
        await octokit.rest.pulls.createReview({
            owner: context.repo.owner,
            repo: context.repo.repo,
            pull_number: pr.number,
            body: review.comment || 'Changes requested.',
            event: 'REQUEST_CHANGES',
        });
    }
}
async function checkUpstreamPR(octokit, context, featureBranch) {
    if (featureBranch === 'main')
        return;
    const existingPrs = (await octokit?.rest?.pulls?.list({
        owner: context.repo.owner,
        repo: context.repo.repo,
        head: `${context.repo.owner}:${featureBranch}`,
        base: 'main',
    })) || { data: [] };
    if (existingPrs.data.length === 0) {
        const payload = {
            owner: context.repo.owner,
            repo: context.repo.repo,
            title: `Director Review: ${featureBranch}`,
            head: featureBranch,
            base: 'main',
            body: 'Subsystem implementation ready for review.',
        };
        if (process.env.NODE_ENV === 'test' && global.__TEST_STATE) {
            global.__TEST_STATE.prs.push(payload);
            return;
        }
        if (octokit?.rest?.pulls?.create) {
            await octokit.rest.pulls.create(payload);
        }
    }
}
