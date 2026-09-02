# Mandrake API reference

<!-- GENERATED FILE. Do not edit by hand. Regenerate with `just gen-api-docs`
     from api/openapi.yaml (console/scripts/gen-api-docs.mjs). -->

Version 0.1.0. Source of truth: [api/openapi.yaml](../api/openapi.yaml).

HTTP+JSON API served by `mandraked`. This file is the contract and the
source of truth for API shape (spec §6.3, §14). `docs/api.md` is generated
from it.

Conventions: plural nouns, UUID identifiers, cursor pagination,
`Idempotency-Key` on POST, `202` with a Job for long operations, RFC 7807
problem details for errors. Timestamps are RFC 3339 in UTC.

Authentication (ADR-0007): a session cookie issued by `POST /auth/login`
for the console, or a bearer token for API and CLI use. Requests that
mutate state and are authenticated by cookie must also send the header
`X-Mandrake-Request: 1`. The same API is served on the Unix socket
`/var/run/mandrake/mandraked.sock`, where uid 0 is trusted without auth
so `mandrakectl` works for recovery with no network (spec §9).

Roles: `viewer` reads; `operator` also mutates infrastructure; `admin`
also manages users, tokens, and the system. Each operation lists the
minimum role in its description.

Phase 2 surface: `health`, `auth`, `system`, `users`, `tokens`, `audit`,
`jobs`, `events`. Later families have tags but no paths yet.

## Endpoints

