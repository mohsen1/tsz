# Claude Orchestrator Action

This is a hierarchical AI agent system for automated software development using Claude.

## Architecture

The action implements a recursive swarm of agents:

1. **Director**: Breaks down high-level goals into subsystems (e.g., Frontend, Backend, Database)
2. **Architect**: Breaks down subsystem goals into atomic coding tasks
3. **Worker**: Writes code for specific files and opens PRs
4. **Reviewer**: Reviews PRs, merges approved changes, and triggers upstream reviews

## Setup

### Required Secrets

Add the following secret to your GitHub repository settings:

- `ANTHROPIC_API_KEY`: Your Anthropic API key for Claude access

### Required Permissions

The workflow requires these permissions (already set in `orchestrator.yml`):
- `contents: write` - For creating branches and commits
- `pull-requests: write` - For creating and merging PRs
- `actions: write` - For dispatching recursive workflows

## Usage

### Manual Trigger

Go to Actions → AI Orchestrator → Run workflow

**Required inputs:**
- `goal`: Description of what you want the AI to build

**Optional inputs:**
- `role`: Override the starting role (default: `director`)
- `parent_branch`: Branch to base changes on (default: `main`)
- `scope_path`: Directory for the agent to work in (default: `.`)

### Example Goals

```
"Add a REST API for user management with authentication"
"Create a React dashboard with charts"
"Implement a database migration system"
```

## How It Works

1. You trigger the workflow with a goal
2. Director creates feature branches for each subsystem
3. Architect dispatches workers for atomic tasks
4. Workers write code and open PRs
5. Reviewer approves and merges PRs automatically
6. When all workers complete, subsystem PRs are created

## Branch Naming

- Subsystem branches: `feature/<subsystem-name>`
- Worker branches: `worker-<task-id>-<timestamp>`
- Director PRs: `Director Review: <feature-branch>`

## Model Configuration

The action uses `claude-sonnet-4-5-20250514` by default. To change the model, edit the source files in the action directory.

## Local Development

To test changes to the action:

```bash
cd .github/actions/orchestrator-action
npm install
npm run build
npm test
```

## Troubleshooting

### Workflow fails immediately
- Check that `ANTHROPIC_API_KEY` secret is set correctly
- Verify the workflow has proper permissions

### Workers don't create files
- Check the worker logs for API errors
- Verify the task context includes file paths

### PRs aren't created
- Ensure the branch was created successfully
- Check that the base branch exists

### Reviewer doesn't merge PRs
- Review the reviewer's comments for issues
- Check that PRs are prefixed with "AI:"
