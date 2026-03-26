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

# Check arguments
if [ $# -eq 0 ]; then
    echo -e "${RED}Usage: $0 <dump-file.sql.gz> [--force]${NC}"
    echo -e "${YELLOW}Example: $0 elidune-db-dump-20260212-143653.sql.gz${NC}"
    echo -e "${YELLOW}Example: $0 elidune-db-dump-20260212-143653.sql.gz --force${NC}"
    exit 1
fi

DUMP_FILE="$1"
FORCE="${2:-}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Resolve absolute path if relative
if [[ ! "$DUMP_FILE" = /* ]]; then
    DUMP_FILE="${PROJECT_ROOT}/${DUMP_FILE}"
fi

# Check if dump file exists
if [ ! -f "${DUMP_FILE}" ]; then
    echo -e "${RED}Error: Dump file ${DUMP_FILE} not found${NC}"
    exit 1
fi

# Check if docker-compose is being used
USE_COMPOSE=false
if [ -f "${PROJECT_ROOT}/docker-compose.complete.yml" ]; then
    # Check if service is running via docker-compose
    if docker-compose -f "${PROJECT_ROOT}/docker-compose.complete.yml" ps elidune-complete 2>/dev/null | grep -q "Up"; then
        USE_COMPOSE=true
        COMPOSE_FILE="${PROJECT_ROOT}/docker-compose.complete.yml"
    fi
fi

echo -e "${GREEN}=== Importing Elidune Database ===${NC}"
echo ""

if [ "${USE_COMPOSE}" = true ]; then
    echo -e "${YELLOW}Using docker-compose mode${NC}"
    SERVICE_NAME="elidune-complete"
    
    # Check if service is running
    if ! docker-compose -f "${COMPOSE_FILE}" ps "${SERVICE_NAME}" 2>/dev/null | grep -q "Up"; then
        echo -e "${RED}Error: elidune-complete service is not running${NC}"
        echo -e "${YELLOW}Start it with: docker-compose -f docker-compose.complete.yml up -d${NC}"
        exit 1
    fi
    
    echo -e "${YELLOW}Service: ${SERVICE_NAME}${NC}"
    echo -e "${YELLOW}Database: ${DB_NAME}, User: ${DB_USER}${NC}"
    echo -e "${YELLOW}Dump file: ${DUMP_FILE}${NC}"
    echo ""
    
    # Check if database has data (unless --force is used)
    if [ "${FORCE}" != "--force" ]; then
        ROW_COUNT=$(docker-compose -f "${COMPOSE_FILE}" exec -T "${SERVICE_NAME}" \
            psql -U "${DB_USER}" -d "${DB_NAME}" -t -c \
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';" 2>/dev/null | tr -d ' ' || echo "0")
        
        if [ "${ROW_COUNT}" != "0" ] && [ "${ROW_COUNT}" != "" ]; then
            echo -e "${YELLOW}Warning: Database ${DB_NAME} already contains data (${ROW_COUNT} tables)${NC}"
            echo -e "${YELLOW}This import will replace all existing data!${NC}"
            read -p "Do you want to continue? (y/N) " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                echo -e "${YELLOW}Import cancelled${NC}"
                exit 0
            fi
        fi
    fi
    
    # Wait for PostgreSQL to be ready
    echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
    for i in {1..60}; do
        if docker-compose -f "${COMPOSE_FILE}" exec -T "${SERVICE_NAME}" \
            pg_isready -U "${DB_USER}" -d "${DB_NAME}" > /dev/null 2>&1; then
            echo -e "${GREEN}✓ PostgreSQL is ready${NC}"
            break
        fi
        if [ $i -eq 60 ]; then
            echo -e "${RED}Error: PostgreSQL did not become ready in time${NC}"
            exit 1
        fi
        sleep 1
    done
    
    # Import database
    echo ""
    echo -e "${YELLOW}Importing database...${NC}"
    echo -e "${YELLOW}(This may take a few minutes depending on the dump size)${NC}"
    
    if [[ "${DUMP_FILE}" == *.gz ]]; then
        # Compressed dump
        gunzip -c "${DUMP_FILE}" | docker-compose -f "${COMPOSE_FILE}" exec -T "${SERVICE_NAME}" \
            psql -U "${DB_USER}" -d "${DB_NAME}"
    else
        # Uncompressed dump
        docker-compose -f "${COMPOSE_FILE}" exec -T "${SERVICE_NAME}" \
            psql -U "${DB_USER}" -d "${DB_NAME}" < "${DUMP_FILE}"
    fi
    
    if [ $? -eq 0 ]; then
        echo ""
        echo -e "${GREEN}✓ Database imported successfully${NC}"
        echo ""
        echo -e "${YELLOW}Verifying import...${NC}"
        
        # Count tables
        TABLE_COUNT=$(docker-compose -f "${COMPOSE_FILE}" exec -T "${SERVICE_NAME}" \
            psql -U "${DB_USER}" -d "${DB_NAME}" -t -c \
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';" 2>/dev/null | tr -d ' ')
        echo -e "${GREEN}✓ Database contains ${TABLE_COUNT} tables${NC}"
        
        # Show some table names
        echo ""
        echo -e "${YELLOW}Sample tables:${NC}"
        docker-compose -f "${COMPOSE_FILE}" exec -T "${SERVICE_NAME}" \
            psql -U "${DB_USER}" -d "${DB_NAME}" -c "\dt" | head -20
        
        echo ""
        echo -e "${GREEN}=== Import completed successfully ===${NC}"
    else
        echo ""
        echo -e "${RED}Error: Failed to import database${NC}"
        exit 1
    fi
else
    # Fallback to direct container access
    CONTAINER_NAME="${CONTAINER_NAME:-elidune-complete}"
    
    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo -e "${RED}Error: Container ${CONTAINER_NAME} is not running${NC}"
        echo -e "${YELLOW}Please start the container first or use docker-compose${NC}"
        exit 1
    fi
    
    echo -e "${YELLOW}Container: ${CONTAINER_NAME}${NC}"
    echo -e "${YELLOW}Database: ${DB_NAME}, User: ${DB_USER}${NC}"
    echo -e "${YELLOW}Dump file: ${DUMP_FILE}${NC}"
    echo ""
    
    # Check if database has data (unless --force is used)
    if [ "${FORCE}" != "--force" ]; then
        ROW_COUNT=$(docker exec "${CONTAINER_NAME}" psql -U "${DB_USER}" -d "${DB_NAME}" -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';" 2>/dev/null | tr -d ' ' || echo "0")
        
        if [ "${ROW_COUNT}" != "0" ] && [ "${ROW_COUNT}" != "" ]; then
            echo -e "${YELLOW}Warning: Database ${DB_NAME} already contains data (${ROW_COUNT} tables)${NC}"
            echo -e "${YELLOW}This import will replace all existing data!${NC}"
            read -p "Do you want to continue? (y/N) " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                echo -e "${YELLOW}Import cancelled${NC}"
                exit 0
            fi
        fi
    fi
    
    # Wait for PostgreSQL to be ready
    echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
    for i in {1..30}; do
        if docker exec "${CONTAINER_NAME}" pg_isready -U "${DB_USER}" -d "${DB_NAME}" > /dev/null 2>&1; then
            echo -e "${GREEN}✓ PostgreSQL is ready${NC}"
            break
        fi
        if [ $i -eq 30 ]; then
            echo -e "${RED}Error: PostgreSQL did not become ready in time${NC}"
            exit 1
        fi
        sleep 1
    done
    
    # Import database
    echo ""
    echo -e "${YELLOW}Importing database...${NC}"
    echo -e "${YELLOW}(This may take a few minutes depending on the dump size)${NC}"
    
    if [[ "${DUMP_FILE}" == *.gz ]]; then
        # Compressed dump
        gunzip -c "${DUMP_FILE}" | docker exec -i "${CONTAINER_NAME}" psql -U "${DB_USER}" -d "${DB_NAME}"
    else
        # Uncompressed dump
        docker exec -i "${CONTAINER_NAME}" psql -U "${DB_USER}" -d "${DB_NAME}" < "${DUMP_FILE}"
    fi
    
    if [ $? -eq 0 ]; then
        echo ""
        echo -e "${GREEN}✓ Database imported successfully${NC}"
        echo ""
        echo -e "${YELLOW}Verifying import...${NC}"
        
        # Count tables
        TABLE_COUNT=$(docker exec "${CONTAINER_NAME}" psql -U "${DB_USER}" -d "${DB_NAME}" -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';" 2>/dev/null | tr -d ' ')
        echo -e "${GREEN}✓ Database contains ${TABLE_COUNT} tables${NC}"
        
        # Show some table names
        echo ""
        echo -e "${YELLOW}Sample tables:${NC}"
        docker exec "${CONTAINER_NAME}" psql -U "${DB_USER}" -d "${DB_NAME}" -c "\dt" | head -20
        
        echo ""
        echo -e "${GREEN}=== Import completed successfully ===${NC}"
    else
        echo ""
        echo -e "${RED}Error: Failed to import database${NC}"
        exit 1
    fi
fi
