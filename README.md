# Hera Stage 1

Hera Stage 1 is a local-first privacy-compliance stack for Zcash and Namada.
The default development path uses:

- Postgres for relational state
- Redis for scan-job queueing
- LocalStack for S3-compatible report artifact storage
- local development KMS keys from `.env`

No real AWS account is required for standard local development or CI.

## Local Setup

1. Copy the environment template.

```sh
cp .env.example .env
```

2. Start the local infrastructure.

```sh
docker compose -f infra/docker/docker-compose.yml --env-file infra/docker/.env.example up -d
```

3. Run the full Stage 1 test suite.

```sh
TMPDIR=/tmp cargo test --workspace
```

4. Run the API and worker in separate terminals.

```sh
cargo run -p hera-api
```

```sh
cargo run -p hera-worker
```

## Environment Notes

- `AWS_ENDPOINT_URL=http://localhost:4566` points the S3 client at LocalStack.
- `AWS_ACCESS_KEY_ID=test` and `AWS_SECRET_ACCESS_KEY=test` are placeholders for LocalStack only.
- `KMS_KEY_ID` is still required as an identifier, but Stage 1 uses `DEV_KMS_KEY_BASE64` through the local dev KMS implementation.
- `S3_BUCKET` is created automatically by the app when it targets LocalStack.

## Test Scope

The standard workspace test run now covers:

- canonical normalization and adapter unit tests
- Postgres repository integration tests
- Redis queue integration tests
- LocalStack-backed report storage integration tests
- API auth, tenant-isolation, and scan enqueue integration tests

Live smoke tests for public Zcash and Namada endpoints remain opt-in because
they depend on external network availability.
