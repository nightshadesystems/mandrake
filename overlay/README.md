# overlay

Files dropped verbatim onto the installed-system root, laid out as they
will appear under `/`. `build/media/build-media.sh` copies this tree into
kayak's `ZFS_CUSTOM_OVERLAY` (ADR-0005); this README is skipped.

Rules:

- Anything that belongs to a package goes in the package instead. This is
  for the handful of files that have no better home.
- Never overlay a file a package delivers unless that file is
  `preserve=true` in its manifest, or `pkg verify` and `pkg update` will
  fight it. Branding files that qualify live in `branding/`, not here.
- Ownership and modes are taken from the staging copy; set them in
  `build-media.sh` if the defaults (root, 0644) are wrong for a file.

Empty in Phase 1 apart from this README.
