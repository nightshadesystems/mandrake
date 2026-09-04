#!/bin/bash
#
# Boot-test the install media under bhyve on the OmniOS build host
# (spec §5 `just test-boot`): an unattended PXE install into a fresh VM
# from the PXE tarball, then wait for mandraked and run API smoke tests.
#
#   test-boot.sh [-k] [-n] [-t SECONDS]
#
#   -k   keep the VM, its disk, and the test network after the run
#   -n   print what would be done and stop
#   -t   overall timeout for install plus first boot (default 1800)
#
# What it sets up, all on a private etherstub so nothing touches a real
# network: a host VNIC with 10.211.0.1/24, dnsmasq for DHCP and TFTP with
# next-server pointing at the host, python3's http.server for the image
# and the answer file, a zvol for the VM's disk, and a bhyve VM with a
# UEFI bootrom that PXE-boots because the disk is blank and boots from the
# disk once the install has written it. The answer file is the sample
# from the tarball plus a static management address, so the smoke tests
# know where to connect.
#
# Must run as root. Needs: bhyve and its firmware, dnsmasq
# (ooce/network/dnsmasq), python3, curl, and a PXE tarball in build/out
# built with the packages (just build-packages; just build-pxe).

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

# shellcheck source=mandrake.env
. "$here/mandrake.env"

stem=mandrake-$MANDRAKE_VERSION-$OMNIOS_RELEASE
: "${PXE_TARBALL:=$repo/build/out/$stem-pxe.tar.gz}"
: "${WORK:=/var/tmp/mandrake-test-boot}"
: "${VM:=mandrake-test}"
: "${VM_MEM:=2G}"
: "${VM_CPUS:=2}"
: "${VM_DISK_SIZE:=20G}"
: "${VM_ZVOL:=rpool/$VM}"
: "${NET:=10.211.0}"
: "${STUB:=mdktest0}"
: "${HOST_VNIC:=mdktesth0}"
: "${GUEST_VNIC:=mdktestg0}"
: "${GUEST_MAC:=02:08:20:11:00:02}"
: "${BOOTROM:=/usr/share/bhyve/uefi-rom.bin}"
: "${DNSMASQ:=/opt/ooce/sbin/dnsmasq}"
: "${ADMIN_USER:=admin}"
: "${ADMIN_PW:=test-boot-passphrase}"
: "${HOSTNAME_GUEST:=mandrake-test}"

host_ip=$NET.1
guest_ip=$NET.2
mac_file=$(echo "$GUEST_MAC" | tr -d ':' | tr '[:lower:]' '[:upper:]')
keep=0 dry_run=0 timeout=1800
console_log=$WORK/console.log
pids=()

die() { echo "test-boot: $*" >&2; exit 1; }
log() { printf '\n=== %s\n' "$*"; }
usage() { sed -n '3,12p' "$0"; }

while getopts 'knt:h' opt; do
    case $opt in
        k) keep=1 ;;
        n) dry_run=1 ;;
        t) timeout=$OPTARG ;;
        h) usage; exit 0 ;;
        *) usage; exit 2 ;;
    esac
done

# ------------------------------------------------------------ checks

check_host() {
    [ "$(uname -s)" = SunOS ] || die "run this on the OmniOS build host"
    [ "$(id -u)" = 0 ] || die "must run as root"
    [ -f "$PXE_TARBALL" ] || die "PXE tarball not found: $PXE_TARBALL (just build-pxe)"
    [ -f "$BOOTROM" ] || die "bhyve firmware not found at $BOOTROM (pkg install system/bhyve/firmware)"
    [ -x "$DNSMASQ" ] || die "dnsmasq not found at $DNSMASQ (pkg install ooce/network/dnsmasq)"
    for t in bhyve bhyvectl dladm ipadm zfs python3 curl; do
        command -v "$t" >/dev/null || die "missing tool: $t"
    done
    if ! tar -tzf "$PXE_TARBALL" | grep -q "http/kayak/000000000000.sample"; then
        die "the tarball has no sample answer file; rebuild the media with Phase 6"
    fi
}

# ------------------------------------------------------------ pieces

