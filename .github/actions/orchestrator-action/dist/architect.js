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
Object.defineProperty(exports, "__esModule", { value: true });
exports.run = run;
const core = __importStar(require("@actions/core"));
const github = __importStar(require("@actions/github"));
const utils_1 = require("./utils");
async function run() {
    const goal = core.getInput('goal');
    const scopePath = core.getInput('scope_path');
    const currentBranch = github.context.ref?.replace('refs/heads/', '') || 'main';
    const fileTree = await (0, utils_1.getFileTree)(scopePath);
    const prompt = `
    You are a Software Architect responsible for: ${scopePath}.
    GOAL: ${goal}

    Current Files in scope:
    ${fileTree}

    Break this down into atomic coding tasks for developers (Workers).
    Each task should focus on 1-3 files maximum.

    Output JSON:
    {
      "tasks": [
        { "id": "init_server", "description": "Create basic Express app", "files": ["src/api/index.js"] }
      ]
    }
  `;
    const plan = await (0, utils_1.getClaudePlan)(prompt);
    if (!plan?.tasks)
        return;
    for (const task of plan.tasks) {
        console.log(`Dispatching Worker for: ${task.id}`);
        await (0, utils_1.dispatchWorkflow)({
            role: 'worker',
            goal: task.description,
            parent_branch: currentBranch,
            scope_path: scopePath,
            task_context: JSON.stringify(task),
            branch_ref: currentBranch,
        });
    }
}
