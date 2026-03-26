#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
DB_USER="${DB_USER:-elidune}"
DB_NAME="${DB_NAME:-elidune}"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
DUMP_FILE="elidune-db-dump-${TIMESTAMP}.sql.gz"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Check if docker-compose is being used
USE_COMPOSE=false
if [ -f "${PROJECT_ROOT}/docker-compose.complete.yml" ]; then
    # Check if service is running via docker-compose
    if docker-compose -f "${PROJECT_ROOT}/docker-compose.complete.yml" ps elidune-complete 2>/dev/null | grep -q "Up"; then
        USE_COMPOSE=true
        COMPOSE_FILE="${PROJECT_ROOT}/docker-compose.complete.yml"
    fi
fi

echo -e "${GREEN}=== Dumping Elidune Database ===${NC}"
echo ""

if [ "${USE_COMPOSE}" = true ]; then
    echo -e "${YELLOW}Using docker-compose mode${NC}"
    CONTAINER_NAME=$(docker-compose -f "${COMPOSE_FILE}" ps -q elidune-complete)
    
    if [ -z "${CONTAINER_NAME}" ]; then
        echo -e "${RED}Error: elidune-complete service is not running${NC}"
        echo -e "${YELLOW}Start it with: docker-compose -f docker-compose.complete.yml up -d${NC}"
        exit 1
    fi
    
    # Get PostgreSQL port from docker-compose
    POSTGRES_PORT=$(docker-compose -f "${COMPOSE_FILE}" port elidune-complete 5432 2>/dev/null | cut -d: -f2 || echo "5433")
    
    echo -e "${YELLOW}Dumping database from docker-compose service...${NC}"
    echo -e "${YELLOW}Database: ${DB_NAME}, User: ${DB_USER}, Port: ${POSTGRES_PORT}${NC}"
    echo ""
    
    # Dump using docker-compose exec
    docker-compose -f "${COMPOSE_FILE}" exec -T elidune-complete \
        pg_dump -U "${DB_USER}" -d "${DB_NAME}" \
        --clean --if-exists --no-owner --no-acl \
        --format=plain | gzip > "${PROJECT_ROOT}/${DUMP_FILE}"
else
    # Fallback to direct container access
    CONTAINER_NAME="${CONTAINER_NAME:-elidune-complete}"
    
    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo -e "${RED}Error: Container ${CONTAINER_NAME} is not running${NC}"
        echo -e "${YELLOW}Please start the container first or use docker-compose${NC}"
        exit 1
    fi
    
    echo -e "${YELLOW}Dumping database from container ${CONTAINER_NAME}...${NC}"
    echo -e "${YELLOW}Database: ${DB_NAME}, User: ${DB_USER}${NC}"
    echo ""
    
    # Dump database
    docker exec "${CONTAINER_NAME}" \
        pg_dump -U "${DB_USER}" -d "${DB_NAME}" \
        --clean --if-exists --no-owner --no-acl \
        --format=plain | gzip > "${PROJECT_ROOT}/${DUMP_FILE}"
fi

if [ $? -eq 0 ]; then
    FILE_SIZE=$(du -h "${PROJECT_ROOT}/${DUMP_FILE}" | cut -f1)
    echo -e "${GREEN}✓ Database dumped successfully${NC}"
    echo -e "${GREEN}File: ${PROJECT_ROOT}/${DUMP_FILE} (${FILE_SIZE})${NC}"
    echo ""
    echo -e "${YELLOW}To import this dump:${NC}"
    echo "  ./scripts/import-db.sh ${DUMP_FILE}"
    echo ""
else
    echo -e "${RED}Error: Failed to dump database${NC}"
    exit 1
fi

echo -e "${GREEN}=== Done ===${NC}"