setup_network() {
    log "test network $STUB: host $host_ip, guest $guest_ip"
    dladm show-etherstub "$STUB" >/dev/null 2>&1 || dladm create-etherstub -t "$STUB"
    dladm show-vnic "$HOST_VNIC" >/dev/null 2>&1 || dladm create-vnic -t -l "$STUB" "$HOST_VNIC"
    dladm show-vnic "$GUEST_VNIC" >/dev/null 2>&1 \
        || dladm create-vnic -t -l "$STUB" -m "$GUEST_MAC" "$GUEST_VNIC"
    ipadm show-if "$HOST_VNIC" >/dev/null 2>&1 || ipadm create-if -t "$HOST_VNIC"
    ipadm show-addr "$HOST_VNIC/v4" >/dev/null 2>&1 \
        || ipadm create-addr -t -T static -a "$host_ip/24" "$HOST_VNIC/v4"
}

unpack() {
    log "unpacking $PXE_TARBALL into $WORK"
    rm -rf "$WORK"
    mkdir -p "$WORK"
    tar -xzf "$PXE_TARBALL" -C "$WORK"
    pxe=$(ls -d "$WORK"/*-pxe)
    [ -d "$pxe/tftpboot" ] && [ -d "$pxe/http/kayak" ] || die "unexpected tarball layout under $WORK"
}

write_answer() {
    local f=$pxe/http/kayak/$mac_file
    log "answer file $f"
    cat > "$f" <<EOF
# Written by test-boot.sh (ADR-0014). Any disk of 8 GiB or more.
BuildRpool '>8000'
SetHostname $HOSTNAME_GUEST
SetTimezone UTC
UseDNS $host_ip test.mandrake
MandrakeAdmin $ADMIN_USER '$ADMIN_PW'
MandrakeMgmt vioif0 $guest_ip/24 $host_ip
EOF
    # The stream is fetched from install_media in loader.conf.local, which
    # points at http://<next-server>/kayak/; the sample shows the layout.
    ls "$pxe/http/kayak"
}

start_services() {
    log "dnsmasq (DHCP + TFTP) and http.server on $host_ip"
    mkdir -p "$WORK/log"
    "$DNSMASQ" --no-daemon --port=0 --log-dhcp \
        --interface="$HOST_VNIC" --bind-interfaces \
        --dhcp-range="$NET.100,$NET.150,2h" \
        --dhcp-option=option:router,"$host_ip" \
        --dhcp-option=option:dns-server,"$host_ip" \
        --dhcp-boot=pxeboot,,"$host_ip" \
        --enable-tftp --tftp-root="$pxe/tftpboot" \
        > "$WORK/log/dnsmasq.log" 2>&1 &
    pids+=($!)
    (cd "$pxe/http" && python3 -m http.server 80 --bind "$host_ip" > "$WORK/log/http.log" 2>&1) &
    pids+=($!)
    sleep 1
    for p in "${pids[@]}"; do
        kill -0 "$p" 2>/dev/null || die "a helper service exited; see $WORK/log"
    done
}

create_disk() {
    log "disk $VM_ZVOL ($VM_DISK_SIZE)"
    zfs list "$VM_ZVOL" >/dev/null 2>&1 && zfs destroy -r "$VM_ZVOL"
    zfs create -V "$VM_DISK_SIZE" -s "$VM_ZVOL"
}

# Run bhyve until the guest powers off or the deadline passes. Exit code 0
# from bhyve means the guest asked for a reboot: run it again, so the PXE
# install's reboot lands on the freshly written disk.
run_vm() {
    local deadline=$(( $(date +%s) + timeout )) rc
    log "bhyve $VM: $VM_CPUS vCPU, $VM_MEM, console in $console_log"
    : > "$console_log"
    while [ "$(date +%s)" -lt "$deadline" ]; do
        bhyvectl --vm="$VM" --destroy >/dev/null 2>&1 || true
        set +e
        bhyve -c "$VM_CPUS" -m "$VM_MEM" -H -w -A \
            -s 0,hostbridge \
            -s 1,lpc \
            -s 2,virtio-net-viona,"$GUEST_VNIC" \
            -s 3,virtio-blk,/dev/zvol/rdsk/"$VM_ZVOL" \
            -l com1,stdio \
            -l bootrom,"$BOOTROM" \
            "$VM" >> "$console_log" 2>&1 < /dev/null
        rc=$?
        set -e
        case $rc in
            0) echo "guest rebooted; starting it again" >> "$console_log" ;;
            1) echo "guest powered off" >> "$console_log"; return 0 ;;
            *) echo "bhyve exited with $rc" >> "$console_log"; return "$rc" ;;
        esac
    done
    return 0
}

wait_for_daemon() {
    local deadline=$(( $(date +%s) + timeout )) url=https://$guest_ip/api/v1/health
    log "waiting up to ${timeout}s for $url"
    while [ "$(date +%s)" -lt "$deadline" ]; do
        if curl -sk -m 5 -o /dev/null -w '%{http_code}' "$url" 2>/dev/null | grep -q '^200$'; then
            echo "mandraked answers"
            return 0
        fi
        if grep -q "INSTALLATION_FAILED\|RunInstall failed\|Answer file has no" "$console_log" 2>/dev/null; then
            die "the install failed; see $console_log"
        fi
        sleep 10
    done
    die "timed out waiting for mandraked; see $console_log"
}

smoke() {
    log "API smoke tests against $guest_ip"
    local base=https://$guest_ip/api/v1 jar=$WORK/cookies fails=0
    rm -f "$jar"
    step() {
        if "$@"; then echo "ok   $step_name"; else echo "FAIL $step_name"; fails=$((fails + 1)); fi
    }
    step_name="health is 200"
    step curl -sk -m 10 -f -o /dev/null "$base/health"
    step_name="login as $ADMIN_USER from the answer file"
    step curl -sk -m 10 -f -o /dev/null -c "$jar" -H 'content-type: application/json' \
        -d "{\"username\":\"$ADMIN_USER\",\"password\":\"$ADMIN_PW\"}" "$base/auth/login"
    step_name="session belongs to an admin"
    step sh -c "curl -sk -m 10 -f -b '$jar' '$base/auth/session' | grep -q '\"role\":\"admin\"'"
    step_name="system reports the hostname $HOSTNAME_GUEST"
    step sh -c "curl -sk -m 10 -f -b '$jar' '$base/system' | grep -q '\"hostname\":\"$HOSTNAME_GUEST\"'"
    step_name="audit has user.create by the installer"
    step sh -c "curl -sk -m 10 -f -b '$jar' '$base/audit?limit=50' | grep -q 'installer'"
    step_name="TLS fingerprint printed on the console"
    step grep -q "TLS certificate SHA-256 fingerprint" "$console_log"
    rm -f "$jar"
    [ "$fails" -eq 0 ] || die "$fails smoke test(s) failed"
    echo "all smoke tests passed"
}

teardown() {
    log "tearing down"
    bhyvectl --vm="$VM" --destroy >/dev/null 2>&1 || true
    for p in "${pids[@]:-}"; do
        [ -n "$p" ] && kill "$p" 2>/dev/null || true
    done
    if ((keep)); then
        echo "keeping $VM_ZVOL, $STUB, and $WORK (-k)"
        return 0
    fi
    zfs destroy -r "$VM_ZVOL" 2>/dev/null || true
    ipadm delete-addr "$HOST_VNIC/v4" 2>/dev/null || true
    ipadm delete-if "$HOST_VNIC" 2>/dev/null || true
    dladm delete-vnic "$HOST_VNIC" 2>/dev/null || true
    dladm delete-vnic "$GUEST_VNIC" 2>/dev/null || true
    dladm delete-etherstub "$STUB" 2>/dev/null || true
}

# ------------------------------------------------------------ main

if ((dry_run)); then
    cat <<EOF
would: unpack $PXE_TARBALL into $WORK
       create etherstub $STUB, host VNIC $HOST_VNIC $host_ip/24, guest VNIC $GUEST_VNIC ($GUEST_MAC)
       write http/kayak/$mac_file with MandrakeAdmin $ADMIN_USER and MandrakeMgmt vioif0 $guest_ip/24
       run dnsmasq (DHCP $NET.100-150, TFTP, next-server $host_ip) and http.server on :80
       create $VM_ZVOL ($VM_DISK_SIZE) and PXE-boot bhyve VM $VM with $BOOTROM
       wait up to ${timeout}s for https://$guest_ip/api/v1/health, then run the smoke tests
       tear down unless -k
EOF
    exit 0
fi

check_host
trap teardown EXIT
setup_network
unpack
write_answer
start_services
create_disk
run_vm &
vm_pid=$!
wait_for_daemon
smoke
log "done: install and first boot verified; console log in $console_log"
kill "$vm_pid" 2>/dev/null || true
