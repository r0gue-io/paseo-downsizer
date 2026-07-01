#!/usr/bin/env bash
# matrix-selftest.sh — validate the Matrix notifier path end-to-end, WITHOUT
# triggering a real downsizing dispatch. Mirrors service/src/matrix.rs exactly:
# resolve the room (alias -> id, or use a `!id`), verify the bot is joined, then
# PUT an m.room.message to the same endpoint the service uses.
#
# Reads the SAME env the service reads (default /etc/paseo-downsizer/service.env),
# so it tests the production credentials, not a copy that can drift.
#
# Usage (run on the VM, as root so it can read service.env):
#   sudo deploy/matrix-selftest.sh            # check identity/room/membership + send a test msg
#   sudo deploy/matrix-selftest.sh --no-send  # checks only, post nothing
#   sudo deploy/matrix-selftest.sh --join     # auto-join the room if not a member, then send
#   sudo deploy/matrix-selftest.sh --message "custom text"
#   ENV_FILE=/path/to/other.env sudo -E deploy/matrix-selftest.sh
#
# Exit codes: 0 all good; 1 config/usage; 2 identity; 3 room resolve;
#             4 not joined (and --join not given / join failed); 5 send failed.
set -euo pipefail

ENV_FILE="${ENV_FILE:-/etc/paseo-downsizer/service.env}"
DO_SEND=1
DO_JOIN=0
MESSAGE="paseo-downsizer Matrix self-test — please ignore"
BOT_EXPECT=""   # optional: assert whoami == this mxid (default: derived from token)

while [ $# -gt 0 ]; do
  case "$1" in
    --no-send)  DO_SEND=0 ;;
    --send)     DO_SEND=1 ;;
    --join)     DO_JOIN=1 ;;
    --message)  MESSAGE="${2:-}"; shift ;;
    --expect)   BOT_EXPECT="${2:-}"; shift ;;
    --env)      ENV_FILE="${2:-}"; shift ;;
    -h|--help)  sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
  shift
done

say()  { printf '\033[1m[%s]\033[0m %s\n' "$1" "$2"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$1"; }
die()  { printf '  \033[31m✗ %s\033[0m\n' "$1" >&2; exit "${2:-1}"; }

# --- load config (source the real env; only need MATRIX_*) --------------------
[ -r "$ENV_FILE" ] || die "cannot read $ENV_FILE (run with sudo?)" 1
set -a; # shellcheck disable=SC1090
source "$ENV_FILE"; set +a
HS="${MATRIX_HOMESERVER:-}"; TOKEN="${MATRIX_TOKEN:-}"; ROOM="${MATRIX_ROOM:-}"
HS="${HS%/}"  # trim trailing slash, like matrix.rs
[ -n "$HS" ] && [ -n "$TOKEN" ] && [ -n "$ROOM" ] \
  || die "MATRIX_HOMESERVER / MATRIX_TOKEN / MATRIX_ROOM must all be set in $ENV_FILE — notifier is DISABLED" 1

# --- tiny helpers: url-encode (matches matrix.rs enc()) + JSON field ----------
urlencode() {  # encode a single path segment; unreserved set = A-Za-z0-9-_.~
  local s="$1" out="" i c
  for (( i=0; i<${#s}; i++ )); do
    c="${s:i:1}"
    case "$c" in
      [a-zA-Z0-9._~-]) out+="$c" ;;
      *) printf -v c '%%%02X' "'$c"; out+="$c" ;;
    esac
  done
  printf '%s' "$out"
}
if command -v jq >/dev/null 2>&1; then
  jget() { jq -r --arg k "$1" '.[$k] // empty'; }        # read stdin JSON
elif command -v python3 >/dev/null 2>&1; then
  jget() { python3 -c 'import sys,json;
try: d=json.load(sys.stdin)
except Exception: d={}
print(d.get(sys.argv[1],"") if isinstance(d,dict) else "")' "$1"; }
else
  die "need jq or python3 to parse Matrix responses" 1
fi
api() { curl -sS -H "Authorization: Bearer $TOKEN" "$@"; }  # authed curl

# --- 1. identity --------------------------------------------------------------
say STEP "1/5 confirm token identity (whoami)"
who="$(api "$HS/_matrix/client/v3/account/whoami")"
mxid="$(printf '%s' "$who" | jget user_id)"
[ -n "$mxid" ] || die "whoami failed: $who" 2
if [ -n "$BOT_EXPECT" ] && [ "$mxid" != "$BOT_EXPECT" ]; then
  die "token belongs to $mxid, expected $BOT_EXPECT" 2
fi
ok "token is $mxid"

# --- 2. resolve room (alias -> id) --------------------------------------------
say STEP "2/5 resolve room '$ROOM'"
if [ "${ROOM:0:1}" = "!" ]; then
  ROOM_ID="$ROOM"; ok "room id given directly: $ROOM_ID"
else
  r="$(api "$HS/_matrix/client/v3/directory/room/$(urlencode "$ROOM")")"
  ROOM_ID="$(printf '%s' "$r" | jget room_id)"
  [ -n "$ROOM_ID" ] || die "could not resolve alias $ROOM: $r" 3
  ok "alias $ROOM -> $ROOM_ID"
fi
ROOM_ENC="$(urlencode "$ROOM_ID")"
say NOTE "compare $ROOM_ID against the <ROOMID> in any M_FORBIDDEN error and Element→Advanced→Internal room ID"

# --- 3. membership ------------------------------------------------------------
say STEP "3/5 check bot membership in $ROOM_ID"
m="$(api "$HS/_matrix/client/v3/rooms/$ROOM_ENC/state/m.room.member/$(urlencode "$mxid")")"
member="$(printf '%s' "$m" | jget membership)"
if [ "$member" = "join" ]; then
  ok "membership = join"
else
  say NOTE "membership = ${member:-<none>} ($(printf '%s' "$m" | jget errcode))"
  if [ "$DO_JOIN" = 1 ]; then
    # --- 4. join ---
    say STEP "4/5 joining $ROOM_ID"
    j="$(api -X POST "$HS/_matrix/client/v3/join/$ROOM_ENC" -H 'Content-Type: application/json' -d '{}')"
    jid="$(printf '%s' "$j" | jget room_id)"
    [ -n "$jid" ] || die "join failed: $j  (invite-only? a human member must invite $mxid first)" 4
    ok "joined $jid"
  else
    die "bot not joined (membership=${member:-none}). Re-run with --join, or have a member invite $mxid." 4
  fi
fi

# --- 5. send (same endpoint as matrix.rs::post) -------------------------------
if [ "$DO_SEND" = 0 ]; then
  say DONE "checks passed; --no-send so nothing posted."
  exit 0
fi
say STEP "5/5 send test message"
txn="paseo-selftest-$$-${RANDOM}"   # unique txn id (no wall-clock needed)
body=$(cat <<JSON
{"msgtype":"m.text","body":"$MESSAGE","format":"org.matrix.custom.html","formatted_body":"<b>paseo-downsizer</b> $MESSAGE"}
JSON
)
s="$(api -X PUT "$HS/_matrix/client/v3/rooms/$ROOM_ENC/send/m.room.message/$txn" \
        -H 'Content-Type: application/json' -d "$body")"
eid="$(printf '%s' "$s" | jget event_id)"
if [ -n "$eid" ]; then
  ok "sent — event_id $eid"
  say DONE "Matrix notifier path is healthy. Check the room in Element to confirm delivery."
else
  die "send failed: $s" 5
fi
