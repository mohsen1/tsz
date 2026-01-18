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
const utils_1 = require("./utils");
async function run() {
    const goal = core.getInput('goal');
    const prompt = `
    You are the CTO/Director.
    GOAL: ${goal}

    Analyze the request. Break this into 2-4 major architectural subsystems (e.g., Frontend, Backend, Database, CI/CD).
    Assign a directory path to each.

    Output JSON:
    {
      "subsystems": [
        { "name": "backend", "goal": "Setup Node.js Express server", "path": "src/api" },
        { "name": "frontend", "goal": "Setup React scaffold", "path": "src/web" }
      ]
    }
  `;
    const plan = await (0, utils_1.getClaudePlan)(prompt);
    if (!plan?.subsystems)
        return;
    for (const system of plan.subsystems) {
        const branchName = `feature/${system.name}`;
        console.log(`Creating subsystem branch: ${branchName}`);
        await (0, utils_1.createBranch)(branchName, 'main');
        console.log(`Hiring Architect for: ${system.name}`);
        await (0, utils_1.dispatchWorkflow)({
            role: 'architect',
            goal: system.goal,
            parent_branch: 'main',
            scope_path: system.path,
            branch_ref: branchName,
        });
    }
}
