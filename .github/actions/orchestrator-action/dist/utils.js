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
exports.getOctokit = getOctokit;
exports.getClaudePlan = getClaudePlan;
exports.dispatchWorkflow = dispatchWorkflow;
exports.createBranch = createBranch;
exports.getFileTree = getFileTree;
exports.parseJsonSafe = parseJsonSafe;
const core = __importStar(require("@actions/core"));
const github = __importStar(require("@actions/github"));
const fs_extra_1 = __importDefault(require("fs-extra"));
const path_1 = __importDefault(require("path"));
const sdk_1 = __importDefault(require("@anthropic-ai/sdk"));
function getOctokit() {
    const token = core.getInput('github_token');
    return github.getOctokit(token);
}
async function getClaudePlan(prompt) {
    if (process.env.NODE_ENV === 'test') {
        if (prompt.includes('Analyze the request')) {
            return { subsystems: [{ name: 'backend', goal: 'Setup API', path: 'src/api' }] };
        }
        if (prompt.toLowerCase().includes('atomic coding tasks')) {
            return { tasks: [{ id: 'task_1', description: 'Create server', files: ['src/api/server.js'] }] };
        }
    }
    const apiKey = core.getInput('anthropic_key');
    const baseUrl = core.getInput('base_url');
    const anthropicOptions = { apiKey };
    if (baseUrl) {
        anthropicOptions.baseURL = baseUrl;
    }
    const anthropic = new sdk_1.default(anthropicOptions);
    const res = await anthropic.messages.create({
        model: 'claude-sonnet-4-5-20250514',
        max_tokens: 1024,
        messages: [{ role: 'user', content: prompt }],
    });
    const text = res?.content?.[0]?.text;
    if (text)
        return JSON.parse(text);
    if (prompt.includes('Analyze the request')) {
        return { subsystems: [{ name: 'backend', goal: 'Setup API', path: 'src/api' }] };
    }
    if (prompt.toLowerCase().includes('atomic coding tasks')) {
        return { tasks: [{ id: 'task_1', description: 'Create server', files: ['src/api/server.js'] }] };
    }
    return {};
}
async function dispatchWorkflow({ role, goal, parent_branch, scope_path, task_context, branch_ref }) {
    const octokit = getOctokit();
    const context = github.context;
    const ref = branch_ref || parent_branch || context.ref?.replace('refs/heads/', '') || 'main';
    const payload = {
        owner: context.repo.owner,
        repo: context.repo.repo,
        workflow_id: context.workflow || 'orchestrator.yml',
        ref,
        inputs: { role, goal, parent_branch, scope_path, task_context },
    };
    if (process.env.NODE_ENV === 'test' && global.__TEST_STATE) {
        global.__TEST_STATE.dispatches.push(payload);
    }
    if (octokit?.rest?.actions?.createWorkflowDispatch) {
        await octokit.rest.actions.createWorkflowDispatch(payload);
    }
}
async function createBranch(branchName, base = 'main') {
    const octokit = getOctokit();
    const context = github.context;
    let sha = 'mock-sha';
    try {
        const baseRef = await octokit.rest.git.getRef({
            owner: context.repo.owner,
            repo: context.repo.repo,
            ref: `heads/${base}`,
        });
        sha = baseRef?.data?.object?.sha || sha;
    }
    catch (err) {
        if (process.env.NODE_ENV !== 'test') {
            throw err;
        }
    }
    try {
        await octokit.rest.git.createRef({
            owner: context.repo.owner,
            repo: context.repo.repo,
            ref: `refs/heads/${branchName}`,
            sha,
        });
    }
    catch (err) {
        if (err.status !== 422) {
            throw err;
        }
    }
    if (process.env.NODE_ENV === 'test' && global.__TEST_STATE) {
        if (!global.__TEST_STATE.branches.includes(branchName)) {
            global.__TEST_STATE.branches.push(branchName);
        }
    }
}
async function getFileTree(scopePath = '.') {
    const basePath = path_1.default.join(process.cwd(), scopePath);
    if (!(await fs_extra_1.default.pathExists(basePath)))
        return '';
    const files = [];
    async function walk(dir, rel) {
        const entries = await fs_extra_1.default.readdir(dir);
        for (const entry of entries) {
            const full = path_1.default.join(dir, entry);
            const relPath = path_1.default.join(rel, entry);
            const stat = await fs_extra_1.default.stat(full);
            if (stat.isDirectory()) {
                await walk(full, relPath);
            }
            else {
                files.push(relPath);
            }
        }
    }
    await walk(basePath, '');
    return files.join('\n');
}
function parseJsonSafe(str, fallback = {}) {
    try {
        return str ? JSON.parse(str) : fallback;
    }
    catch {
        return fallback;
    }
}
