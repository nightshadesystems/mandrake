# Mandrake API reference

<!-- GENERATED FILE. Do not edit by hand. Regenerate with `just gen-api-docs`
     from api/openapi.yaml. The generator is chosen in Phase 2. -->

This file is the human-readable rendering of [`api/openapi.yaml`](../api/openapi.yaml),
committed so reviewers can read the contract without tooling. The OpenAPI file
is the source of truth for API shape; the spec is the source of truth for
behaviour.

## Conventions

- REST over HTTPS. Base path `/api/v1`.
- Plural nouns, UUID identifiers, cursor pagination via `cursor` and `limit`.
- `POST` requests accept an `Idempotency-Key` header.
- Long-running operations return `202 Accepted` with a `Job` body.
- Errors are RFC 7807 problem details with content type
  `application/problem+json`.
- Authentication is a session cookie for the console or a bearer token for API
  and CLI use. The Unix socket accepts root without auth.

## Resources

| Family | Path prefix | Phase |
|---|---|---|
| System | `/system` | 2 |
| Users and tokens | `/users`, `/tokens` | 2 |
| Audit | `/audit` | 2 |
| Events | `/events` (WebSocket) | 2 |
| Jobs | `/jobs` | 2 |
| Network | `/network/{links,vnics,aggrs,vlans,etherstubs,addresses,routes}` | 3 |
| Storage | `/storage/{pools,datasets,volumes,snapshots}` | 3 |
| Images | `/images`, `/images/sources` | 4 |
| Zones | `/zones` | 4 |
| VMs | `/vms` | 5 |

## Endpoints

Only the stub endpoint exists in Phase 0.

### `GET /system`

Returns the host summary. See `SystemInfo` in the OpenAPI file.
