# kayak patches

Upstream kayak commits that the pinned release branch lacks, exported with
`git format-patch` from `omniosorg/kayak` master. They are applied to the
fork's `mandrake/r151054` branch, which is what the `build/kayak` submodule
points at (ADR-0004).

0001 to 0005 add the overlay mechanism (`ZFS_CUSTOM_OVERLAY`,
`MINIROOT_CUSTOM_OVERLAY`, `.overlay-hooks/`) that Mandrake's media build
relies on (ADR-0005); they are upstream code. 0006 is Mandrake's own
(ADR-0014): it hooks `/kayak/lib/mandrake.sh` into the installer, names
the boot environment from `$BENAME`, runs `mandrake-config` from the
post-install menu, and brands the menu titles.

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
