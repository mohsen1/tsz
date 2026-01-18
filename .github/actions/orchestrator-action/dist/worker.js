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
const exec = __importStar(require("@actions/exec"));
const fs_extra_1 = __importDefault(require("fs-extra"));
const path_1 = __importDefault(require("path"));
const sdk_1 = __importDefault(require("@anthropic-ai/sdk"));
const utils_1 = require("./utils");
async function run() {
    const goal = core.getInput('goal');
    const parentBranch = core.getInput('parent_branch') || 'main';
    const taskContext = core.getInput('task_context');
    const task = (0, utils_1.parseJsonSafe)(taskContext, {});
    const files = task.files || [];
    if (files.length === 0) {
        console.log('No files to edit. Skipping worker.');
        return;
    }
    // Create a unique branch for this worker's changes
    const taskId = task.id || 'task';
    const timestamp = Date.now().toString(36);
    const branchName = `worker-${taskId}-${timestamp}`;
    console.log(`Creating worker branch: ${branchName}`);
    await (0, utils_1.createBranch)(branchName, parentBranch);
    const apiKey = core.getInput('anthropic_key');
    const baseUrl = core.getInput('base_url');
    const anthropicOptions = { apiKey };
    if (baseUrl) {
        anthropicOptions.baseURL = baseUrl;
    }
    const anthropic = new sdk_1.default(anthropicOptions);
    const prompt = `
    You are a code worker.
    GOAL: ${goal}
    FILES TO EDIT:
    ${files.join('\n')}

    Provide full file contents for the files above. If multiple files are needed, prefix each section with "FILE: <path>".
  `;
    let responseText;
    if (process.env.NODE_ENV === 'test') {
        responseText = `FILE: ${files[0]}
const express = require('express');
const app = express();
module.exports = app;`;
    }
    else {
        const msg = await anthropic.messages.create({
            model: 'claude-sonnet-4-5-20250514',
            max_tokens: 2048,
            messages: [{ role: 'user', content: prompt }],
        });
        responseText =
            msg?.content?.[0]?.text?.trim() ||
                `FILE: ${files[0]}
console.log('placeholder');`;
    }
    await writeFilesFromResponse(files, responseText);
    // Commit the files to git
    await commitFiles(branchName, goal);
    await openPullRequest({
        goal,
        parentBranch,
        body: taskContext,
        branchName,
    });
}
async function writeFilesFromResponse(files, responseText) {
    const map = extractFileContents(responseText);
    for (const file of files) {
        if (!(file in map)) {
            console.log(`Warning: No content found for ${file} in response`);
            continue;
        }
        const content = map[file];
        const targetPath = path_1.default.isAbsolute(file) ? file : path_1.default.join(process.cwd(), file);
        await fs_extra_1.default.ensureDir(path_1.default.dirname(targetPath));
        await fs_extra_1.default.writeFile(targetPath, content.trim() + '\n');
        console.log(`Wrote file ${targetPath}`);
    }
}
async function commitFiles(branchName, goal) {
    try {
        // Configure git
        await exec.exec('git', ['config', 'user.name', 'github-actions[bot]']);
        await exec.exec('git', ['config', 'user.email', 'github-actions[bot]@users.noreply.github.com']);
        // Check if there are any changes to commit
        let statusOutput = '';
        await exec.exec('git', ['status', '--porcelain'], {
            listeners: {
                stdout: (data) => {
                    statusOutput += data.toString();
                },
            },
        });
        if (!statusOutput.trim()) {
            console.log('No changes to commit');
            return;
        }
        // Add all changes
        await exec.exec('git', ['add', '-A']);
        // Commit with the goal as the message
        const commitMessage = `AI: ${goal}`;
        await exec.exec('git', ['commit', '-m', commitMessage]);
        // Push the branch
        await exec.exec('git', ['push', '-u', 'origin', branchName]);
        console.log(`Committed and pushed changes to ${branchName}`);
    }
    catch (error) {
        console.error(`Error committing files: ${error}`);
        throw error;
    }
}
function extractFileContents(text) {
    const result = {};
    const regex = /FILE:\s*([^\n]+)\n([\s\S]*?)(?=FILE:|$)/g;
    let match;
    while ((match = regex.exec(text)) !== null) {
        result[match[1].trim()] = match[2].trim();
    }
    return result;
}
async function openPullRequest({ goal, parentBranch, body, branchName, }) {
    const octokit = (0, utils_1.getOctokit)();
    const context = github.context;
    const headBranch = branchName || context.ref?.replace('refs/heads/', '') || 'main';
    const payload = {
        owner: context.repo.owner,
        repo: context.repo.repo,
        title: `AI: ${goal}`,
        head: headBranch,
        base: parentBranch,
        body: body || 'Automated worker output.',
    };
    if (process.env.NODE_ENV === 'test' && global.__TEST_STATE) {
        global.__TEST_STATE.prs.push(payload);
    }
    if (octokit?.rest?.pulls?.create) {
        await octokit.rest.pulls.create(payload);
    }
}
