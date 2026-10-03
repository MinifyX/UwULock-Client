#!/bin/sh
# Before removing UwULock (not on an update: dpkg says "upgrade", rpm counts 1): the helper for
# the virtual security key stops listening. Never fails the removal.
case "$1" in
  remove | purge | 0)
    if [ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1; then
      systemctl disable --now uwulock-uhid-broker.socket 2>/dev/null || true
    fi
    ;;
esac
exit 0
