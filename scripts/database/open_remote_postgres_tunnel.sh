#!/usr/bin/env sh
set -eu

SSH_HOST="${CDITOR_REMOTE_SSH_HOST:-edpb1492802.bohrium.tech}"
SSH_PORT="${CDITOR_REMOTE_SSH_PORT:-22}"
SSH_USER="${CDITOR_REMOTE_SSH_USER:-root}"
LOCAL_PORT="${CDITOR_REMOTE_DB_LOCAL_PORT:-15433}"
REMOTE_HOST="${CDITOR_REMOTE_DB_HOST:-127.0.0.1}"
REMOTE_PORT="${CDITOR_REMOTE_DB_PORT:-5433}"

printf 'Opening SSH tunnel: localhost:%s -> %s:%s via %s@%s:%s\n' \
  "$LOCAL_PORT" "$REMOTE_HOST" "$REMOTE_PORT" "$SSH_USER" "$SSH_HOST" "$SSH_PORT"
printf 'Keep this process running while using the editor.\n'

exec ssh \
  -p "$SSH_PORT" \
  -o ExitOnForwardFailure=yes \
  -o ServerAliveInterval=30 \
  -o ServerAliveCountMax=3 \
  -N \
  -L "${LOCAL_PORT}:${REMOTE_HOST}:${REMOTE_PORT}" \
  "${SSH_USER}@${SSH_HOST}"
