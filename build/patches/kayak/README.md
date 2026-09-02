# kayak patches

Upstream kayak commits that the pinned release branch lacks, exported with
`git format-patch` from `omniosorg/kayak` master. They are applied to the
fork's `mandrake/r151054` branch, which is what the `build/kayak` submodule
points at (ADR-0004).

All of them add the overlay mechanism (`ZFS_CUSTOM_OVERLAY`,
`MINIROOT_CUSTOM_OVERLAY`, `.overlay-hooks/`) that Mandrake's media build
relies on (ADR-0005). Nothing here is Mandrake-specific; the fork stays
upstream code.

To (re)create the branch in the fork:

```sh
cd mandrake-kayak
git remote add upstream https://github.com/omniosorg/kayak.git 2>/dev/null
git fetch upstream
git checkout -B mandrake/r151054 upstream/r151054
git am ../mandrake/build/patches/kayak/*.patch
git push -u origin mandrake/r151054
```

When the OmniOS pin moves, check each patch against the new release branch
with `git apply --check`; drop the ones upstream has merged.
