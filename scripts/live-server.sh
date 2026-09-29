#!/usr/bin/env bash
# The client crates against a real UwULock Server, not against fakes: a server from a
# UwULock-Server checkout (its last `cargo build`) on a free port with a certificate from a test
# CA, one account per test, then the tests of uwulock-bitwarden that are ignored by default.
#
#   scripts/live-server.sh ../UwULock-Server                  # all of them
#   scripts/live-server.sh ../UwULock-Server server::icons    # those whose name has this
#   LIVE_VAULTWARDEN=1 scripts/live-server.sh ../UwULock-Server server::moving
#   LIVE_BROWSER=1 scripts/live-server.sh ../UwULock-Server server::uploader
#   scripts/live-server.sh ../UwULock-Server --hold           # only start it, print how to reach it
#
# With LIVE_VAULTWARDEN=1 a Vaultwarden runs beside it (Docker), filled by the server's
# scripts/e2e/vaultwarden.mjs with Bitwarden's CLI (attachments, Sends, an organisation): the
# source of the move. The CLI it needs is installed once under ~/.cache/uwulock-live.
# With LIVE_BROWSER=1 the web vault's upload page answers a file request in Chromium
# (scripts/live-upload.mjs, in Playwright's Docker image).
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
server_dir=$(cd "${1:?usage: live-server.sh <UwULock-Server checkout> [test filter]}" && pwd)
filter=${2:-server::}
binary=${UWULOCK_BINARY:-$server_dir/target/debug/uwulock-server}
[ -x "$binary" ] || { echo "no $binary: cargo build -p uwulock-server there first"; exit 1; }

