/**
 * Sending requests to the background and unwrapping its `Reply`: a failure is thrown as the
 * `Failure` it carries.
 */

import { ext } from './browser';
import type { ContentRequest, Failure, PageRequest, Reply } from './protocol';

export class RequestFailed extends Error {
  kind: string;
  constructor(failure: Failure) {
    super(failure.message);
    this.kind = failure.kind;
  }
}

/** Ask the background; throws `RequestFailed`. */
export async function ask<T>(request: PageRequest | ContentRequest): Promise<T> {
  let reply: Reply<T> | undefined;
  try {
    reply = (await ext.runtime.sendMessage(request)) as Reply<T> | undefined;
  } catch (error) {
    throw new RequestFailed({ kind: 'unavailable', message: String(error) });
  }
  if (!reply) throw new RequestFailed({ kind: 'unavailable', message: 'No answer.' });
  if (!reply.ok) throw new RequestFailed(reply.error);
  return reply.value;
}
