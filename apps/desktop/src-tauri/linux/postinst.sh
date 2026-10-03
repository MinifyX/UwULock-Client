#!/bin/sh
# After installing: UwULock's helper for the virtual security key listens (docs/passkeys.md),
# and the hidraw rule applies right away. Never fails the install.
#
# Packages before 0.5 gave the person at the seat /dev/uhid itself (uaccess); that rule is gone,
# and so is the access it left on the device node.
# Without setfacl (the acl package), chmod does it: on a node with an ACL it sets the mask,
# which takes the named user's access away until the next boot recreates the node.
if [ -e /dev/uhid ]; then
  if command -v setfacl >/dev/null 2>&1; then
    setfacl --remove-all /dev/uhid 2>/dev/null || true
  fi
  chmod 0600 /dev/uhid 2>/dev/null || true
fi
# Root's only: systemd starts it as root for each connection.
chmod 0755 /usr/lib/uwulock/uwulock-uhid-broker 2>/dev/null || true
chown root:root /usr/lib/uwulock/uwulock-uhid-broker 2>/dev/null || true
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules 2>/dev/null || true
  udevadm trigger --subsystem-match=misc --sysname-match=uhid 2>/dev/null || true
fi
if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload 2>/dev/null || true
  systemctl enable --now uwulock-uhid-broker.socket 2>/dev/null || true
fi
exit 0
