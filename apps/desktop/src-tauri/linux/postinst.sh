#!/bin/sh
# After installing: the udev rule for UwULock's virtual security key applies right away, not
# only after the next boot (docs/passkeys.md). Never fails the install.
modprobe uhid 2>/dev/null || true
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules 2>/dev/null || true
  udevadm trigger --subsystem-match=misc --sysname-match=uhid 2>/dev/null || true
fi
exit 0