free_port() { python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }
port=$(free_port)
work=$(mktemp -d)
vaultwarden=
cleanup() {
  kill "${server:-}" "${sink:-}" 2>/dev/null || true
  [ -n "$vaultwarden" ] && docker rm -f "$vaultwarden" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT
mkdir -p "$work/data"

spki=$("$server_dir/scripts/e2e/test-ca.sh" "$work/tls")
export UWULOCK_DATA=$work/data UWULOCK_LISTEN=127.0.0.1:$port UWULOCK_PUBLIC=https://localhost:$port
export UWULOCK_TLS=files UWULOCK_TLS_CERT=$work/tls/cert.pem UWULOCK_TLS_KEY=$work/tls/key.pem
export UWULOCK_UPDATE_CHECK=off UWULOCK_LOGIN_ATTEMPTS=1000
origin=$UWULOCK_PUBLIC
password='correct horse battery staple'

# Mail goes nowhere, but the server has it: Sends only for given addresses need it.
smtp_port=$(free_port)
python3 - "$smtp_port" <<'SINK' &
import socketserver, sys
class Sink(socketserver.StreamRequestHandler):
    def handle(self):
        say = lambda text: self.wfile.write((text + "\r\n").encode())
        say("220 sink.test ESMTP")
        data = False
        for raw in self.rfile:
            line = raw.decode(errors="replace").rstrip("\r\n")
            if data:
                if line == ".":
                    data = False
                    say("250 OK")
                continue
            verb = line[:4].upper()
            if verb in ("EHLO", "HELO"): say("250 sink.test")
            elif verb == "DATA": data = True; say("354 go on")
            elif verb == "QUIT": say("221 bye"); return
            else: say("250 OK")
socketserver.ThreadingTCPServer.allow_reuse_address = True
socketserver.ThreadingTCPServer(("127.0.0.1", int(sys.argv[1])), Sink).serve_forever()
SINK
sink=$!
export UWULOCK_SMTP_HOST=127.0.0.1 UWULOCK_SMTP_PORT=$smtp_port UWULOCK_SMTP_SECURITY=none
export UWULOCK_SMTP_FROM=lock@example.com

# One account per test, so they run side by side; the first is the admin.
accounts=(admin sync realtime hub keys rotate icons versions reminders requests uploads sends move)
declare -A links
for name in "${accounts[@]}"; do
  flag=; [ "$name" = admin ] && flag=--admin
  links[$name]=$("$binary" invite $flag "$name@example.com" | tail -1)
done
"$binary" serve >"$work/server.log" 2>&1 &
server=$!
trap 'tail -n 40 "$work/server.log"' ERR
for _ in $(seq 1 100); do curl -sf --cacert "$work/tls/ca.pem" "$origin/alive" >/dev/null && break; sleep 0.1; done
registering=()
for name in "${accounts[@]}"; do
  NODE_EXTRA_CA_CERTS=$work/tls/ca.pem node "$server_dir/scripts/e2e/register.mjs" \
    "$origin" "${links[$name]}" "$name@example.com" "$password" >/dev/null &
  registering+=($!)
done
for pid in "${registering[@]}"; do wait "$pid"; done

if [ -n "${LIVE_VAULTWARDEN:-}" ]; then
  cache=${XDG_CACHE_HOME:-$HOME/.cache}/uwulock-live
  bw_version=2026.8.0 # the newest CLI wants an endpoint Vaultwarden doesn't have yet
  [ -x "$cache/bw-$bw_version/node_modules/.bin/bw" ] ||
    npm install --silent --no-audit --no-fund --prefix "$cache/bw-$bw_version" "@bitwarden/cli@$bw_version"
  vw_port=$(free_port)
  vaultwarden=uwulock-live-vw-$vw_port
  mkdir -p "$work/vw"
  docker run -d --rm --name "$vaultwarden" --user "$(id -u):$(id -g)" -p "127.0.0.1:$vw_port:$vw_port" \
    -v "$work/vw:/data" -v "$work/tls:/ssl:ro" -e SIGNUPS_ALLOWED=true -e "ROCKET_PORT=$vw_port" \
    -e 'ROCKET_TLS={certs="/ssl/cert.pem",key="/ssl/key.pem"}' -e "DOMAIN=https://localhost:$vw_port" \
    vaultwarden/server:1.37.3 >/dev/null
  for _ in $(seq 1 100); do curl -sf --cacert "$work/tls/ca.pem" "https://localhost:$vw_port/alive" >/dev/null && break; sleep 0.2; done
  NODE_EXTRA_CA_CERTS=$work/tls/ca.pem BW=$cache/bw-$bw_version/node_modules/.bin/bw \
    node "$server_dir/scripts/e2e/vaultwarden.mjs" seed "https://localhost:$vw_port" "$work/vw-state.json" >/dev/null
  export UWULOCK_TEST_VAULTWARDEN=https://localhost:$vw_port UWULOCK_TEST_VAULTWARDEN_STATE=$work/vw-state.json
fi

if [ -n "${LIVE_BROWSER:-}" ]; then
  [ -d "$server_dir/scripts/e2e/node_modules" ] || (cd "$server_dir/scripts/e2e" && pnpm install --frozen-lockfile)
  cat >"$work/upload.sh" <<UPLOAD
#!/usr/bin/env bash
exec docker run --rm --network host --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "$here:$here:ro" -v "$server_dir:$server_dir:ro" -w "$here" \
  -e CHROMIUM_ARGS=--ignore-certificate-errors-spki-list=$spki \
  -e CHROMIUM=/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-linux64/chrome-headless-shell \
  mcr.microsoft.com/playwright:v1.63.0-noble node "$here/scripts/live-upload.mjs" "$server_dir" "\$@"
UPLOAD
  chmod +x "$work/upload.sh"
  export UWULOCK_TEST_UPLOADER=$work/upload.sh
fi

# rustls reads the test CA from here, for HTTPS and the WebSockets alike.
export SSL_CERT_FILE=$work/tls/ca.pem
export UWULOCK_TEST_SERVER=$origin UWULOCK_TEST_PASSWORD=$password
if [ "$filter" = --hold ]; then
  echo "export SSL_CERT_FILE=$SSL_CERT_FILE UWULOCK_TEST_SERVER=$origin UWULOCK_TEST_PASSWORD='$password'"
  [ -n "$vaultwarden" ] && echo "export UWULOCK_TEST_VAULTWARDEN=$UWULOCK_TEST_VAULTWARDEN UWULOCK_TEST_VAULTWARDEN_STATE=$UWULOCK_TEST_VAULTWARDEN_STATE"
  echo "server log: $work/server.log"
  wait "$server"
fi
cd "$here"
cargo test -p uwulock-bitwarden --test integration -q -- --ignored "$filter"
