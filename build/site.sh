# shellcheck shell=bash disable=SC2034 # sourced by omnios-build; every variable is read there
# omnios-build site configuration for Mandrake.
#
# build/packages/build-packages.sh copies this file to
# build/omnios-build/lib/site.sh (git-ignored there) before building.
# Anything from omnios-build/lib/config.sh may be overridden here.

# The publisher layered on top of omnios (ADR-0006).
PKGPUBLISHER=nightshade.systems

# Destination file repository; build-packages.sh sets MANDRAKE_REPO_DIR.
PKGSRVR=file://${MANDRAKE_REPO_DIR:-$ROOTDIR/../out/repo}/

# Used only for package-content diffs. Point at the published repository
# once one exists; until then diff against our own build repo.
IPS_REPO=$PKGSRVR

# Nightshade metadata on every package.
PUBLISHER_EMAIL=engineering@nightshade.systems

# Privilege escalation on the build host.
PFEXEC=pfexec

# A built illumos-omnios workspace is only needed for kernel-level packages
# Mandrake does not build. Leave unset.
#PREBUILT_ILLUMOS=

# Mandrake packages are never expensive.
EXPENSIVE=""
