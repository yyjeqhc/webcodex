export class BrowserFault extends Error {
  constructor(readonly code: string, message: string, readonly recovery = 'Observe the current state before choosing another action.') {
    super(message);
    this.name = 'BrowserFault';
  }
}

// Never turn a possibly-dispatched browser action into an ordinary completed failure.
// The SDK terminates the provider without a fabricated response, preserving the
// WebCodex Runner's OutcomeUnknown semantics. There is no automatic retry.
export class UncertainAction extends Error {
  constructor() {
    super('Browser action outcome is unknown. Reconcile browser state before retrying.');
    this.name = 'UncertainAction';
  }
}

export function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function boundedString(value: unknown, name: string, max = 8192, allowEmpty = false): string {
  if (typeof value !== 'string' || (!allowEmpty && value.length === 0) || value.includes('\0') || Buffer.byteLength(value) > max) {
    throw new BrowserFault('invalid_argument', `Invalid ${name}.`);
  }
  return value;
}
