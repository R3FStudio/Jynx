import type { JynxClient } from './index.d.ts';
export function discoveryPath(home?: string): string;
export function discoverLocalApi(path?: string): Promise<{ baseUrl: string; apiKey: string }>;
export function connectLocal(options?: { path?: string; fetchImpl?: typeof fetch }): Promise<JynxClient>;
