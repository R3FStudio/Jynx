$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding($false)
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
if ($request.action -eq 'screenshot') {
  Add-Type -AssemblyName System.Windows.Forms
  Add-Type -AssemblyName System.Drawing
  $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
  $bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  try { $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bounds.Size); $bitmap.Save($request.path, [System.Drawing.Imaging.ImageFormat]::Png) }
  finally { $graphics.Dispose(); $bitmap.Dispose() }
  @{ accepted = $true } | ConvertTo-Json -Compress
  exit
}
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$script:nodes = New-Object System.Collections.Generic.List[object]
$script:found = $null
$limit = if ($request.action -eq 'snapshot') { [int]$request.limit } else { 5000 }
function Visit($element, $parent, $depth) {
  if ($null -eq $element -or $script:nodes.Count -ge $limit -or $depth -gt 12) { return }
  try {
    $id = ($element.GetRuntimeId() -join '.')
    if ($request.action -ne 'snapshot' -and $id -eq $request.locator) { $script:found = $element; return }
    $current = $element.Current
    $node = @{ locator = $id; parent = $parent; name = $current.Name; role = $current.ControlType.ProgrammaticName; enabled = $current.IsEnabled; focused = $current.HasKeyboardFocus; actions = @() }
    $node.actions = @($element.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName })
    if (-not $current.IsPassword) {
      $value = $null
      if ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$value)) { $node.text = $value.Current.Value }
    }
    $script:nodes.Add($node)
    $child = $walker.GetFirstChild($element)
    while ($null -ne $child -and $null -eq $script:found -and $script:nodes.Count -lt $limit) { Visit $child $id ($depth + 1); $child = $walker.GetNextSibling($child) }
  } catch [System.Windows.Automation.ElementNotAvailableException] { return }
}
$root = [System.Windows.Automation.AutomationElement]::RootElement
$window = $walker.GetFirstChild($root)
while ($null -ne $window -and $null -eq $script:found -and $script:nodes.Count -lt $limit) {
  $process = Get-Process -Id $window.Current.ProcessId -ErrorAction SilentlyContinue
  if (-not $request.application -or $process.ProcessName -like ('*' + $request.application + '*') -or $window.Current.Name -like ('*' + $request.application + '*')) { Visit $window $null 0 }
  $window = $walker.GetNextSibling($window)
}
if ($request.action -eq 'snapshot') { @{ os = 'Windows'; nodes = @($script:nodes.ToArray()); truncated = ($script:nodes.Count -ge $limit) } | ConvertTo-Json -Depth 12 -Compress; exit }
if ($null -eq $script:found) { throw 'Target expired. Read a fresh accessibility snapshot.' }
$target = $script:found
switch ($request.action) {
  'focus' { $target.SetFocus() }
  'set_text' { $pattern = $target.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern); $pattern.SetValue([string]$request.text) }
  'invoke' {
    $pattern = $null
    if ($target.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) { $pattern.Invoke() }
    elseif ($target.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) { $pattern.Toggle() }
    elseif ($target.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) { $pattern.Expand() }
    else { throw 'This element does not expose a native invoke/toggle/expand action.' }
  }
  'select' { $child = $walker.GetFirstChild($target); for ($i = 0; $i -lt $request.index -and $null -ne $child; $i++) { $child = $walker.GetNextSibling($child) }; if ($null -eq $child) { throw 'Selection index is unavailable.' }; $child.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select() }
  default { throw 'Unsupported native action.' }
}
@{ accepted = $true; action = $request.action; verify = 'Read a fresh snapshot to verify the result.' } | ConvertTo-Json -Compress
