import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

export class HarnessRpcError extends Error {
  constructor(message, code) {
    super(message);
    this.name = 'HarnessRpcError';
    this.code = code;
  }
}


export class HarnessClient {
  constructor({ binaryPath, cwd, spawnImpl = spawn, onNotification } = {}) {
    if (!binaryPath) throw new TypeError('binaryPath is required');
    this.binaryPath = binaryPath;
    this.cwd = cwd;
    this.spawnImpl = spawnImpl;
    this.onNotification = onNotification;
    this.nextId = 1;
    this.pending = new Map();
    this.child = null;
    this.closed = false;
  }

  start() {
    if (this.closed) throw new Error('Harness client is closed');
    if (this.child) return;
    const child = this.spawnImpl(this.binaryPath, ['serve'], {
      cwd: this.cwd,
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
    });
    this.child = child;
    this.lines = createInterface({ input: child.stdout });
    this.lines.on('line', (line) => this.handleLine(line));
    child.stderr.on('data', () => undefined);
    child.once('error', (error) => this.failAll(error));
    child.once('exit', (code) => this.failAll(new Error(`Harness exited with code ${code}`)));
  }

  handleLine(line) {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      this.onNotification?.({ method: 'protocol/error', params: { message: 'Harness returned invalid JSON' } });
      return;
    }
    if (message.method === 'agent/event') {
      for (const [id, entry] of this.pending) {
        if (entry.requestId !== message.params?.request_id) continue;
        try {
          entry.onEvent?.(message.params.event);
        } catch (error) {
          this.pending.delete(id);
          entry.reject(error);
        }
        break;
      }
      return;
    }
    if (message.method) {
      this.onNotification?.(message);
      return;
    }
    const entry = this.pending.get(String(message.id));
    if (!entry) return;
    this.pending.delete(String(message.id));
    if (message.error) entry.reject(new HarnessRpcError(message.error.message, message.error.code));
    else entry.resolve(message.result);
  }

  failAll(error) {
    for (const entry of this.pending.values()) entry.reject(error);
    this.pending.clear();
    this.child = null;
  }

  call(method, params = {}, { onEvent } = {}) {
    this.start();
    const id = this.nextId++;
    const requestId = method === 'agent/run' ? String(params.request_id || id) : undefined;
    const request = { id, method, params: requestId ? { ...params, request_id: requestId } : params };
    return new Promise((resolve, reject) => {
      this.pending.set(String(id), { resolve, reject, requestId, onEvent });
      try {
        this.child.stdin.write(`${JSON.stringify(request)}\n`, (error) => {
          if (!error || !this.pending.has(String(id))) return;
          this.pending.delete(String(id));
          reject(error);
        });
      } catch (error) {
        this.pending.delete(String(id));
        reject(error);
      }
    });
  }

  diagnostics() {
    return this.call('server/diagnostics');
  }
  listThreads(params = {}) {
    return this.call('thread/list', params);
  }
  saveThread(thread) {
    return this.call('thread/save', { thread });
  }
  readThread(threadId) {
    return this.call('thread/read', { threadId });
  }
  resumeThread(threadId) {
    return this.call('thread/resume', { threadId });
  }
  listThreadTurns(threadId) {
    return this.call('thread/turns/list', { threadId });
  }
  archiveThread(threadId) {
    return this.call('thread/archive', { threadId });
  }
  unarchiveThread(threadId) {
    return this.call('thread/unarchive', { threadId });
  }
  renameThread(threadId, name) {
    return this.call('thread/name/set', { threadId, name });
  }
  forkThread(threadId) {
    return this.call('thread/fork', { threadId });
  }
  deleteThread(threadId) {
    return this.call('thread/delete', { threadId });
  }
  getRuntimeState() {
    return this.call('runtime/state/get');
  }
  updateRuntimeState(patch) {
    return this.call('runtime/state/update', { patch });
  }
  clearRuntimeState() {
    return this.call('runtime/state/clear');
  }
  getMcpConfig() {
    return this.call('mcp/config');
  }
  addMcpServer(name, command, args = []) {
    return this.call('mcp/config/add', { name, command, args });
  }
  runAgent(params, onEvent) {
    return this.call('agent/run', params, { onEvent });
  }
  respondToApproval(approvalId, approved) {
    return this.call('approval/respond', { approval_id: approvalId, approved });
  }

  close() {
    const child = this.child;
    this.closed = true;
    this.failAll(new Error('Harness client closed'));
    this.lines?.close();
    child?.kill();
  }
}
