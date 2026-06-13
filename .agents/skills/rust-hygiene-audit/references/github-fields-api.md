# GitHub sub-issues + native Issue Fields API

The non-obvious bits that cost time the first time. Driven via `gh api`.

## Sub-issues (parent/child hierarchy)

```bash
# child_db_id is the issue DATABASE id, not its number:
child_db_id=$(gh api repos/OWNER/REPO/issues/<child_number> -q .id)
gh api -X POST repos/OWNER/REPO/issues/<parent_number>/sub_issues -F sub_issue_id=$child_db_id
```

- Use **`-F`** (typed integer). `-f` sends a *string* and the call 422s / fails
  silently — this is the #1 gotcha.
- `sub_issue_id` is the database id from `.id`, **not** the issue number.
- Always write a `## Child issue map` into the parent body too, as a durable
  fallback that survives even if a native link fails.

## Native Issue Fields (GraphQL, preview)

Read existing fields — `issueFields` is a **union**, query with `... on`:

```graphql
{ repository(owner:"O",name:"R"){ issueFields(first:30){ nodes{
    __typename ... on IssueFieldSingleSelect { id name options{ id name } } } } } }
```

Set values (one mutation can set several fields — pass a list):

```graphql
mutation { setIssueFieldValue(input:{
  issueId:"<issue node id>",
  issueFields:[
    {fieldId:"<Priority field id>", singleSelectOptionId:"<option id>"},
    {fieldId:"<Effort field id>",   singleSelectOptionId:"<option id>"}
  ]}){ issue{ number } } }
```

- `issueId` is the GraphQL **node id** (`gh issue list --json id`), not number.
- `setIssueFieldValue` works with **`repo`** scope.
- `createIssueField` / `updateIssueField` (creating a field, adding/renaming
  options) require **`admin:org`**. A default `repo`+`read:org` token cannot add
  a 4th option to an existing field. If you need to and lack the scope: ask the
  user to add the option in the issue-fields UI, or `gh auth refresh -s
  admin:org`, or fall back to the options that already exist.
- `IssueFieldSingleSelectOptionInput.priority` is `Int!` (required) even though
  introspection reports it nullable — pass an explicit ordering int.
- Read back values via `issueFieldValues` → `... on IssueFieldSingleSelectValue
  { field{ ... on IssueFieldSingleSelect{ name } } name }`.

## Priority/Effort conventions used by this skill

- Priority ranking **correctness > speed > tech-debt** → High / Medium / Low,
  with Urgent reserved for `urgent`/`panic` labels.
- Effort from audit size: S→low, M→mid, L→high, epic-scale→top option.
- Both axes are independent: a tech-debt epic is `Priority=Low, Effort=High`.
