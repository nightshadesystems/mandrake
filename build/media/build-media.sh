#!/bin/bash
#
# Build Mandrake install media with kayak on the OmniOS build host.
#
#   build-media.sh [-n] [-o DIR] <target>...
#
#   zfs        installed-system ZFS stream          (kayak install-web)
#   miniroot   installer ramdisk                    (kayak install-tftp)
#   iso        BIOS+UEFI hybrid ISO                 (kayak install-iso)
#   usb        dd-able USB image                    (kayak install-usb)
#   pxe        PXE tarball: pxeboot, unix, miniroot, loader config, stream
#   all        iso usb pxe
#   clean      destroy the kayak build dataset and the staging directory
#
#   -n         stage the overlays and print what would run, then stop
#   -o DIR     output directory (default build/out)
#
# Mandrake reaches the images only through kayak's overlay hooks
# (ZFS_CUSTOM_OVERLAY, MINIROOT_CUSTOM_OVERLAY; ADR-0005). Nothing under
# build/kayak is modified by this script. Must run as root: kayak needs it
# for lofi, newfs, and mkisofs.

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
kayak=$repo/build/kayak

# shellcheck source=mandrake.env
. "$here/mandrake.env"

: "${STAGE_DIR:=/var/tmp/mandrake-media}"
: "${BUILDSEND:=}"                # kayak default: rpool/kayak_image, or <zone root>/kayak_image
: "${BUILDSEND_MP:=/kayak_image}" # kayak's fixed mountpoint
: "${PKGURL:=$OMNIOS_PKGURL}"
: "${PREBUILT_ILLUMOS:=}"
# Local repository with the Mandrake packages (just build-packages). When it
# holds system/mandrake/daemon the packages go onto the image (ADR-0010).
: "${MANDRAKE_BUILD_REPO:=$repo/build/out/repo}"
with_packages=0

VERSION=$OMNIOS_RELEASE
stem=mandrake-$MANDRAKE_VERSION-$OMNIOS_RELEASE
out_dir=$repo/build/out
dry_run=0

die() { echo "build-media: $*" >&2; exit 1; }
log() { printf '\n=== %s\n' "$*"; }
usage() { sed -n '3,20p' "$0"; }

while getopts 'no:h' opt; do
    case $opt in
        n) dry_run=1 ;;
        o) out_dir=$OPTARG ;;
        h) usage; exit 0 ;;
        *) usage; exit 2 ;;
    esac
done
shift $((OPTIND - 1))
[ $# -gt 0 ] || { usage; exit 2; }

want_zfs=0 want_miniroot=0 want_iso=0 want_usb=0 want_pxe=0 want_clean=0
for t in "$@"; do
    case $t in
        zfs) want_zfs=1 ;;
        miniroot) want_miniroot=1 ;;
        iso) want_iso=1 ;;
        usb) want_usb=1 ;;
        pxe) want_pxe=1 ;;
        all) want_iso=1 want_usb=1 want_pxe=1 ;;
        clean) want_clean=1 ;;
        *) die "unknown target '$t'" ;;
    esac
done
((want_usb)) && want_iso=1
((want_iso || want_pxe)) && want_zfs=1 want_miniroot=1

check_host() {
    [ "$(uname -s)" = SunOS ] || die "run this on the OmniOS build host"
    local id ver t
    id=$(awk -F= '$1 == "ID" { print $2 }' /etc/os-release)
    ver=$(awk -F= '$1 == "VERSION" { print $2 }' /etc/os-release)
    [ "$id" = omnios ] || die "host is not OmniOS (ID=$id)"
    [ "$ver" = "$OMNIOS_RELEASE" ] \
        || die "host runs OmniOS $ver but media targets $OMNIOS_RELEASE;" \
               "kayak takes boot files from the running system"
    [ "$(id -u)" = 0 ] || die "must run as root"
    [ -f "$kayak/Makefile" ] || die "kayak submodule missing; run 'just vendor'"
    grep -q apply_custom_overlay "$kayak/lib/utils.sh" \
        || die "kayak checkout lacks overlay support;" \
               "the submodule must point at the fork's mandrake/$OMNIOS_RELEASE branch"
    for t in gmake pv xz pkg zfs lofiadm mkisofs digest; do
        command -v "$t" >/dev/null || die "missing tool: $t"
    done
}

# Copy every regular file under $1 (except README.md) into $2, keeping paths.
copy_tree() {
    local src=$1 dst=$2 f
    (cd "$src" && find . -type f ! -name README.md -print) | while read -r f; do
        mkdir -p "$dst/$(dirname "$f")"
        cp -p "$src/$f" "$dst/$f"
    done
}

