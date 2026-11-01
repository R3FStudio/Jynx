export interface ChatMessage {
  role: 'system' | 'developer' | 'user' | 'assistant' | 'tool';
  content: string;
  name?: string;
}
export interface ChatRequest {
  model: string;
  messages: ChatMessage[];
  temperature?: number;
  max_tokens?: number;
  [key: string]: unknown;
}
export interface ModelInfo {
  id: string;
  object: string;
  created?: number;
  owned_by?: string;
  [key: string]: unknown;
}
export interface ChatChoice {
  index: number;
  message?: ChatMessage;
  delta?: Partial<ChatMessage>;
  finish_reason?: string | null;
}
export interface ChatCompletion {
  id: string;
  choices: ChatChoice[];
  [key: string]: unknown;
}
export interface ChatChunk {
  id: string;
  choices: ChatChoice[];
  [key: string]: unknown;
}
export interface RequestOptions {
  signal?: AbortSignal;
}
export interface ClientOptions {
  baseUrl?: string;
  apiKey?: string;
  fetchImpl?: typeof fetch;
}
export class JynxApiError extends Error {
  status: number;
  body: unknown;
  constructor(message: string, status: number, body: unknown);
}
export class JynxProtocolError extends Error {
  constructor(message: string);
}
export class JynxClient {
  readonly baseUrl: string;
  readonly apiKey: string;
  constructor(options?: ClientOptions);
  health(options?: RequestOptions): Promise<unknown>;
  listModels(options?: RequestOptions): Promise<ModelInfo[]>;
  createChatCompletion(input: ChatRequest, options?: RequestOptions): Promise<ChatCompletion>;
  streamChatCompletion(input: ChatRequest, options?: RequestOptions): AsyncGenerator<ChatChunk>;
}


