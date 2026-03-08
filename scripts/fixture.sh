#!/usr/bin/env bash
#
# fixture.sh — Create test fixture databases and populate them via cumulative tests.
#
# Usage:
#   ./scripts/fixture.sh          # Drop, create, migrate, populate all 3 test DBs
#   ./scripts/fixture.sh drop     # Drop all test databases only
#   ./scripts/fixture.sh create   # Create + migrate only (no tests)
#
# Test databases:
#   clawdia_test_gary  — settings, conversations, audit basics (tests 01-05)
#   clawdia_test_emma  — reconstruction, scoping, complex values (tests 06-10)
#   clawdia_test_oli  — extensions, skills, routines, secrets, mixed (tests 11-16)
#
# Safety:
#   - Only operates on databases containing _test_
#   - Will NEVER touch ironclaw, freeclawdia_*, ironclaw_db, or any cream_* database
#   - Uses the local unix socket connection (same as dev)

set -euo pipefail

PGUSER="${PGUSER:-gary}"
TEST_DBS=("clawdia_test_gary" "clawdia_test_emma" "clawdia_test_oli")

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Safety: verify a database name is a test database
assert_test_db() {
    local db="$1"
    if [[ ! "$db" =~ _test_ ]]; then
        echo -e "${RED}SAFETY: refusing to operate on '$db' — not a test database${NC}" >&2
        exit 1
    fi
}

drop_databases() {
    echo -e "${YELLOW}Dropping test databases...${NC}"
    for db in "${TEST_DBS[@]}"; do
        assert_test_db "$db"
        if psql -U "$PGUSER" -lqt | cut -d'|' -f1 | grep -qw "$db"; then
            # Terminate existing connections
            psql -U "$PGUSER" -d postgres -c \
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '$db' AND pid <> pg_backend_pid();" \
                > /dev/null 2>&1 || true
            dropdb -U "$PGUSER" "$db" && echo -e "  ${GREEN}dropped${NC} $db" || echo -e "  ${RED}failed to drop${NC} $db"
        else
            echo -e "  (not found) $db"
        fi
    done
}

create_databases() {
    echo -e "${YELLOW}Creating test databases...${NC}"
    for db in "${TEST_DBS[@]}"; do
        assert_test_db "$db"
        if psql -U "$PGUSER" -lqt | cut -d'|' -f1 | grep -qw "$db"; then
            echo -e "  (exists) $db"
        else
            createdb -U "$PGUSER" "$db" && echo -e "  ${GREEN}created${NC} $db" || echo -e "  ${RED}failed to create${NC} $db"
        fi
    done

    # Install pgvector extension (required by V1 migration).
    # This needs superuser. Try via sudo postgres first, fall back to current user.
    echo -e "${YELLOW}Installing pgvector extension...${NC}"
    for db in "${TEST_DBS[@]}"; do
        assert_test_db "$db"
        if psql -U "$PGUSER" -d "$db" -tAc "SELECT 1 FROM pg_extension WHERE extname = 'vector'" 2>/dev/null | grep -q 1; then
            echo -e "  (exists) $db"
        elif sudo -n -u postgres psql -d "$db" -c "CREATE EXTENSION IF NOT EXISTS vector;" > /dev/null 2>&1; then
            echo -e "  ${GREEN}installed${NC} pgvector on $db (via postgres)"
        elif psql -U "$PGUSER" -d "$db" -c "CREATE EXTENSION IF NOT EXISTS vector;" > /dev/null 2>&1; then
            echo -e "  ${GREEN}installed${NC} pgvector on $db"
        else
            echo -e "  ${RED}FAILED${NC} to install pgvector on $db"
            echo -e "  ${YELLOW}Run: sudo -u postgres psql -d $db -c \"CREATE EXTENSION IF NOT EXISTS vector;\"${NC}"
        fi
    done
}

# Run a group of tests against a specific database.
# Uses individual test names since cargo test uses substring matching, not regex.
run_tests() {
    local url="$1"
    shift
    local tests=("$@")
    local failed=0

    for t in "${tests[@]}"; do
        if ! DATABASE_URL="$url" FIXTURE_MODE=1 cargo test --test time_travel_integration "$t" -- --test-threads=1 --exact 2>&1 | tail -3; then
            failed=1
        fi
    done

    return $failed
}

populate_fixtures() {
    echo -e "${YELLOW}Populating fixtures via cumulative tests...${NC}"

    # DB 1: tests 01-05 (settings, conversations, audit basics)
    local url1="postgres://${PGUSER}@%2Fvar%2Frun%2Fpostgresql/${TEST_DBS[0]}"
    echo -e "\n  ${YELLOW}${TEST_DBS[0]}: tests 01-05${NC}"
    run_tests "$url1" \
        test_01_audit_log_captures_settings_mutations \
        test_02_reconstruct_settings_at_epochs \
        test_03_audit_conversation_lifecycle \
        test_04_interleaved_mutations_multi_entity \
        test_05_rapid_mutations_preserve_order

    # DB 2: tests 06-10 (reconstruction, scoping, complex values, limits)
    local url2="postgres://${PGUSER}@%2Fvar%2Frun%2Fpostgresql/${TEST_DBS[1]}"
    echo -e "\n  ${YELLOW}${TEST_DBS[1]}: tests 06-10${NC}"
    run_tests "$url2" \
        test_06_overwrite_chain_reconstruction \
        test_07_audit_history_entity_scoped \
        test_08_old_new_values_preserved \
        test_09_metadata_stored \
        test_10_limit_respected

    # DB 3: tests 11-16 (extensions, skills, routines, secrets, mixed)
    local url3="postgres://${PGUSER}@%2Fvar%2Frun%2Fpostgresql/${TEST_DBS[2]}"
    echo -e "\n  ${YELLOW}${TEST_DBS[2]}: tests 11-16${NC}"
    run_tests "$url3" \
        test_11_extension_lifecycle_audit \
        test_12_skill_install_remove_audit \
        test_13_routine_lifecycle_audit \
        test_14_secret_audit_no_values_leaked \
        test_15_mixed_entity_types_filtered \
        test_16_multi_extension_interleaved_epochs

    echo -e "\n${GREEN}Fixture complete.${NC}"
    echo ""
    echo "Test databases ready:"
    for db in "${TEST_DBS[@]}"; do
        local count
        count=$(psql -U "$PGUSER" -d "$db" -tAc "SELECT count(*) FROM audit_log" 2>/dev/null || echo "?")
        echo "  $db — $count audit entries"
    done
}

case "${1:-all}" in
    drop)
        drop_databases
        ;;
    create)
        drop_databases
        create_databases
        ;;
    all|fixture)
        drop_databases
        create_databases
        populate_fixtures
        ;;
    *)
        echo "Usage: $0 [drop|create|all]"
        exit 1
        ;;
esac
