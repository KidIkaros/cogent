# VS Code Integration for Cogent

Cogent integrates with VS Code through SARIF output and the SARIF Viewer extension.

## Prerequisites

1. Install the **SARIF Viewer** extension from the VS Code marketplace:
   - Search for "SARIF Viewer" in the Extensions panel (Ctrl+Shift+X)
   - Install by Microsoft (`MS-SarifVSCode.sarif-viewer`)

## Quick Start

Run Cogent with SARIF output and open the results inline:

```bash
cogent check . --format sarif > cogent.sarif
```

Then open `cogent.sarif` in VS Code. The SARIF Viewer extension will automatically:
- Show findings as inline squiggles (red/yellow underlines)
- Populate the Problems panel with issue details
- Allow clicking each finding to jump to the exact file:line

## Auto-run on Save

Add a VS Code task that runs `cogent check` automatically. Create `.vscode/tasks.json` in your project root:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "cogent check",
      "type": "shell",
      "command": "cogent",
      "args": ["check", ".", "--format", "sarif"],
      "group": "test",
      "presentation": {
        "reveal": "silent"
      },
      "problemMatcher": []
    }
  ]
}
```

## Keyboard Shortcut

Bind a key to run the check task instantly. Open your keybindings JSON (Ctrl+K Ctrl+S → click icon in top right) and add:

```json
{
  "key": "ctrl+shift+c",
  "command": "workbench.action.tasks.runTask",
  "args": "cogent check"
}
```

Now press **Ctrl+Shift+C** to re-run Cogent and refresh the SARIF panel.

## Recommended Settings

Add these to your workspace settings (`.vscode/settings.json`):

```json
{
  "sarif-viewer.connectToGithubCodeScanning": "off"
}
```

This prevents the SARIF Viewer from attempting GitHub Code Scanning integration and keeps everything local.

## Troubleshooting

- **No squiggles shown**: Make sure the SARIF Viewer extension is enabled. Try reloading the window (Ctrl+Shift+P → "Developer: Reload Window").
- **File paths not resolving**: Cogent SARIF output uses relative paths. Ensure you run `cogent check` from the workspace root.
- **Large repos slow**: Use `--skip vulnscan,supply-chain` to skip heavy checks during development. Run the full suite in CI.
