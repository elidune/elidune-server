# Docker management scripts for Elidune

## build-db-image.sh

Builds a full Docker image containing PostgreSQL and the Elidune server with data from your local database.

**Usage:**
```bash
./scripts/build-db-image.sh
```

**Environment variables:**
- `DB_HOST`: PostgreSQL host (default: localhost)
- `DB_PORT`: PostgreSQL port (default: 5432)
- `DB_USER`: PostgreSQL user (default: elidune)
- `DB_PASSWORD`: PostgreSQL password (default: elidune)
- `DB_NAME`: Database name (default: elidune)
- `IMAGE_NAME`: Docker image name (default: elidune-complete)
- `IMAGE_TAG`: Image tag (default: latest)

## export-image.sh

Exports the Docker image to a tar.gz file for transfer to another machine.

**Usage:**
```bash
./scripts/export-image.sh
```

**Environment variables:**
- `IMAGE_NAME`: Image to export (default: elidune-complete)
- `IMAGE_TAG`: Image tag (default: latest)
- `EXPORT_FILE`: Export filename (default: elidune-complete-YYYYMMDD-HHMMSS.tar.gz)

## import-image.sh

Imports a Docker image from a tar.gz file.

**Usage:**
```bash
./scripts/import-image.sh <image-file.tar.gz>
```

**Example:**
```bash
./scripts/import-image.sh elidune-complete-20260212-143653.tar.gz
```

## Full workflow: Machine A → Machine B

### On machine A (source)

1. **Build the image:**
   ```bash
   ./scripts/build-db-image.sh
   ```

2. **Export the image:**
   ```bash
   ./scripts/export-image.sh
   ```
   This creates `elidune-complete-YYYYMMDD-HHMMSS.tar.gz`.

3. **Transfer the file to machine B:**
   ```bash
   scp elidune-complete-*.tar.gz user@machine-b:/path/to/destination/
   ```
   Or use another method (USB, network, etc.).

### On machine B (destination)

1. **Copy the import script (optional):**
   ```bash
   scp scripts/import-image.sh user@machine-b:/path/to/destination/
   ```

2. **Import the image:**
   ```bash
   ./scripts/import-image.sh elidune-complete-*.tar.gz
   ```
   Or manually:
   ```bash
   gunzip -c elidune-complete-*.tar.gz | docker load
   ```

3. **Run the container:**
   ```bash
   docker run -d --name elidune-complete \
     -p 5432:5432 \
     -p 8080:8080 \
     -e JWT_SECRET=your-secret-key \
     elidune-complete:latest
   ```

4. **Verify:**
   ```bash
   # Check logs
   docker logs elidune-complete

   # Check PostgreSQL
   docker exec elidune-complete pg_isready -U elidune

   # Check API
   curl http://localhost:8080/api/v1/health
   ```

## Important notes

- The image contains **all** PostgreSQL data.
- The image contains the **compiled** Elidune server.
- Default ports are **5432** (PostgreSQL) and **8080** (API).
- Ensure these ports are free on the destination host.
- To change ports, use `-p HOST_PORT:CONTAINER_PORT` with `docker run`.