stage_overlays() {
    log "staging overlays under $STAGE_DIR"
    rm -rf "$STAGE_DIR/zfs" "$STAGE_DIR/miniroot"
    local zfs=$STAGE_DIR/zfs mini=$STAGE_DIR/miniroot
    mkdir -p "$zfs/etc" "$zfs/boot/forth" "$zfs/boot/conf.d" "$zfs/.overlay-hooks" \
             "$mini/boot/forth" "$mini/boot/conf.d"

    # Installed system: overlay/ verbatim, then branding, then hooks.
    copy_tree "$repo/overlay" "$zfs"
    cp -p "$repo/branding/motd" "$zfs/etc/motd"
    cp -p "$repo"/branding/loader/*.4th "$zfs/boot/forth/"
    cp -p "$repo/branding/loader/conf.d/mandrake" "$zfs/boot/conf.d/mandrake"
    cp -p "$here"/hooks/*.sh "$here/mandrake.env" "$zfs/.overlay-hooks/"
    if [ -f "$MANDRAKE_BUILD_REPO/pkg5.repository" ] \
        && pkgrepo list -s "$MANDRAKE_BUILD_REPO" -H system/mandrake/daemon >/dev/null 2>&1; then
        with_packages=1
        echo "MANDRAKE_BUILD_REPO=file://$MANDRAKE_BUILD_REPO/" > "$zfs/.overlay-hooks/build.env"
        echo "Mandrake packages: from $MANDRAKE_BUILD_REPO"
    else
        echo "Mandrake packages: none found at $MANDRAKE_BUILD_REPO; building branded media only"
    fi

    # Installer ramdisk (and so the ISO root): loader branding only, with
    # the install-media menu title.
    cp -p "$repo"/branding/loader/*.4th "$mini/boot/forth/"
    sed "s/^loader_menu_title=.*/loader_menu_title=\"$LOADER_MENU_TITLE\"/" \
        "$repo/branding/loader/conf.d/mandrake" > "$mini/boot/conf.d/mandrake"

    # Modes and ownership as the packaged neighbours have them.
    chmod 0444 "$zfs"/boot/forth/*.4th "$mini"/boot/forth/*.4th
    chmod 0644 "$zfs/etc/motd" "$zfs/boot/conf.d/mandrake" "$mini/boot/conf.d/mandrake"
    chmod 0755 "$zfs"/.overlay-hooks/*.sh
    if [ "$(id -u)" = 0 ]; then
        chown -R root:sys "$zfs/etc" "$zfs/boot" "$mini/boot"
    fi

    (cd "$STAGE_DIR" && find zfs miniroot -type f | sort)
}

kayak_targets() {
    local t=()
    if ((want_usb)); then
        t+=(install-usb)
    elif ((want_iso)); then
        t+=(install-iso)
    else
        ((want_zfs)) && t+=(install-web)
        ((want_miniroot)) && t+=(install-tftp)
    fi
    echo "${t[@]}"
}

run_kayak() {
    local -a vars=("VERSION=$VERSION" "BUILDSEND_MP=$BUILDSEND_MP")
    [ -n "$BUILDSEND" ] && vars+=("BUILDSEND=$BUILDSEND")
    [ -n "$PREBUILT_ILLUMOS" ] && vars+=("PREBUILT_ILLUMOS=$PREBUILT_ILLUMOS")
    export PKGURL VERSION
    export ZFS_CUSTOM_OVERLAY=$STAGE_DIR/zfs
    export MINIROOT_CUSTOM_OVERLAY=$STAGE_DIR/miniroot
    log "kayak: gmake ${vars[*]} $*"
    echo "    PKGURL=$PKGURL"
    echo "    ZFS_CUSTOM_OVERLAY=$ZFS_CUSTOM_OVERLAY"
    echo "    MINIROOT_CUSTOM_OVERLAY=$MINIROOT_CUSTOM_OVERLAY"
    if ((dry_run)); then
        return 0
    fi
    (cd "$kayak" && gmake "${vars[@]}" "$@")
}

verify_zfs() {
    local root=$BUILDSEND_MP/$VERSION
    log "verifying installed-system image at $root"
    [ -d "$root/etc" ] || die "image root $root not found"
    # No grep -q on a pipeline: an early exit would trip pipefail.
    pkg -R "$root" publisher -H | awk '{ print $1 }' | grep -x "$MANDRAKE_PUBLISHER" >/dev/null \
        || die "publisher $MANDRAKE_PUBLISHER missing from the image;" \
               "the overlay hook failed, see the kayak output above"
    grep -q Mandrake "$root/etc/motd" || die "/etc/motd is not branded"
    [ -f "$root/boot/conf.d/mandrake" ] || die "/boot/conf.d/mandrake missing"
    [ -f "$root/boot/forth/brand-mandrake.4th" ] || die "brand-mandrake.4th missing"
    if ((with_packages)); then
        pkg -R "$root" list -H system/mandrake/daemon system/mandrake/cli >/dev/null \
            || die "Mandrake packages are not installed in the image; see the hook output above"
        [ -f "$root/lib/svc/manifest/system/mandrake/mandraked.xml" ] || die "SMF manifest missing"
        echo "Mandrake packages installed:"
        pkg -R "$root" list -H system/mandrake/daemon system/mandrake/cli
    fi
    pkg -R "$root" publisher
}

verify_iso() {
    local iso=$BUILDSEND_MP/$VERSION.iso mnt=$STAGE_DIR/iso.mnt dev ok=1
    log "verifying ISO $iso"
    dev=$(lofiadm -a "$iso")
    mkdir -p "$mnt"
    mount -F hsfs -o ro "$dev" "$mnt"
    [ -f "$mnt/boot/conf.d/mandrake" ] || ok=0
    [ -f "$mnt/boot/forth/brand-mandrake.4th" ] || ok=0
    [ -f "$mnt/boot/forth/logo-mandrake.4th" ] || ok=0
    umount "$mnt"
    lofiadm -d "$dev"
    ((ok)) || die "ISO root lacks the Mandrake loader files; the miniroot overlay did not apply"
    echo "ISO carries the Mandrake loader branding"
}

build_pxe() {
    local pxe=$STAGE_DIR/pxe/$stem-pxe
    log "assembling PXE tarball"
    rm -rf "$STAGE_DIR/pxe"
    mkdir -p "$pxe/http/kayak"
    cp -rp "$BUILDSEND_MP/tftpboot" "$pxe/tftpboot"
    # kayak fills tftpboot/boot/forth from the pre-overlay miniroot dataset,
    # and TFTP cannot enumerate /boot/conf.d, so add the branding by hand.
    cp -p "$repo"/branding/loader/*.4th "$pxe/tftpboot/boot/forth/"
    chmod 0444 "$pxe"/tftpboot/boot/forth/*-mandrake.4th
    {
        echo
        echo "# Mandrake branding (see branding/loader/conf.d/mandrake)"
        sed -e '/^#/d' -e '/^$/d' \
            -e "s/^loader_menu_title=.*/loader_menu_title=\"$LOADER_MENU_TITLE\"/" \
            "$repo/branding/loader/conf.d/mandrake"
    } >> "$pxe/tftpboot/boot/loader.conf.local"
    # loader.conf.local boot-args fetch the stream from http://<next-server>/kayak/
    cp -p "$BUILDSEND_MP/kayak_$VERSION.zfs.xz" "$pxe/http/kayak/omnios-$VERSION.zfs.xz"
    cat > "$pxe/README" <<EOT
