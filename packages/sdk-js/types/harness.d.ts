export class HarnessRpcError extends Error {
  code: number;
  constructor(message: string, code: number);
}
export interface HarnessNotification {
  method: string;
  params?: Record<string, unknown>;
}
export interface HarnessClientOptions {
  binaryPath: string;
  cwd?: string;
  onNotification?: (notification: HarnessNotification) => void;
}
export class HarnessClient {
  constructor(options: HarnessClientOptions);
  start(): void;
  call<T = unknown>(
    method: string,
    params?: Record<string, unknown>,
    options?: { onEvent?: (event: unknown) => void },
  ): Promise<T>;
  diagnostics(): Promise<{ name: string; protocol_version: number; transport: string; features: string[] }>;
  listThreads(params?: { archived?: boolean; searchTerm?: string }): Promise<unknown>;
  saveThread(thread: Record<string, unknown>): Promise<unknown>;
  readThread(threadId: string): Promise<unknown>;
  resumeThread(threadId: string): Promise<unknown>;
  listThreadTurns(threadId: string): Promise<unknown>;
  archiveThread(threadId: string): Promise<unknown>;
  unarchiveThread(threadId: string): Promise<unknown>;
  renameThread(threadId: string, name: string): Promise<unknown>;
  forkThread(threadId: string): Promise<unknown>;
  deleteThread(threadId: string): Promise<unknown>;
  getRuntimeState(): Promise<unknown>;
  updateRuntimeState(patch: Record<string, unknown>): Promise<unknown>;
  clearRuntimeState(): Promise<unknown>;
  getMcpConfig(): Promise<unknown>;
  addMcpServer(name: string, command: string, args?: string[]): Promise<unknown>;
  runAgent(params: Record<string, unknown>, onEvent?: (event: unknown) => void): Promise<unknown>;
  respondToApproval(approvalId: string, approved: boolean): Promise<{ ok: boolean }>;
  close(): void;
}