| Method | Path | Summary |
|---|---|---|
| GET | `/health` | [Liveness check](#gethealth) |
| POST | `/auth/login` | [Log in with username and password](#login) |
| POST | `/auth/logout` | [End the current session](#logout) |
| GET | `/auth/session` | [The current actor](#getsession) |
| GET | `/system` | [Host identity](#getsystem) |
| GET | `/system/resources` | [CPU, memory, and load](#getsystemresources) |
| GET | `/users` | [List users](#listusers) |
| POST | `/users` | [Create a user](#createuser) |
| GET | `/users/{id}` | [Get a user](#getuser) |
| PATCH | `/users/{id}` | [Update a user](#updateuser) |
| DELETE | `/users/{id}` | [Delete a user](#deleteuser) |
| PUT | `/users/{id}/password` | [Set a user's password](#setuserpassword) |
| GET | `/tokens` | [List tokens](#listtokens) |
| POST | `/tokens` | [Create a token](#createtoken) |
| GET | `/tokens/{id}` | [Get a token's metadata](#gettoken) |
| DELETE | `/tokens/{id}` | [Revoke a token](#deletetoken) |
| GET | `/audit` | [List audit entries, newest first](#listaudit) |
| GET | `/jobs` | [List jobs, newest first](#listjobs) |
| GET | `/jobs/{id}` | [Get a job](#getjob) |
| GET | `/events` | [Event stream (WebSocket)](#streamevents) |

## health

Liveness, unauthenticated

### getHealth

`GET /health`: Liveness check.

Returns 204 when the daemon is serving. No authentication, no body.

No authentication.

| Status | Body | Description |
|---|---|---|
| 204 |  | Alive |

## auth

Console login, logout, and the current session

### login

`POST /auth/login`: Log in with username and password.

Issues a session cookie. Five failures within fifteen minutes lock
the user for fifteen minutes; ten attempts a minute from one address
are rate limited.

No authentication.

Request body: `LoginRequest`

| Status | Body | Description |
|---|---|---|
| 200 | `Session` | Logged in; the session cookie is set |
| 401 | `Problem` | Error as RFC 7807 problem details |
| 423 | `Problem` | User locked after repeated failures |
| 429 | `Problem` | Rate limited |
| default | `Problem` | Error as RFC 7807 problem details |

### logout

`POST /auth/logout`: End the current session.

Deletes the session behind the cookie. Bearer tokens are not affected.

| Parameter | In | Type | Description |
|---|---|---|---|
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Session ended; the cookie is cleared |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getSession

`GET /auth/session`: The current actor.

Works for sessions, tokens, and the root socket. Any role.

| Status | Body | Description |
|---|---|---|
| 200 | `Session` | Current actor |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

## system

Host identity, time, resources, boot environments

### getSystem

`GET /system`: Host identity.

Any role.

| Status | Body | Description |
|---|---|---|
| 200 | `SystemInfo` | Host identity |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getSystemResources

`GET /system/resources`: CPU, memory, and load.

Point-in-time figures for dashboard gauges. Any role.

| Status | Body | Description |
|---|---|---|
| 200 | `SystemResources` | Resource figures |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

## users

Local users and roles

### listUsers

`GET /users`: List users.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `cursor` | query | string | Opaque cursor from a previous page's next_cursor |
| `limit` | query | integer |  |

| Status | Body | Description |
|---|---|---|
| 200 | `UserList` | Page of users |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createUser

`POST /users`: Create a user.

Role `admin`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `UserCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `User` | Created |
| 409 | `Problem` | Username already exists |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getUser

`GET /users/{id}`: Get a user.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |

| Status | Body | Description |
|---|---|---|
| 200 | `User` | The user |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### updateUser

`PATCH /users/{id}`: Update a user.

Role `admin`. An admin cannot remove their own `admin` role or
disable themselves; the last enabled admin cannot be disabled.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `UserUpdate`

| Status | Body | Description |
|---|---|---|
| 200 | `User` | Updated |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### deleteUser

`DELETE /users/{id}`: Delete a user.

Role `admin`. Ends the user's sessions and revokes their tokens.
Audit rows keep the username. The last enabled admin cannot be
deleted, nor can the caller delete themselves.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Deleted |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### setUserPassword

`PUT /users/{id}/password`: Set a user's password.

Any role for one's own account, with `current_password` required.
Role `admin` for any account, without `current_password`. Ends every
other session of that user.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `PasswordChange`

| Status | Body | Description |
|---|---|---|
| 204 |  | Password changed |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

## tokens

API bearer tokens

### listTokens

`GET /tokens`: List tokens.

Any role lists their own tokens. Role `admin` may pass `user_id` to
list another user's. Secrets are never returned.

| Parameter | In | Type | Description |
|---|---|---|---|
| `user_id` | query | `Id` |  |
| `cursor` | query | string | Opaque cursor from a previous page's next_cursor |
| `limit` | query | integer |  |

| Status | Body | Description |
|---|---|---|
| 200 | `TokenList` | Page of tokens |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createToken

`POST /tokens`: Create a token.

Any role creates a token for themselves; role `admin` may set
`user_id` to create one for another user. The secret is returned
once, in this response only.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `TokenCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `TokenCreated` | Created; `secret` is shown only here |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getToken

`GET /tokens/{id}`: Get a token's metadata.

Owner or role `admin`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |

| Status | Body | Description |
|---|---|---|
| 200 | `Token` | The token |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### deleteToken

`DELETE /tokens/{id}`: Revoke a token.

Owner or role `admin`. Takes effect immediately.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Revoked |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

## audit

Audit log of mutating calls

### listAudit

`GET /audit`: List audit entries, newest first.

Any role. Entries are immutable.

| Parameter | In | Type | Description |
|---|---|---|---|
| `actor_id` | query | `Id` |  |
| `object_id` | query | `Id` |  |
| `action` | query | string | Exact action name, for example `user.create` |
| `since` | query | string (date-time) |  |
| `until` | query | string (date-time) |  |
| `cursor` | query | string | Opaque cursor from a previous page's next_cursor |
| `limit` | query | integer |  |

| Status | Body | Description |
|---|---|---|
| 200 | `AuditList` | Page of audit entries |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

## jobs

Long-running operations

### listJobs

`GET /jobs`: List jobs, newest first.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `state` | query | `JobState` |  |
| `cursor` | query | string | Opaque cursor from a previous page's next_cursor |
| `limit` | query | integer |  |

| Status | Body | Description |
|---|---|---|
| 200 | `JobList` | Page of jobs |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getJob

`GET /jobs/{id}`: Get a job.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |

| Status | Body | Description |
|---|---|---|
| 200 | `Job` | The job |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

## events

Event stream over WebSocket

### streamEvents

`GET /events`: Event stream (WebSocket).

Upgrade to a WebSocket. Each text frame from the server is one
`Event` as JSON. Authenticate with the session cookie or with the
bearer token in the `Authorization` header of the upgrade request.
Optional `since` replays events with a greater id first. Any role.
Clients send nothing; pings are handled by the protocol.

| Parameter | In | Type | Description |
|---|---|---|---|
| `since` | query | string | Event id to resume after |

| Status | Body | Description |
|---|---|---|
| 101 |  | Switching Protocols |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

## Schemas

### Id

Globally unique object identifier, stored in illumos alongside the object

Type: string (uuid)

### Timestamp

RFC 3339, UTC

Type: string (date-time)

### Role

Type: `admin` \| `operator` \| `viewer`

### Problem

RFC 7807 problem details. `type` is `about:blank` for plain HTTP errors, or `https://mandrake.nightshade.systems/problems/<slug>` for application errors such as `invalid-credentials`, `locked`, `idempotency-mismatch`, `last-admin`.

| Field | Type | Description |
|---|---|---|
| `type` (required) | string (uri) |  |
| `title` (required) | string |  |
| `status` (required) | integer |  |
| `detail` | string |  |
| `instance` | string (uri) |  |
| `request_id` | string | Matches the daemon log and the audit row |

### Page

Cursor-paginated envelope; concrete list responses extend this with a typed items array

| Field | Type | Description |
|---|---|---|
| `items` (required) | array of any |  |
| `next_cursor` | string \| null | Pass as cursor to fetch the next page; null on the last page |

### LoginRequest

| Field | Type | Description |
|---|---|---|
| `username` (required) | string |  |
| `password` (required) | string (password) |  |

### Actor

Who is acting. `root` over the socket has no user id.

| Field | Type | Description |
|---|---|---|
| `id` | `Id` |  |
| `username` (required) | string |  |
| `role` (required) | `Role` |  |
| `via` (required) | `session` \| `token` \| `socket` |  |
| `token_id` | `Id` |  |

### Session

| Field | Type | Description |
|---|---|---|
| `actor` (required) | `Actor` |  |
| `expires_at` | `Timestamp` |  |
| `idle_expires_at` | `Timestamp` |  |

### User

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `username` (required) | string |  |
| `role` (required) | `Role` |  |
| `display_name` | string |  |
| `disabled` (required) | boolean |  |
| `locked_until` | string \| null (date-time) | Set while the user is locked out after failed logins |
| `last_login_at` | string \| null (date-time) |  |
| `created_at` (required) | `Timestamp` |  |
| `updated_at` (required) | `Timestamp` |  |

### UserCreate

| Field | Type | Description |
|---|---|---|
| `username` (required) | string | Lowercase, starts with a letter or underscore, at most 32 characters |
| `password` (required) | string (password) |  |
| `role` (required) | `Role` |  |
| `display_name` | string |  |

### UserUpdate

Every field optional; omitted fields are unchanged

| Field | Type | Description |
|---|---|---|
| `role` | `Role` |  |
| `display_name` | string |  |
| `disabled` | boolean |  |

### PasswordChange

| Field | Type | Description |
|---|---|---|
| `current_password` | string (password) | Required when changing one's own password |
| `new_password` (required) | string (password) |  |

### UserList

Extends `Page`.

| Field | Type | Description |
|---|---|---|
| `items` | array of `User` |  |

### Token

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `user_id` (required) | `Id` |  |
| `name` (required) | string |  |
| `prefix` (required) | string | First eight characters of the secret after `mdk_`, for identification |
| `created_at` (required) | `Timestamp` |  |
| `expires_at` | string \| null (date-time) |  |
| `last_used_at` | string \| null (date-time) |  |

### TokenCreate

| Field | Type | Description |
|---|---|---|
| `name` (required) | string |  |
| `user_id` | `Id` |  |
| `expires_in_seconds` | integer \| null | Omit or null for a token that does not expire |

### TokenCreated

Extends `Token`.

| Field | Type | Description |
|---|---|---|
| `secret` (required) | string | The full bearer token `mdk_...`; shown once |

### TokenList

Extends `Page`.

| Field | Type | Description |
|---|---|---|
| `items` | array of `Token` |  |

### ObjectRef

The object an audit entry or event is about

| Field | Type | Description |
|---|---|---|
| `kind` (required) | string | Resource family singular, for example `user`, `token`, `vm`, `system` |
| `id` | `Id` |  |
| `name` | string |  |

### AuditEntry

| Field | Type | Description |
|---|---|---|
| `id` (required) | string | Monotonic; usable as an audit cursor |
| `at` (required) | `Timestamp` |  |
| `actor` (required) | `Actor` |  |
| `action` (required) | string | `<kind>.<verb>`, for example `user.create`, `token.revoke`, `auth.login` |
| `object` (required) | `ObjectRef` |  |
| `before` | object \| null | Summary of the object before the call; secrets never appear |
| `after` | object \| null | Summary of the object after the call |
| `result` (required) | `ok` \| `denied` \| `failed` |  |
| `detail` | string |  |
| `request_id` | string |  |
| `source` | string | Client address, or `socket` |

### AuditList

Extends `Page`.

| Field | Type | Description |
|---|---|---|
| `items` | array of `AuditEntry` |  |

### JobState

Type: `queued` \| `running` \| `succeeded` \| `failed` \| `cancelled`

### Job

A long-running operation; returned with 202

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `state` (required) | `JobState` |  |
| `kind` (required) | string | Operation family, for example image.import or vm.create |
| `target` | `ObjectRef` |  |
| `progress` | number |  |
| `message` | string |  |
| `created_at` (required) | `Timestamp` |  |
| `started_at` | string \| null (date-time) |  |
| `finished_at` | string \| null (date-time) |  |
| `error` | `Problem` |  |

### JobList

Extends `Page`.

| Field | Type | Description |
|---|---|---|
| `items` | array of `Job` |  |

### Event

One frame on the event stream

| Field | Type | Description |
|---|---|---|
| `id` (required) | string | Monotonic; pass as `since` to resume |
| `at` (required) | `Timestamp` |  |
| `kind` (required) | string | `<kind>.<verb>` as in audit actions, plus `job.progress` |
| `object` | `ObjectRef` |  |
| `actor` | `Actor` |  |
| `data` | object | Kind-specific payload, for example the Job for `job.*` |

### Metadata

Per-object metadata held in SQLite, not in illumos (ADR-0002)

| Field | Type | Description |
|---|---|---|
| `display_name` | string |  |
| `description` | string |  |
| `tags` | array of string |  |
| `notes` | string |  |

### SystemInfo

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `hostname` (required) | string |  |
| `product` (required) | `mandrake` |  |
| `version` (required) | string | Mandrake release version |
| `omnios_release` (required) | string |  |
| `boot_environment` (required) | string | Active boot environment name |
| `uptime_seconds` (required) | integer |  |
| `time` (required) | `Timestamp` |  |
| `timezone` | string |  |

### SystemResources

| Field | Type | Description |
|---|---|---|
| `cpus` (required) | integer |  |
| `load_avg` (required) | array of number | 1, 5, and 15 minute load averages |
| `memory` (required) | object |  |
| `sampled_at` | `Timestamp` |  |

