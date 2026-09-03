#!/bin/bash
#
# Mandrake answer-file verbs for the kayak installer (ADR-0014).
#
# Staged into the installer ramdisk as /kayak/lib/mandrake.sh and sourced
# at the end of kayak's install_help.sh, so the verbs are available to PXE
# answer files and to the interactive screens alike. The verbs only record
# answers; MandrakeApply writes them into the mounted BE once BuildBE and
# ApplyChanges have run. Kayak's own log, logcmd, bomb, Postboot, and
# ALTROOT are used as kayak defines them.
#
#   MandrakeAdmin <user> <password>          console admin and OS admin user
#   MandrakeMgmt <link> dhcp                 management VNIC mgmt0 over <link>
#   MandrakeMgmt <link> <addr>/<prefix> [gw] the same, static
#   MandrakeSshKey '<public key>'            authorised key for the OS admin
#
# shellcheck disable=SC2154 # log, bomb, ALTROOT, RPOOL come from kayak
# shellcheck disable=SC2034 # BENAME is read by kayak's BuildBE

[ -n "$MANDRAKE_LIB_LOADED" ] && return 0
MANDRAKE_LIB_LOADED=1

# build-media.sh stages mandrake.env beside the kayak configuration.
if [ -f /kayak/etc/mandrake.env ]; then
    # shellcheck source=../../media/mandrake.env
    . /kayak/etc/mandrake.env
fi
: "${MANDRAKE_VERSION:=0.0.0}"

# The initial boot environment (spec §10.4); kayak's BuildBE defaults to it.
BENAME=mandrake-$MANDRAKE_VERSION

# uid and gid the daemon package fixes for the mandrake user (ADR-0010).
MANDRAKE_UID=63
MANDRAKE_GID=63

# The management VNIC (ADR-0014).
MANDRAKE_MGMT_VNIC=mgmt0

MANDRAKE_ADMIN=
MANDRAKE_ADMIN_PW=
MANDRAKE_MGMT_LINK=
MANDRAKE_MGMT_MODE=
MANDRAKE_MGMT_ADDR=
MANDRAKE_MGMT_GW=
MANDRAKE_SSH_KEYS=()

# ------------------------------------------------------------ validation

# A POSIX user name that is also acceptable to the daemon (ADR-0007).
mandrake_valid_user() {
    [[ $1 =~ ^[a-z_][a-z0-9_-]{0,31}$ ]] && [ "$1" != root ]
}

# A data link the installer can see. The installed system loads the same
# drivers, so the name carries over.
mandrake_valid_link() {
    /sbin/dladm show-phys -p -o link 2>/dev/null | grep -qx -- "$1"
}

# IPv4 or IPv6 address with a prefix length, as ipadm wants it.
mandrake_valid_cidr() {
    [[ $1 =~ ^[0-9A-Fa-f:.]+/[0-9]{1,3}$ ]]
}

mandrake_valid_addr() {
    [[ $1 =~ ^[0-9A-Fa-f:.]+$ ]]
}

# ------------------------------------------------------------ verbs

function MandrakeAdmin {
    typeset user=$1 pw=$2
    log "MandrakeAdmin: $user"
    mandrake_valid_user "$user" \
        || bomb "MandrakeAdmin: '$user' is not a valid user name (a-z, digits, _ -, not root)"
    [ ${#pw} -ge 8 ] || bomb "MandrakeAdmin: the password must be at least 8 characters"
    MANDRAKE_ADMIN=$user
    MANDRAKE_ADMIN_PW=$pw
}

function MandrakeMgmt {
    typeset link=$1 addr=$2 gw=$3
    log "MandrakeMgmt: $link $addr ${gw:-}"
    [ -n "$link" ] || bomb "MandrakeMgmt: a link name is required"
    mandrake_valid_link "$link" \
        || bomb "MandrakeMgmt: link '$link' not found; dladm show-phys lists the links"
    MANDRAKE_MGMT_LINK=$link
    case $addr in
        dhcp|DHCP)
            MANDRAKE_MGMT_MODE=dhcp
            MANDRAKE_MGMT_ADDR=
            MANDRAKE_MGMT_GW=
            ;;
        *)
            mandrake_valid_cidr "$addr" \
                || bomb "MandrakeMgmt: '$addr' must be dhcp or an address with a prefix length"
            if [ -n "$gw" ]; then
                mandrake_valid_addr "$gw" || bomb "MandrakeMgmt: gateway '$gw' is not an address"
            fi
            MANDRAKE_MGMT_MODE=static
            MANDRAKE_MGMT_ADDR=$addr
            MANDRAKE_MGMT_GW=$gw
            ;;
    esac
}

function MandrakeSshKey {
    typeset key=$1
    log "MandrakeSshKey: ${key:0:40}..."
    [[ $key =~ ^(ssh-(rsa|ed25519|dss)|ecdsa-sha2-nistp[0-9]+|sk-[a-z0-9-]+@openssh\.com)\ [A-Za-z0-9+/=]+ ]] \
        || bomb "MandrakeSshKey: not an OpenSSH public key"
    case $key in
        *\'*) bomb "MandrakeSshKey: a key may not contain a single quote" ;;
    esac
    MANDRAKE_SSH_KEYS+=("$key")
}

# ------------------------------------------------------------ apply

