#!/bin/bash
#
# Capture the illumos tool output the driver parsers are tested against
# (ADR-0011). Run on an OmniOS host with the pools, links, and datasets
# you want represented; commit the results under crates/*/testdata/ and
# delete the matching *.synthetic.txt files.
#
#   capture-testdata.sh [-o DIR] [zfs|net|all]
#
# Each capture is written verbatim to <name>.<hostname>.txt with a
# <name>.<hostname>.txt.meta sidecar recording host, release, date, and
# the exact command. Nothing here mutates the system.

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
what=all
out_root=$repo/crates

while getopts 'o:h' opt; do
    case $opt in
        o) out_root=$OPTARG ;;
        h) sed -n '3,13p' "$0"; exit 0 ;;
        *) sed -n '3,13p' "$0"; exit 2 ;;
    esac
done
shift $((OPTIND - 1))
[ $# -gt 0 ] && what=$1

[ "$(uname -s)" = SunOS ] || { echo "capture-testdata: run this on an OmniOS host" >&2; exit 1; }
host=$(hostname)
release=$(awk -F= '$1 == "VERSION" { print $2 }' /etc/os-release)
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# capture CRATE NAME COMMAND...: run COMMAND, save stdout and a sidecar.
capture() {
    local crate=$1 name=$2
    shift 2
    local dir=$out_root/$crate/testdata
    mkdir -p "$dir"
    local file=$dir/$name.$host.txt
    if "$@" > "$file" 2> "$file.stderr"; then
        rm -f "$file.stderr"
    else
        echo "  (non-zero exit; stderr kept in $file.stderr)" >&2
    fi
    {
        echo "host: $host"
        echo "release: $release"
        echo "captured: $now"
        echo "command: $*"
    } > "$file.meta"
    echo "captured $crate/testdata/$name.$host.txt ($(wc -l < "$file") lines)"
}

zfs_columns=name,type,mountpoint,mounted,used,available,referenced,logicalused,quota,reservation,compression,compressratio,atime,recordsize,volsize,volblocksize,origin,creation,nightshade.systems:mandrake-id
snap_columns=name,used,referenced,creation,clones,nightshade.systems:mandrake-id

capture_zfs() {
    capture mandrake-zfs zpool-list-Hp zpool list -Hp -o name,size,allocated,free,fragmentation,capacity,dedupratio,health
    for pool in $(zpool list -H -o name); do
        capture mandrake-zfs "zpool-status.$pool" zpool status "$pool"
    done
    capture mandrake-zfs zfs-list-Hp zfs list -Hp -t filesystem,volume -s name -o "$zfs_columns"
    capture mandrake-zfs zfs-list-snapshots-Hp zfs list -Hp -t snapshot -s creation -o "$snap_columns"
    capture mandrake-zfs diskinfo-Hp diskinfo -Hp
    capture mandrake-zfs beadm-list-H beadm list -H
}

capture_net() {
    capture mandrake-net dladm-show-link-p dladm show-link -p -o link,class,mtu,state,bridge,over
    capture mandrake-net dladm-show-phys-p dladm show-phys -p -o link,media,state,speed,duplex,device
    capture mandrake-net dladm-show-phys-m-p dladm show-phys -m -p -o link,slot,address,inuse,client
    capture mandrake-net dladm-show-aggr-p dladm show-aggr -p -o link,policy,addrpolicy,lacpactivity,lacptimer,flags
    capture mandrake-net dladm-show-aggr-x-p dladm show-aggr -x -p -o link,port,speed,duplex,state,address,portstate
    capture mandrake-net dladm-show-vlan-p dladm show-vlan -p -o link,vid,over,flags
    capture mandrake-net dladm-show-etherstub-p dladm show-etherstub -p -o link
    capture mandrake-net dladm-show-vnic-p dladm show-vnic -p -o link,over,speed,macaddress,macaddrtype,vid,zone
    capture mandrake-net dladm-show-linkprop-mtu-p dladm show-linkprop -p mtu -p -o link,value
    capture mandrake-net ipadm-show-if-p ipadm show-if -p -o ifname,class,state,current,persistent,over
    capture mandrake-net ipadm-show-addr-p ipadm show-addr -p -o addrobj,type,state,current,persistent,addr
    capture mandrake-net netstat-rn netstat -rn
    capture mandrake-net route-p-show route -p show
}

case $what in
    zfs) capture_zfs ;;
    net) capture_net ;;
    all) capture_zfs; capture_net ;;
    *) echo "capture-testdata: unknown set '$what' (zfs|net|all)" >&2; exit 2 ;;
esac
echo "done; review the files, then commit them and delete the *.synthetic.txt they replace"
