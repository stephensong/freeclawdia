.PHONY: fixture fixture-drop fixture-create test test-e2e check ship

# Create test fixture databases populated with predictable content.
# Drops existing test DBs, recreates, runs cumulative tests to populate.
fixture:
	./scripts/fixture.sh all

# Drop all test databases only.
fixture-drop:
	./scripts/fixture.sh drop

# Create + migrate test databases without populating.
fixture-create:
	./scripts/fixture.sh create

# Run all cargo tests (unit + integration).
test:
	cargo test

# Run E2E tests against the fixture (requires running FreeClawdia instance).
test-e2e:
	cd tests/e2e && pytest scenarios/

# Full quality gate: format, lint, test.
check:
	cargo fmt -- --check
	cargo clippy --all --benches --tests --examples --all-features
	cargo test

# Alias for check (matches the /ship skill).
ship: check