# JSON string body for $1: backslash, double quote, and control characters.
mandrake_json_escape() {
    typeset s=$1
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    s=${s//$'\t'/\\t}
    s=${s//$'\n'/\\n}
    s=${s//$'\r'/\\r}
    printf '%s' "$s"
}

# /etc/mandrake/firstboot.json in the BE; the daemon consumes it once.
mandrake_write_firstboot() {
    typeset dir=$ALTROOT/etc/mandrake f=$ALTROOT/etc/mandrake/firstboot.json
    log "Writing $f"
    mkdir -p "$dir"
    (
        umask 077
        printf '{\n  "admin": { "username": "%s", "password": "%s" }\n}\n' \
            "$(mandrake_json_escape "$MANDRAKE_ADMIN")" \
            "$(mandrake_json_escape "$MANDRAKE_ADMIN_PW")" > "$f"
    )
    chmod 0600 "$f"
    chown "$MANDRAKE_UID:$MANDRAKE_GID" "$f"
}

# The OS admin: Primary Administrator, no password, keys only (spec §12).
mandrake_os_admin() {
    typeset u=$MANDRAKE_ADMIN key
    Postboot "/usr/sbin/useradd -m -d /export/home/$u -s /usr/bin/bash -P 'Primary Administrator' $u"
    Postboot "/usr/bin/passwd -N $u"
    if [ ${#MANDRAKE_SSH_KEYS[@]} -gt 0 ]; then
        Postboot "/usr/bin/mkdir -p /export/home/$u/.ssh"
        for key in "${MANDRAKE_SSH_KEYS[@]}"; do
            Postboot "echo '$key' >> /export/home/$u/.ssh/authorized_keys"
        done
        Postboot "/usr/bin/chmod 700 /export/home/$u/.ssh"
        Postboot "/usr/bin/chmod 600 /export/home/$u/.ssh/authorized_keys"
        Postboot "/usr/bin/chown -R $u:other /export/home/$u/.ssh"
    fi
}

# sshd: keys only, no root (spec §12). sshd_config is preserve=true, so
# pkg treats the edit as the administrator's.
mandrake_sshd() {
    typeset f=$ALTROOT/etc/ssh/sshd_config
    log "Configuring sshd in $f"
    sed -i -e '/^[# ]*PermitRootLogin /d' -e '/^[# ]*PasswordAuthentication /d' \
        -e '/^[# ]*KbdInteractiveAuthentication /d' "$f"
    {
        echo
        echo "# Mandrake (ADR-0014): keys only, no root"
        echo "PermitRootLogin no"
        echo "PasswordAuthentication no"
        echo "KbdInteractiveAuthentication no"
    } >> "$f"
}

# The management VNIC and its address, persistent, applied on first boot.
mandrake_mgmt() {
    typeset v=$MANDRAKE_MGMT_VNIC
    Postboot "/usr/sbin/dladm create-vnic -l $MANDRAKE_MGMT_LINK $v"
    Postboot "/usr/sbin/ipadm create-if $v"
    if [ "$MANDRAKE_MGMT_MODE" = dhcp ]; then
        Postboot "/usr/sbin/ipadm create-addr -T dhcp $v/v4"
    else
        Postboot "/usr/sbin/ipadm create-addr -T static -a $MANDRAKE_MGMT_ADDR $v/v4"
        [ -n "$MANDRAKE_MGMT_GW" ] && Postboot "/usr/sbin/route -p add default $MANDRAKE_MGMT_GW"
    fi
}

# IP Filter: SSH and HTTPS in on the management VNIC, everything out,
# nothing else in to the global zone (spec §12).
mandrake_ipfilter() {
    typeset v=$MANDRAKE_MGMT_VNIC f=$ALTROOT/etc/ipf/ipf.conf
    log "Writing $f"
    mkdir -p "$ALTROOT/etc/ipf"
    cat > "$f" <<EOF
# Mandrake (ADR-0014): the global zone accepts SSH and HTTPS on the
# management VNIC only. Zones and VMs have their own stacks.
pass out quick all keep state
pass in quick on $v proto tcp from any to any port = 22 flags S keep state
pass in quick on $v proto tcp from any to any port = 443 flags S keep state
pass in quick on $v proto udp from any port = 67 to any port = 68 keep state
pass in quick on $v proto icmp all keep state
block in log all
EOF
    Postboot "/usr/sbin/svcadm enable network/ipfilter"
}

function MandrakeApply {
    [ -n "$ALTROOT" ] || bomb "MandrakeApply: no boot environment is mounted"
    [ -n "$MANDRAKE_ADMIN" ] || bomb "MandrakeAdmin <user> <password> is required"
    [ -n "$MANDRAKE_MGMT_LINK" ] || bomb "MandrakeMgmt <link> dhcp|<addr/prefix> [gateway] is required"
    log "Applying Mandrake configuration: admin $MANDRAKE_ADMIN, mgmt $MANDRAKE_MGMT_VNIC over $MANDRAKE_MGMT_LINK ($MANDRAKE_MGMT_MODE)"
    mandrake_write_firstboot
    mandrake_os_admin
    mandrake_sshd
    mandrake_mgmt
    mandrake_ipfilter
    # Lock the admin password out of the environment once written.
    MANDRAKE_ADMIN_PW=
    return 0
}
