#!/usr/bin/env sh
set -eu

SSH_HOST="${CDITOR_REMOTE_SSH_HOST:-edpb1492802.bohrium.tech}"
SSH_PORT="${CDITOR_REMOTE_SSH_PORT:-22}"
SSH_USER="${CDITOR_REMOTE_SSH_USER:-root}"
REMOTE_DIR="${CDITOR_REMOTE_POSTGRES_DIR:-/opt/cditor-v2-postgres}"
POSTGRES_PORT="${CDITOR_REMOTE_POSTGRES_PORT:-5433}"
POSTGRES_USER="${CDITOR_REMOTE_POSTGRES_USER:-cditor}"
POSTGRES_PASSWORD="${CDITOR_REMOTE_POSTGRES_PASSWORD:-cditor}"
POSTGRES_DB="${CDITOR_REMOTE_POSTGRES_DB:-cditor_test}"

printf 'Bootstrapping remote PostgreSQL on %s@%s:%s\n' "$SSH_USER" "$SSH_HOST" "$SSH_PORT"
printf 'Database URL after success: postgres://%s:%s@%s:%s/%s\n' \
  "$POSTGRES_USER" "$POSTGRES_PASSWORD" "$SSH_HOST" "$POSTGRES_PORT" "$POSTGRES_DB"

ssh -p "$SSH_PORT" "${SSH_USER}@${SSH_HOST}" \
  "REMOTE_DIR='$REMOTE_DIR' POSTGRES_PORT='$POSTGRES_PORT' POSTGRES_USER='$POSTGRES_USER' POSTGRES_PASSWORD='$POSTGRES_PASSWORD' POSTGRES_DB='$POSTGRES_DB' sh -s" <<'REMOTE_SH'
set -eu

mkdir -p "$REMOTE_DIR"
cd "$REMOTE_DIR"

cat > docker-compose.yml <<EOF
services:
  postgres_test:
    image: postgres:16
    container_name: cditor-postgres-test
    restart: unless-stopped
    environment:
      POSTGRES_USER: ${POSTGRES_USER}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: ${POSTGRES_DB}
    ports:
      - "${POSTGRES_PORT}:5432"
    volumes:
      - cditor_postgres_test_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB}"]
      interval: 5s
      timeout: 5s
      retries: 10

volumes:
  cditor_postgres_test_data:
EOF

if command -v docker-compose >/dev/null 2>&1; then
  docker-compose up -d postgres_test
elif docker compose version >/dev/null 2>&1; then
  docker compose up -d postgres_test
else
  echo 'docker compose is not installed on remote server' >&2
  exit 1
fi

printf 'Waiting for PostgreSQL to become ready...\n'
for i in 1 2 3 4 5 6 7 8 9 10 11 12; do
  if docker exec cditor-postgres-test pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
    printf 'Remote PostgreSQL is ready.\n'
    exit 0
  fi
  sleep 2
done

echo 'PostgreSQL container started, but readiness check timed out.' >&2
docker ps --filter name=cditor-postgres-test
exit 1
REMOTE_SH
