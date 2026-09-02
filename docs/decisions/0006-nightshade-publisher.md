# ADR-0006: The nightshade.systems publisher: name, origin, and signing

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 1 (Media); packages arrive in Phase 2

## Context

ADR-0001 layers a second IPS publisher on top of `omnios`. Installed
systems must carry it from first boot so `pkg` can find Mandrake packages,
and the media build needs its origin URL before any package exists. IPS
requires an origin per publisher, so the layout of the repository behind
that URL has to be decided now, as does whether packages are signed.

OmniOS publishes `omnios` at `https://pkg.omnios.org/r151054/core`, with the
release in the path, because packages built against one release do not
install on another. OmniOS requires signatures on the `omnios` publisher.
`omnios-build` has no signing support of its own; signing is done to the
repository after publication.

## Decision

- Publisher name: `nightshade.systems`, as in the spec.
- Origin: `https://pkg.nightshade.systems/mandrake/<omnios release>/`, so
  `https://pkg.nightshade.systems/mandrake/r151054/` for the current pin.
  The product and the OmniOS release are both in the path; a future
  product or release gets its own repository rather than a shared one.
- The origin is a single value, `MANDRAKE_PUBLISHER_URL` in
  `build/media/mandrake.env`, and nothing else in the tree spells it out.
- Search order: `omnios` stays first and sticky, as kayak sets it.
  `nightshade.systems` is added second with `--no-refresh`, so media build
  without the origin being reachable.
- Signature policy on `nightshade.systems` is `verify`: signatures are
  checked when present and not required. Package signing is deferred until
  a signing key and its custody are decided, which gets its own ADR before
  the first public repository.
- The empty repository is created with `pkgrepo create` and
  `pkgrepo add-publisher` (`build/media/init-repo.sh`) and served with
  `pkg.depotd` for demos. Production hosting is not decided here.

## Consequences

- A fresh install lists both publishers and `pkg refresh` succeeds once the
  origin is served. Until then `pkg refresh` reports the origin
  unreachable for `nightshade.systems` and continues, which is acceptable
  for Phase 1.
- Packages published in Phase 2 are unsigned. Before the origin is public,
  the signing ADR must land and `signature-policy` may move to
  `require-signatures` in a media rebuild.
- Changing the origin later is one edit to `mandrake.env` for new media,
  plus `pkg set-publisher -G old -g new` on installed hosts, which the
  Phase 7 update flow can carry.
