# Docker Compose for Elidune Complete

This guide explains how to use Docker Compose to run the full Elidune image with persistent volumes and configuration variables.

## Initial setup

1. **Copy the environment file:**
   ```bash
   cp .env.example .env
   ```

2. **Edit `.env` as needed:**
   ```bash
   nano .env
   ```

   Important variables:
   - `JWT_SECRET`: Secret for JWT tokens (change in production!)
   - `POSTGRES_PASSWORD`: PostgreSQL password
   - Ports: `POSTGRES_PORT`, `API_PORT`, `GUI_PORT`

3. **Generate a secure JWT secret:**
   ```bash
   openssl rand -base64 32
   ```
   Put the result in `JWT_SECRET` in `.env`.

## Usage

### Start services

```bash
docker-compose -f docker-compose.complete.yml up -d
```

### Stop services

```bash
docker-compose -f docker-compose.complete.yml stop
```

### Restart services

```bash
docker-compose -f docker-compose.complete.yml restart
```

### View logs

```bash
docker-compose -f docker-compose.complete.yml logs -f
```

### Stop and remove containers (volumes kept)

```bash
docker-compose -f docker-compose.complete.yml down
```

### Remove containers **and** volumes (⚠️ deletes data)

```bash
docker-compose -f docker-compose.complete.yml down -v
```

## Database import/export

`dump-db.sh` and `import-db.sh` automatically detect docker-compose usage.

### Export database

```bash
./scripts/dump-db.sh
```

The dump is created in the project root with a timestamped name.

### Import database

```bash
./scripts/import-db.sh elidune-db-dump-20260213-143653.sql.gz
```

Force import without confirmation:

```bash
./scripts/import-db.sh elidune-db-dump-20260213-143653.sql.gz --force
```

## Persistent volumes

Data is stored in named Docker volumes:
- `elidune-postgres-data`: PostgreSQL data
- `elidune-redis-data`: Redis data

These survive `docker-compose down` without `-v`.

### Backup volumes

```bash
./scripts/backup-volumes.sh
```

### Restore volumes

```bash
./scripts/restore-volumes.sh ./backups/volumes-20260213-XXXXXX
```

## Environment variables

All can be set in `.env`:

- **Docker image:** `ELIDUNE_IMAGE=elidune-complete:latest`
- **Ports:** `POSTGRES_PORT`, `REDIS_PORT`, `API_PORT`, `GUI_PORT`
- **Database:** `DATABASE_URL`, `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB`
- **Redis:** `REDIS_URL`
- **Security:** `JWT_SECRET` (⚠️ change in production!)
- **Logging:** `RUST_LOG`
- **Server:** `ELIDUNE_SERVER_HOST`, `ELIDUNE_SERVER_PORT`

## Service URLs

After startup:

- **Web UI:** http://localhost:8181 (or `GUI_PORT` from `.env`)
- **API:** http://localhost:8282 (or `API_PORT` from `.env`)
- **PostgreSQL:** localhost:5433 (or `POSTGRES_PORT` from `.env`)
- **Redis:** localhost:6379 (or `REDIS_PORT` from `.env`)

## Useful commands

### Service status

```bash
docker-compose -f docker-compose.complete.yml ps
```

### Shell inside container

```bash
docker-compose -f docker-compose.complete.yml exec elidune-complete sh
```

### PostgreSQL logs

```bash
docker-compose -f docker-compose.complete.yml exec elidune-complete tail -f /var/log/supervisor/postgresql.out.log
```

### Elidune server logs

```bash
docker-compose -f docker-compose.complete.yml exec elidune-complete tail -f /var/log/supervisor/elidune-server.out.log
```

## Migrating from a plain `docker run` container

If you already run a container with `docker run`, you can move to docker-compose:

1. **Stop the old container:**
   ```bash
   docker stop elidune-complete
   ```

2. **Create volumes and copy data:**
   ```bash
   ./scripts/migrate-to-volumes.sh
   ```

3. **Start with docker-compose:**
   ```bash
   docker-compose -f docker-compose.complete.yml up -d
   ```

Existing volumes are picked up automatically by docker-compose.