Mandrake $MANDRAKE_VERSION PXE install set (OmniOS $OMNIOS_RELEASE)

tftpboot/   serve over TFTP; the DHCP bootfile is pxeboot
http/       serve over HTTP at the DHCP next-server, or edit boot-args in
            tftpboot/boot/loader.conf.local to name the server

Unattended installs need a kayak answer file under http/kayak/, named by the
target's MAC address; that arrives in Phase 6. See docs/build.md.
EOT
    tar -cf - -C "$STAGE_DIR/pxe" "$stem-pxe" | gzip -9 > "$out_dir/$stem-pxe.tar.gz"
}

collect() {
    log "collecting outputs into $out_dir"
    mkdir -p "$out_dir"
    ((want_zfs)) && cp -p "$BUILDSEND_MP/kayak_$VERSION.zfs.xz" "$out_dir/$stem.zfs.xz"
    ((want_iso)) && cp -p "$BUILDSEND_MP/$VERSION.iso" "$out_dir/$stem.iso"
    ((want_usb)) && cp -p "$BUILDSEND_MP/$VERSION.usb-dd" "$out_dir/$stem.usb"
    ((want_pxe)) && build_pxe
    (cd "$out_dir" && digest -v -a sha256 "$stem".* > "$stem.sha256")
    ls -lh "$out_dir"
}

if ((want_clean)); then
    log "cleaning"
    if ((dry_run)); then
        echo "would run: gmake zfsdestroy in $kayak; rm -rf $STAGE_DIR"
    else
        (cd "$kayak" && gmake zfsdestroy)
        rm -rf "$STAGE_DIR"
    fi
    exit 0
fi

((dry_run)) || check_host
stage_overlays
# shellcheck disable=SC2046 # kayak targets are single words
run_kayak $(kayak_targets)
if ((dry_run)); then
    log "dry run: nothing built"
    exit 0
fi
((want_zfs)) && verify_zfs
((want_iso)) && verify_iso
collect
log "done"
