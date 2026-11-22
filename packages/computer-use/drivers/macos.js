
function run(argv) {
  const request = JSON.parse(argv[0]);
  const events = Application('System Events');
  const nodes = [];
  const safe = (fn, fallback) => { try { return fn(); } catch (_) { return fallback; } };
  const apps = events.applicationProcesses.whose({backgroundOnly: false})();
  if (request.action === 'snapshot') {
    const limit = request.limit;
    function visit(element, locator, parent, depth) {
      if (nodes.length >= limit || depth > 8) return;
      const name = safe(() => element.name(), '');
      const role = safe(() => element.role(), 'unknown');
      const key = JSON.stringify(locator);
      const node = {locator: {...locator, name, role}, parent, name, role,
        enabled: safe(() => element.enabled(), false), focused: safe(() => element.focused(), false),
        actions: safe(() => element.actions().map(action => action.name()), [])};
      if (role !== 'AXSecureTextField') {
        const value = safe(() => element.value(), null);
        if (typeof value === 'string') node.text = value.slice(0, 2000);
      }
      nodes.push(node);
      const children = safe(() => element.uiElements(), []);
      children.slice(0, limit).forEach((child, index) => visit(child, {pid: locator.pid, path: locator.path.concat(index)}, key, depth + 1));
    }
    apps.forEach(app => {
      if (request.application && !safe(() => app.name(), '').toLowerCase().includes(request.application.toLowerCase())) return;
      const windows = safe(() => app.windows(), []);
      windows.forEach((window, index) => visit(window, {pid: app.unixId(), path: [index]}, null, 0));
    });
    return JSON.stringify({os: 'Darwin', nodes, truncated: nodes.length >= limit});
  }
  const locator = request.locator;
  const app = apps.find(app => app.unixId() === locator.pid);
  if (!app) throw Error('Application closed. Read a fresh snapshot.');
  let target = app.windows()[locator.path[0]];
  locator.path.slice(1).forEach(index => { target = target.uiElements()[index]; });
  if (!target || safe(() => target.role(), '') !== locator.role || safe(() => target.name(), '') !== locator.name)
    throw Error('Target changed. Read a fresh accessibility snapshot.');
  if (request.action === 'focus') target.focused = true;
  else if (request.action === 'set_text') target.value = request.text;
  else if (request.action === 'select') target.uiElements()[request.index || 0].selected = true;
  else if (request.action === 'invoke') {
    const actions = target.actions();
    const action = request.name ? actions.find(action => action.name() === request.name) : actions[0];
    if (!action) throw Error('This element exposes no matching native action.');
    action.perform();
  } else throw Error('Unsupported native action.');
  return JSON.stringify({accepted: true, action: request.action, verify: 'Read a fresh snapshot to verify the result.'});
}
