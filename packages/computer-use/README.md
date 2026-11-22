# Jynx native computer-use linker

This MCP server connects the Harness to OS accessibility APIs: AT-SPI on Linux, UI Automation on Windows, and Accessibility via System Events on macOS. It reads native elements and invokes controls, focuses fields, sets text, and selects items. It does not run arbitrary scripts supplied by the model. Screenshots are a separate, explicit operation; the returned PNG can be inspected using the Harness `view_image` tool.

Enable it from Desktop's Computer page, or add this to the existing Harness MCP configuration (adjust the absolute path):

```json
{"mcpServers":{"jynx-computer":{"command":"python3","args":["/absolute/path/to/Jynx/packages/computer-use/server.py"],"enabled":true}}}
```

Use `python` on Windows. Python 3 is required. Linux also requires `python3-gi` and `gir1.2-atspi-2.0`; screenshots use your desktop's screenshot utility. Windows uses PowerShell with UI Automation; macOS uses `osascript` and `screencapture`. Grant OS accessibility/screen-recording permission when requested. Unsupported apps or restricted sessions produce real errors; the linker does not bypass OS permissions or claim actions succeeded without checking their native return values. Read a fresh accessibility snapshot after an action to verify its outcome.

Run `python3 server.py --capabilities` to detect features without reading applications or taking screenshots. Element handles are scoped to a server process and expire after a new snapshot. Filter snapshots to the requested application. Installed skills live at the repository's root `skills/` directory. Existing Harness MCP approvals apply to these tools.

Validation: `python3 -m unittest discover -s tests` runs synthetic protocol/action tests. On Linux, `dbus-run-session -- python3 tests/live_linux.py` additionally opens a temporary GTK fixture and verifies real AT-SPI text editing and button activation in an isolated bus; it does not inspect other apps or take screenshots. Windows and macOS drivers require validation on their respective systems.
