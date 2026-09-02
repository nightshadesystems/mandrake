# ADR-0007: Local users, sessions, tokens, and the root socket

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 2 (Daemon core)

## Context

Spec §6.1 and §12 fix the outline: local users with argon2 hashes, session
cookies for the console, bearer tokens for API and CLI, roles `admin`,
`operator`, `viewer`, tokens hashed at rest, lockout and rate limiting on
login, and an unauthenticated path over the Unix socket for root (§9). The
details below are what the spec leaves open and what the code depends on.

## Decision

**Users.** Stored in SQLite. Passwords are hashed with argon2id, 19 MiB
memory, 2 iterations, 1 lane, a 16-byte salt; the parameters travel in the
PHC string so they can be raised later without a migration. Minimum
password length 12, no composition rules. A user can be disabled, which
ends its sessions and refuses its tokens without deleting its audit trail.

**Roles.** `viewer` reads everything except other users' tokens.
`operator` additionally performs every infrastructure mutation (VMs,
zones, images, network, storage, jobs) and manages its own password and
tokens. `admin` additionally manages users, others' tokens, system
settings, and updates. Roles are checked per route; there is no
per-object ACL in v1.

**Sessions.** Server-side rows keyed by a 32-byte random id, sent as the
cookie `mandrake_session` with `HttpOnly`, `Secure`, `SameSite=Strict`,
`Path=/`. Idle timeout 12 hours, absolute lifetime 7 days, both stored per
row. Logout deletes the row. Every request that mutates state and is
authenticated by cookie must also carry the header
`X-Mandrake-Request: 1`; browsers do not add custom headers cross-site,
so this plus `SameSite` covers CSRF without per-form tokens.
Bearer-authenticated requests are exempt.

**Tokens.** `mdk_` followed by 32 random bytes in base64url, shown once at
creation. SQLite stores a SHA-256 of the whole string plus the first eight
characters after the prefix for display. Optional expiry; `last_used_at`
is updated at most once a minute. A token carries its owner's role at the
time of use, never a role of its own.

**Root socket.** The daemon also listens on
`/var/run/mandrake/mandraked.sock`, mode 0600 root. A peer whose
credentials say uid 0 is the synthetic actor `root` with role `admin` and
needs no session or token. Any other uid is rejected. This is the recovery
path and how the first admin is created.

**Lockout and rate limit.** Five failed logins for a user within fifteen
minutes lock the user for fifteen minutes; the lock is a timestamp on the
row and is reported as `423 Locked`. Independently, one source address may
attempt at most ten logins a minute; excess gets `429 Too Many Requests`.
Failures are counted before the password is checked so timing does not
reveal whether the user exists.

**Idempotency.** `Idempotency-Key` on POST is scoped to the actor and kept
24 hours with the request body hash and the response. A repeat with the
same key and body returns the stored response; a different body returns
`422`.

**Audit.** Every mutating call writes one row after the fact: time, actor
(user id, username, via `session`, `token`, or `socket`), action, object
kind, id, and name, a before and after summary as JSON, the result, and
the request id. Audit rows are never updated or deleted by the API.

## Consequences

- The console needs no CSRF token plumbing, only one constant header on
  its API client.
- No password reset flow exists in v1; an admin sets a new password, or
  root does over the socket.
- Session rows accumulate and are swept hourly; nothing else expires
  implicitly.
- Rate-limit state is in memory and resets on restart, acceptable for a
  single host.
- A later fleet layer that needs delegated identity (OIDC, SSO) adds a
  second authentication source; nothing here prevents it.
