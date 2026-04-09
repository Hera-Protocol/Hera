.PHONY: dev migrate test test-integration test-decoder lint clean

dev:
	# Start the local infrastructure stack, then compile the full workspace against it.
	docker compose -f infra/docker/docker-compose.yml up -d
	cargo build --workspace

migrate:
	# Apply pending Postgres migrations without starting the full app stack. Use this after schema changes or on a fresh database.
	cargo run -p hera-db --bin migrate

test:
	# Run the default workspace test suite for fast local correctness checks.
	cargo test --workspace

test-integration:
	# Run ignored integration tests that depend on Docker-backed services and therefore are slower and more environment-sensitive.
	cargo test --workspace -- --ignored

test-decoder:
	# Validate the standalone Namada decoder package offline and with a writable temp dir, matching the restricted local environment used for Stage 1 development.
	TMPDIR=/tmp cargo test --offline --manifest-path tools/namada-masp-decoder/Cargo.toml

lint:
	# Enforce the repo's formatting and warning-free lint baseline before commit or CI.
	cargo clippy --workspace -- -D warnings
	cargo fmt --check

clean:
	# Tear down the local Docker stack and remove persisted volumes when a completely clean environment is needed.
	docker compose -f infra/docker/docker-compose.yml down -v
