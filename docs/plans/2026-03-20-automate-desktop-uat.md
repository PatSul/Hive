# Automate Desktop UAT Plan

Date: March 20, 2026  
Branch: `codex/develop`  
Focus: `Automate > Workflow Builder` desktop stabilization

## Goal

Validate that the redesigned desktop `Automate` surface is usable for the core workflow-builder path:

1. Create a workflow
2. Add supported nodes
3. Connect nodes
4. Save the workflow
5. Reload the workflow from the sidebar list
6. Run the workflow
7. Confirm warnings are honest for unsupported or incomplete cases

## Current Scope

In scope:

- Trigger, Run Command, Call API, Send Notification, End nodes
- node connection UX
- save/load round-trip
- workflow list reload
- graph-aware execution ordering
- builder validation banners

Not in scope yet:

- Condition branching execution
- Execute Skill runtime execution
- rich node property editing inside the builder

Expected current behavior:

- `Condition` and `Execute Skill` appear as unavailable in the palette
- existing unsupported nodes can still load, but the builder should warn and block run
- supported action nodes use starter/default values when created

## Environment

Use the desktop Rust app from this repo:

- Binary: [hive.exe](/H:/WORK/AG/AIrglowStudio/hive/target-workflow-builder/debug/hive.exe)
- Workspace under test: [hive](/H:/WORK/AG/AIrglowStudio/hive)

Recommended setup:

- start from a clean app launch
- switch workspace explicitly to [hive](/H:/WORK/AG/AIrglowStudio/hive) if the app opens wider than intended
- open `Automate`
- open `Workflow Builder`

## Test Cases

### 1. Builder Opens Cleanly

Steps:

1. Launch the desktop app.
2. Open `Automate > Workflow Builder`.

Expected:

- panel renders without a blank canvas
- node palette is visible
- `WORKFLOWS` list is visible
- tip banner explains how to connect nodes

### 2. Supported Node Creation

Steps:

1. Add `Run Command`.
2. Add `Call API`.
3. Add `Send Notification`.
4. Add `End`.

Expected:

- each supported node appears on the canvas
- selecting a node shows the properties panel
- properties panel shows the stored action type instead of every node pretending to be `RunCommand`

### 3. Unsupported Node Honesty

Steps:

1. Inspect `Execute Skill` in the palette.
2. Inspect `Condition` in the palette.

Expected:

- both are visually unavailable/disabled
- each shows a short note explaining why it is unavailable
- clicking them does not add a node

### 4. Connection UX

Steps:

1. Click the Trigger output port.
2. Connect Trigger -> Run Command.
3. Connect Run Command -> Call API.
4. Connect Call API -> Send Notification.
5. Connect Send Notification -> End.

Expected:

- hit targets feel easy to click
- a connection status banner appears while connecting
- a preview line appears while dragging a connection
- the final graph remains visible after each connection

### 5. Save Round-Trip

Steps:

1. Click `Save`.
2. Confirm the workflow appears in the left `WORKFLOWS` list as a saved entry.
3. Click another saved workflow if available.
4. Click the newly saved workflow again.

Expected:

- save succeeds without a silent no-op
- the saved workflow is listed in the sidebar
- clicking the saved entry loads the workflow back into the canvas
- the loaded workflow remains the active entry

### 6. Workspace Persistence

Steps:

1. After saving, confirm a workflow file exists in [hive/.hive/workflows](/H:/WORK/AG/AIrglowStudio/hive/.hive/workflows).
2. Confirm the file name matches the builder workflow id stem.

Expected:

- a `.json` workflow file exists for the executable workflow
- canvas save and executable workflow save both happened

### 7. Run Happy Path

Steps:

1. With a connected supported workflow open, click `Run`.

Expected:

- `Run` is enabled
- no validation error blocks the run
- the run starts
- notifications/reporting indicate the workflow was launched

### 8. Validation Blocking

Steps:

1. Create a new workflow with no Action nodes and inspect the banner.
2. Create a workflow with an unsupported node loaded or present.
3. Create a workflow with disconnected actions.

Expected:

- missing required pieces show validation messages
- unsupported runtime shapes show validation errors
- disconnected actions show warnings
- `Run` is visually disabled when blocking validation errors exist

### 9. Execution Order Follows Graph

Steps:

1. Create three connected action nodes in one order.
2. Rewire them into a different order from the Trigger.
3. Save and run again.

Expected:

- execution order follows the connected graph path, not just node creation order

### 10. Reload Workflows Uses Active Workspace

Steps:

1. Save a workflow in the active workspace.
2. Trigger workflow reload via the app path that refreshes workflows.

Expected:

- reload uses the active workspace root, not a broader parent directory
- the saved workflow remains available in the builder and agents/workflows surfaces

## Failure Log Template

Use this format for each issue found:

```md
### Issue N
- Area:
- Steps to reproduce:
- Expected:
- Actual:
- Severity:
- Screenshot/log:
```

## Exit Criteria

Pass this UAT slice when all of the following are true:

- supported nodes can be added and connected reliably
- save/load round-trip works from the builder UI
- the workflow list is actionable
- run behavior matches the connected graph
- unsupported capabilities are visibly marked and blocked honestly
- no major blank-screen or dead-click issues occur during the Automate flow
