export class JynxApiError extends Error {
  constructor(message, status, body) {
    super(message);
    this.name = 'JynxApiError';
    this.status = status;
    this.body = body;
  }
}

export class JynxProtocolError extends Error {
  constructor(message) {
    super(message);
    this.name = 'JynxProtocolError';
  }
}
