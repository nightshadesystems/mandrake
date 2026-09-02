// The one HTTP client (ADR-0008): openapi-fetch over the generated schema.
// Adds the CSRF header every mutating cookie-authenticated request needs
// (ADR-0007) and turns problem responses into thrown ApiError values.

import createClient from 'openapi-fetch';

import type { components, paths } from './schema';

export type Problem = components['schemas']['Problem'];
export type Schemas = components['schemas'];

/** A non-2xx response, carrying the RFC 7807 problem the daemon sent. */
export class ApiError extends Error {
  readonly problem: Problem;

  constructor(problem: Problem) {
    super(problem.detail ?? problem.title);
    this.name = 'ApiError';
    this.problem = problem;
  }

  get status(): number {
    return this.problem.status;
  }

  /** The slug of a typed problem such as `locked` or `invalid-credentials`. */
  get slug(): string | undefined {
    const base = 'https://mandrake.nightshade.systems/problems/';
    return this.problem.type.startsWith(base) ? this.problem.type.slice(base.length) : undefined;
  }

  static from(error: unknown, status: number): ApiError {
    if (isProblem(error)) return new ApiError(error);
    return new ApiError({
      type: 'about:blank',
      title: status === 0 ? 'Network error' : `HTTP ${String(status)}`,
      status,
    });
  }
}

function isProblem(value: unknown): value is Problem {
  return (
    typeof value === 'object' &&
    value !== null &&
    'status' in value &&
    'title' in value &&
    typeof (value as { status: unknown }).status === 'number'
  );
}

export const api = createClient<paths>({
  baseUrl: '/api/v1',
  headers: { 'X-Mandrake-Request': '1' },
});

interface Outcome<T> {
  data?: T;
  error?: unknown;
  response: Response;
}

/** Await a client call and unwrap it: data on success, ApiError otherwise. */
export async function unwrap<T>(pending: Promise<Outcome<T>>): Promise<T> {
  let outcome: Outcome<T>;
  try {
    outcome = await pending;
  } catch (cause) {
    throw new ApiError({
      type: 'about:blank',
      title: 'Network error',
      status: 0,
      detail: cause instanceof Error ? cause.message : String(cause),
    });
  }
  if (!outcome.response.ok) {
    throw ApiError.from(outcome.error, outcome.response.status);
  }
  return outcome.data as T;
}

/** Whether an error is the daemon saying the session is gone. */
export function isUnauthorized(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401;
}
