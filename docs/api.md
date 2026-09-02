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

Surface so far: `health`, `auth`, `system`, `users`, `tokens`, `audit`,
`jobs`, `events` (Phase 2); `storage` and `network` (Phase 3). Later
families have tags but no paths yet.

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
| GET | `/storage/devices` | [Disks on the host](#listdevices) |
| GET | `/storage/pools` | [List pools](#listpools) |
| POST | `/storage/pools` | [Create a data pool](#createpool) |
| GET | `/storage/pools/{id}` | [Pool with vdev layout, health, and scan status](#getpool) |
| PATCH | `/storage/pools/{id}` | [Update pool metadata](#updatepool) |
| DELETE | `/storage/pools/{id}` | [Destroy a pool](#destroypool) |
| POST | `/storage/pools/{id}/scrub` | [Start a scrub](#startscrub) |
| DELETE | `/storage/pools/{id}/scrub` | [Stop a running scrub](#stopscrub) |
| GET | `/storage/datasets` | [List filesystems and volumes](#listdatasets) |
| POST | `/storage/datasets` | [Create a filesystem or volume](#createdataset) |
| GET | `/storage/datasets/{id}` | [Get a dataset](#getdataset) |
| PATCH | `/storage/datasets/{id}` | [Change properties or metadata](#updatedataset) |
| DELETE | `/storage/datasets/{id}` | [Destroy a dataset](#destroydataset) |
| GET | `/storage/volumes` | [List volumes (zvols)](#listvolumes) |
| GET | `/storage/snapshots` | [List snapshots](#listsnapshots) |
| POST | `/storage/snapshots` | [Take a snapshot](#createsnapshot) |
| GET | `/storage/snapshots/{id}` | [Get a snapshot](#getsnapshot) |
| DELETE | `/storage/snapshots/{id}` | [Destroy a snapshot](#destroysnapshot) |
| POST | `/storage/snapshots/{id}/rollback` | [Roll the dataset back to this snapshot](#rollbacksnapshot) |
| POST | `/storage/snapshots/{id}/clone` | [Clone a snapshot into a new dataset](#clonesnapshot) |
| GET | `/network/links` | [List every datalink](#listlinks) |
| GET | `/network/links/{id}` | [Get a datalink](#getlink) |
| PATCH | `/network/links/{id}` | [Change MTU or metadata](#updatelink) |
| POST | `/network/aggrs` | [Create a link aggregation](#createaggr) |
| DELETE | `/network/aggrs/{id}` | [Delete an aggregation](#deleteaggr) |
| POST | `/network/vlans` | [Create a VLAN link](#createvlan) |
| DELETE | `/network/vlans/{id}` | [Delete a VLAN link](#deletevlan) |
| POST | `/network/etherstubs` | [Create an etherstub](#createetherstub) |
| DELETE | `/network/etherstubs/{id}` | [Delete an etherstub](#deleteetherstub) |
| POST | `/network/vnics` | [Create a VNIC](#createvnic) |
| DELETE | `/network/vnics/{id}` | [Delete a VNIC](#deletevnic) |
| GET | `/network/addresses` | [List IP addresses](#listaddresses) |
| POST | `/network/addresses` | [Add an address to a link](#createaddress) |
| GET | `/network/addresses/{id}` | [Get an address](#getaddress) |
| DELETE | `/network/addresses/{id}` | [Remove an address](#deleteaddress) |
| GET | `/network/routes` | [List routes](#listroutes) |
| POST | `/network/routes` | [Add a persistent static route](#createroute) |
| DELETE | `/network/routes/{id}` | [Remove a static route](#deleteroute) |

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

## network

Crossbow objects: links, vnics, aggrs, vlans, etherstubs, addresses, routes

### listLinks

`GET /network/links`: List every datalink.

Physical links, aggrs, VLANs, etherstubs, and VNICs in one list with
their `kind` and what they sit `over`, enough to draw the topology.
Any role.

| Status | Body | Description |
|---|---|---|
| 200 | `LinkList` | Links |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getLink

`GET /network/links/{id}`: Get a datalink.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |

| Status | Body | Description |
|---|---|---|
| 200 | `Link` | The link |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### updateLink

`PATCH /network/links/{id}`: Change MTU or metadata.

Role `operator`. MTU changes apply with `dladm set-linkprop`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `LinkUpdate`

| Status | Body | Description |
|---|---|---|
| 200 | `Link` | Updated |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createAggr

`POST /network/aggrs`: Create a link aggregation.

Role `operator`. `dladm create-aggr`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `AggrCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `Link` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### deleteAggr

`DELETE /network/aggrs/{id}`: Delete an aggregation.

Role `operator`. Refused while VLANs or VNICs sit over it, or when protected.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Deleted |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createVlan

`POST /network/vlans`: Create a VLAN link.

Role `operator`. `dladm create-vlan`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `VlanCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `Link` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### deleteVlan

`DELETE /network/vlans/{id}`: Delete a VLAN link.

Role `operator`. Refused while VNICs sit over it, or when protected.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Deleted |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createEtherstub

`POST /network/etherstubs`: Create an etherstub.

Role `operator`. `dladm create-etherstub`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: object

| Status | Body | Description |
|---|---|---|
| 201 | `Link` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### deleteEtherstub

`DELETE /network/etherstubs/{id}`: Delete an etherstub.

Role `operator`. Refused while VNICs sit over it.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Deleted |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createVnic

`POST /network/vnics`: Create a VNIC.

Role `operator`. `dladm create-vnic`; MAC auto unless pinned.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `VnicCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `Link` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### deleteVnic

`DELETE /network/vnics/{id}`: Delete a VNIC.

Role `operator`. Refused when it carries addresses, belongs to a zone, or is protected.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Deleted |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### listAddresses

`GET /network/addresses`: List IP addresses.

Every address object from `ipadm show-addr`. Any role.

| Status | Body | Description |
|---|---|---|
| 200 | `AddressList` | Addresses |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createAddress

`POST /network/addresses`: Add an address to a link.

Role `operator`. Creates the IP interface first when the link has
none. Static, DHCP, or IPv6 autoconf.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `AddressCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `Address` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getAddress

`GET /network/addresses/{id}`: Get an address.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |

| Status | Body | Description |
|---|---|---|
| 200 | `Address` | The address |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### deleteAddress

`DELETE /network/addresses/{id}`: Remove an address.

Role `operator`. The management address is protected.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Deleted |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### listRoutes

`GET /network/routes`: List routes.

The routing table; only static routes are managed. Any role.

| Status | Body | Description |
|---|---|---|
| 200 | `RouteList` | Routes |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createRoute

`POST /network/routes`: Add a persistent static route.

Role `operator`. `route -p add`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `RouteCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `Route` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### deleteRoute

`DELETE /network/routes/{id}`: Remove a static route.

Role `operator`. Dynamic and interface routes are refused.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Deleted |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

## storage

ZFS pools, datasets, volumes, snapshots

### listDevices

`GET /storage/devices`: Disks on the host.

Every block device the host sees, with the pool that uses it when
one does. Source for the pool creation flow. Any role.

| Status | Body | Description |
|---|---|---|
| 200 | `DeviceList` | Devices |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### listPools

`GET /storage/pools`: List pools.

Any role. `rpool` is included and marked protected.

| Parameter | In | Type | Description |
|---|---|---|---|
| `cursor` | query | string | Opaque cursor from a previous page's next_cursor |
| `limit` | query | integer |  |

| Status | Body | Description |
|---|---|---|
| 200 | `PoolList` | Page of pools |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createPool

`POST /storage/pools`: Create a data pool.

Role `operator`. Runs `zpool create` with the given vdev layout.
Devices already in a pool are refused unless `force` is set.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `PoolCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `Pool` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getPool

`GET /storage/pools/{id}`: Pool with vdev layout, health, and scan status.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |

| Status | Body | Description |
|---|---|---|
| 200 | `Pool` | The pool |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### updatePool

`PATCH /storage/pools/{id}`: Update pool metadata.

Role `operator`. Only Mandrake metadata; pool properties are not exposed in this phase.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `Metadata`

| Status | Body | Description |
|---|---|---|
| 200 | `Pool` | Updated |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### destroyPool

`DELETE /storage/pools/{id}`: Destroy a pool.

Role `admin`. The body must echo the pool name (spec §7). `rpool`
is refused with the `protected` problem. Destroys every dataset in
the pool.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `PoolDestroy`

| Status | Body | Description |
|---|---|---|
| 204 |  | Destroyed |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### startScrub

`POST /storage/pools/{id}/scrub`: Start a scrub.

Role `operator`. Returns a job that tracks the scrub to completion.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 202 | `Job` | Operation started; poll the job or watch the event stream |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | A scrub or resilver is already running |
| default | `Problem` | Error as RFC 7807 problem details |

### stopScrub

`DELETE /storage/pools/{id}/scrub`: Stop a running scrub.

Role `operator`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Stopped |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### listDatasets

`GET /storage/datasets`: List filesystems and volumes.

Any role. Filter by pool, parent, or kind.

| Parameter | In | Type | Description |
|---|---|---|---|
| `pool` | query | string |  |
| `parent` | query | string | Only direct children of this dataset name |
| `kind` | query | `DatasetKind` |  |
| `cursor` | query | string | Opaque cursor from a previous page's next_cursor |
| `limit` | query | integer |  |

| Status | Body | Description |
|---|---|---|
| 200 | `DatasetList` | Page of datasets |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createDataset

`POST /storage/datasets`: Create a filesystem or volume.

Role `operator`. Volumes need `volsize_bytes`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `DatasetCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `Dataset` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getDataset

`GET /storage/datasets/{id}`: Get a dataset.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |

| Status | Body | Description |
|---|---|---|
| 200 | `Dataset` | The dataset |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### updateDataset

`PATCH /storage/datasets/{id}`: Change properties or metadata.

Role `operator`. Volumes may grow (`volsize_bytes` larger than now);
shrinking is refused. Mandrake metadata may be set on any dataset,
including protected ones.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `DatasetUpdate`

| Status | Body | Description |
|---|---|---|
| 200 | `Dataset` | Updated |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### destroyDataset

`DELETE /storage/datasets/{id}`: Destroy a dataset.

Role `operator`. Refused when the dataset has children or
snapshots unless `recursive` is set, or when it is protected.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |
| `recursive` | query | boolean | Also destroy children and snapshots (`zfs destroy -r`) |

| Status | Body | Description |
|---|---|---|
| 204 |  | Destroyed |
| 403 | `Problem` | Error as RFC 7807 problem details |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | Has children or snapshots and `recursive` was not set |
| default | `Problem` | Error as RFC 7807 problem details |

### listVolumes

`GET /storage/volumes`: List volumes (zvols).

The `kind=volume` view of datasets. Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `pool` | query | string |  |
| `cursor` | query | string | Opaque cursor from a previous page's next_cursor |
| `limit` | query | integer |  |

| Status | Body | Description |
|---|---|---|
| 200 | `DatasetList` | Page of volumes |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### listSnapshots

`GET /storage/snapshots`: List snapshots.

Any role. Filter by the dataset they belong to.

| Parameter | In | Type | Description |
|---|---|---|---|
| `dataset` | query | string | Dataset name; with `recursive` also descendants |
| `recursive` | query | boolean |  |
| `cursor` | query | string | Opaque cursor from a previous page's next_cursor |
| `limit` | query | integer |  |

| Status | Body | Description |
|---|---|---|
| 200 | `SnapshotList` | Page of snapshots |
| 401 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### createSnapshot

`POST /storage/snapshots`: Take a snapshot.

Role `operator`.

| Parameter | In | Type | Description |
|---|---|---|---|
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: `SnapshotCreate`

| Status | Body | Description |
|---|---|---|
| 201 | `Snapshot` | Created |
| 409 | `Problem` | Error as RFC 7807 problem details |
| 422 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### getSnapshot

`GET /storage/snapshots/{id}`: Get a snapshot.

Any role.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |

| Status | Body | Description |
|---|---|---|
| 200 | `Snapshot` | The snapshot |
| 404 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### destroySnapshot

`DELETE /storage/snapshots/{id}`: Destroy a snapshot.

Role `operator`. Refused when clones depend on it.

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

| Status | Body | Description |
|---|---|---|
| 204 |  | Destroyed |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### rollbackSnapshot

`POST /storage/snapshots/{id}/rollback`: Roll the dataset back to this snapshot.

Role `operator`. Refused when newer snapshots exist unless
`discard_newer` is set (`zfs rollback -r`).

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: object

| Status | Body | Description |
|---|---|---|
| 204 |  | Rolled back |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | Error as RFC 7807 problem details |
| default | `Problem` | Error as RFC 7807 problem details |

### cloneSnapshot

`POST /storage/snapshots/{id}/clone`: Clone a snapshot into a new dataset.

Role `operator`. How images become VM disks and zone roots (spec §7).

| Parameter | In | Type | Description |
|---|---|---|---|
| `id` (required) | path | `Id` |  |
| `Idempotency-Key` | header | string | Client-chosen key, scoped to the actor, kept 24 hours. A repeat with the same key and body returns the original response; a different body returns 422. |
| `X-Mandrake-Request` | header | `1` | Must be `1` on mutating requests authenticated by the session cookie |

Request body: object

| Status | Body | Description |
|---|---|---|
| 201 | `Dataset` | Created |
| 404 | `Problem` | Error as RFC 7807 problem details |
| 409 | `Problem` | Error as RFC 7807 problem details |
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

### Bytes

A size in bytes, exact (from `-p` parsable output)

Type: integer

### Device

A disk as `diskinfo` reports it

| Field | Type | Description |
|---|---|---|
| `name` (required) | string | Device name, for example `c1t0d0` |
| `vendor` | string |  |
| `product` | string |  |
| `serial` | string |  |
| `size_bytes` (required) | `Bytes` |  |
| `removable` (required) | boolean |  |
| `solid_state` | boolean |  |
| `pool` | string \| null | Pool using this device, if any |

### DeviceList

| Field | Type | Description |
|---|---|---|
| `items` (required) | array of `Device` |  |

### PoolHealth

Type: `ONLINE` \| `DEGRADED` \| `FAULTED` \| `OFFLINE` \| `UNAVAIL` \| `REMOVED` \| `SUSPENDED`

### VdevType

Type: `root` \| `disk` \| `file` \| `mirror` \| `raidz1` \| `raidz2` \| `raidz3` \| `log` \| `cache` \| `spare` \| `replacing` \| `spare-group`

### Vdev

One node of a pool's vdev tree

| Field | Type | Description |
|---|---|---|
| `name` (required) | string | As `zpool status` prints it (`mirror-0`, `c1t0d0`) |
| `type` (required) | `VdevType` |  |
| `state` (required) | `PoolHealth` |  |
| `read_errors` | integer |  |
| `write_errors` | integer |  |
| `checksum_errors` | integer |  |
| `note` | string | Trailing note such as `(resilvering)` |
| `children` (required) | array of `Vdev` |  |

### ScanStatus

The current or last scrub or resilver

| Field | Type | Description |
|---|---|---|
| `function` (required) | `scrub` \| `resilver` |  |
| `state` (required) | `in_progress` \| `finished` \| `canceled` |  |
| `progress` | number |  |
| `started_at` | `Timestamp` |  |
| `finished_at` | string \| null (date-time) |  |
| `errors` | integer |  |
| `rate_bytes_per_second` | integer |  |
| `summary` | string | The `scan:` line verbatim |

### Pool

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `name` (required) | string |  |
| `health` (required) | `PoolHealth` |  |
| `size_bytes` (required) | `Bytes` |  |
| `allocated_bytes` (required) | `Bytes` |  |
| `free_bytes` (required) | `Bytes` |  |
| `fragmentation_percent` | integer |  |
| `capacity_percent` | integer |  |
| `dedup_ratio` | number |  |
| `protected` (required) | boolean | `rpool`: no destroy or vdev changes through the API |
| `vdevs` (required) | `Vdev` |  |
| `scan` | `ScanStatus` |  |
| `status_text` | string | The `status:` and `action:` lines from `zpool status`, when present |
| `metadata` | `Metadata` |  |

### PoolList

Extends `Page`.

| Field | Type | Description |
|---|---|---|
| `items` | array of `Pool` |  |

### VdevSpec

One top-level vdev to create

| Field | Type | Description |
|---|---|---|
| `type` (required) | `stripe` \| `mirror` \| `raidz1` \| `raidz2` \| `raidz3` \| `log` \| `cache` \| `spare` |  |
| `devices` (required) | array of string |  |

### PoolCreate

| Field | Type | Description |
|---|---|---|
| `name` (required) | string |  |
| `vdevs` (required) | array of `VdevSpec` |  |
| `ashift` | integer | Default 12 |
| `compression` | string | Root dataset compression, default `lz4` |
| `force` | boolean | `zpool create -f`: reuse devices with old labels |
| `metadata` | `Metadata` |  |

### PoolDestroy

| Field | Type | Description |
|---|---|---|
| `name` (required) | string | Must equal the pool's name |

### DatasetKind

Type: `filesystem` \| `volume`

### Dataset

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `name` (required) | string | Full dataset name |
| `pool` (required) | string |  |
| `kind` (required) | `DatasetKind` |  |
| `mountpoint` | string \| null |  |
| `mounted` | boolean |  |
| `used_bytes` (required) | `Bytes` |  |
| `available_bytes` (required) | `Bytes` |  |
| `referenced_bytes` (required) | `Bytes` |  |
| `logical_used_bytes` | `Bytes` |  |
| `quota_bytes` | integer \| null |  |
| `reservation_bytes` | integer \| null |  |
| `compression` | string |  |
| `compress_ratio` | number |  |
| `atime` | boolean |  |
| `recordsize_bytes` | integer |  |
| `volsize_bytes` | integer \| null | Volumes only |
| `volblocksize_bytes` | integer \| null | Volumes only |
| `origin` | string \| null | Snapshot this dataset was cloned from |
| `protected` (required) | boolean | Root datasets, boot environments, and Mandrake's own datasets |
| `created_at` (required) | `Timestamp` |  |
| `metadata` | `Metadata` |  |

### DatasetCreate

| Field | Type | Description |
|---|---|---|
| `name` (required) | string | Full name, for example `tank/vms/disk0` |
| `kind` (required) | `DatasetKind` |  |
| `volsize_bytes` | integer | Required for volumes |
| `volblocksize_bytes` | integer |  |
| `compression` | string |  |
| `quota_bytes` | integer |  |
| `reservation_bytes` | integer |  |
| `mountpoint` | string |  |
| `atime` | boolean |  |
| `recordsize_bytes` | integer |  |
| `sparse` | boolean | Volumes: `zfs create -s` |
| `create_parents` | boolean | `zfs create -p` |
| `metadata` | `Metadata` |  |

### DatasetUpdate

Every field optional; omitted fields are unchanged

| Field | Type | Description |
|---|---|---|
| `volsize_bytes` | integer | Volumes: grow only |
| `compression` | string |  |
| `quota_bytes` | integer \| null | null removes the quota |
| `reservation_bytes` | integer \| null |  |
| `mountpoint` | string |  |
| `atime` | boolean |  |
| `metadata` | `Metadata` |  |

### DatasetList

Extends `Page`.

| Field | Type | Description |
|---|---|---|
| `items` | array of `Dataset` |  |

### Snapshot

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `name` (required) | string | Full name `dataset@snap` |
| `dataset` (required) | string |  |
| `short_name` (required) | string | The part after `@` |
| `used_bytes` (required) | `Bytes` |  |
| `referenced_bytes` (required) | `Bytes` |  |
| `clones` | array of string | Datasets cloned from this snapshot |
| `created_at` (required) | `Timestamp` |  |
| `metadata` | `Metadata` |  |

### SnapshotCreate

| Field | Type | Description |
|---|---|---|
| `dataset` (required) | string |  |
| `name` (required) | string | The part after `@` |
| `recursive` | boolean |  |
| `metadata` | `Metadata` |  |

### SnapshotList

Extends `Page`.

| Field | Type | Description |
|---|---|---|
| `items` | array of `Snapshot` |  |

### LinkName

A datalink name; illumos requires it to end in a digit

Type: string

### LinkKind

Type: `phys` \| `aggr` \| `vlan` \| `etherstub` \| `vnic` \| `other`

### LinkState

Type: `up` \| `down` \| `unknown`

### Link

One datalink. Fields not meaningful for a kind are absent.

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `name` (required) | string |  |
| `kind` (required) | `LinkKind` |  |
| `state` (required) | `LinkState` |  |
| `over` | array of string | Links this one sits on; several for an aggr, one otherwise |
| `mtu` | integer |  |
| `mac` | string | Colon-separated, lowercase |
| `mac_mode` | `auto` \| `fixed` \| `random` \| `factory` | VNICs |
| `vid` | integer | VLAN id for VLAN links and tagged VNICs |
| `speed_mbps` | integer | Physical links and aggrs |
| `duplex` | `full` \| `half` \| `unknown` |  |
| `device` | string | Driver instance behind a physical link, for example `e1000g0` |
| `media` | string |  |
| `aggr` | `AggrInfo` |  |
| `zone` | string \| null | Zone the link is assigned to, if not the global zone |
| `protected` (required) | boolean | Part of the path to the management address |
| `metadata` | `Metadata` |  |

### AggrInfo

| Field | Type | Description |
|---|---|---|
| `policy` (required) | string | `L2`, `L3`, `L4`, or combinations such as `L2,L3` |
| `lacp_mode` (required) | `off` \| `active` \| `passive` |  |
| `lacp_timer` (required) | `short` \| `long` |  |
| `ports` (required) | array of object |  |

### LinkList

| Field | Type | Description |
|---|---|---|
| `items` (required) | array of `Link` |  |

### LinkUpdate

| Field | Type | Description |
|---|---|---|
| `mtu` | integer |  |
| `metadata` | `Metadata` |  |

### AggrCreate

| Field | Type | Description |
|---|---|---|
| `name` (required) | `LinkName` |  |
| `ports` (required) | array of string |  |
| `policy` | string |  |
| `lacp_mode` | `off` \| `active` \| `passive` |  |
| `lacp_timer` | `short` \| `long` |  |
| `metadata` | `Metadata` |  |

### VlanCreate

| Field | Type | Description |
|---|---|---|
| `name` (required) | `LinkName` |  |
| `vid` (required) | integer |  |
| `over` (required) | string |  |
| `metadata` | `Metadata` |  |

### VnicCreate

| Field | Type | Description |
|---|---|---|
| `name` (required) | `LinkName` |  |
| `over` (required) | string |  |
| `mac` | string | Pin a MAC; omitted means auto |
| `vid` | integer |  |
| `mtu` | integer |  |
| `metadata` | `Metadata` |  |

### AddressKind

Type: `static` \| `dhcp` \| `addrconf`

### Address

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `name` (required) | string | The address object, for example `vnic0/v4` |
| `interface` (required) | string | The link (IP interface) it belongs to |
| `kind` (required) | `AddressKind` |  |
| `family` (required) | `inet` \| `inet6` |  |
| `address` | string | `a.b.c.d/prefix` or `xx::/prefix`; absent until DHCP or autoconf assigns one |
| `state` (required) | string | `ok`, `tentative`, `duplicate`, `inaccessible`, `disabled`, ... |
| `persistent` | boolean |  |
| `protected` (required) | boolean | The address the daemon listens on |
| `metadata` | `Metadata` |  |

### AddressList

| Field | Type | Description |
|---|---|---|
| `items` (required) | array of `Address` |  |

### AddressCreate

| Field | Type | Description |
|---|---|---|
| `interface` (required) | string | Link name; the IP interface is created if missing |
| `kind` (required) | `AddressKind` |  |
| `address` | string | Required for `static`, with prefix length |
| `alias` | string | The part after `/` in the address object name; default `v4` or `v6` |
| `temporary` | boolean | Do not persist across reboot |
| `metadata` | `Metadata` |  |

### Route

| Field | Type | Description |
|---|---|---|
| `id` (required) | `Id` |  |
| `destination` (required) | string | `default` or a network with prefix |
| `gateway` | string |  |
| `family` (required) | `inet` \| `inet6` |  |
| `interface` | string |  |
| `flags` | string | As `netstat -rn` prints them |
| `kind` (required) | `static` \| `interface` \| `dynamic` | Only `static` routes are managed |
| `persistent` | boolean |  |

### RouteList

| Field | Type | Description |
|---|---|---|
| `items` (required) | array of `Route` |  |

### RouteCreate

| Field | Type | Description |
|---|---|---|
| `destination` (required) | string | `default` or a network with prefix |
| `gateway` (required) | string |  |

