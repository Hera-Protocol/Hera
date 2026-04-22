# Stage 1 Frontend/Backend Map

This maps the current `Hera-Frontend` pages to the actual Stage 1 API in `Hera`.

## Current Stage 1 API

Implemented today:

- `POST /v1/workspaces`
- `POST /v1/cases`
- `POST /v1/cases/:id/zcash/import-view-key`
- `POST /v1/cases/:id/namada/import-view-key`
- `POST /v1/cases/:id/scan`
- `GET /v1/cases/:id/status`
- `GET /v1/cases/:id/events`
- `GET /v1/cases/:id/report.json`
- `GET /v1/cases/:id/report.pdf`

All routes require `Authorization: Bearer <api_key>`.

## Page Map

### `/dashboard`

Frontend needs:

- summary cards: active cases, reports generated, last scan
- recent cases table

Backend support:

- no direct support

Missing endpoints:

- `GET /v1/workspaces`
- `GET /v1/workspaces/:workspace_id/cases?limit=5`
- `GET /v1/workspaces/:workspace_id/reports?limit=5`
- optionally `GET /v1/workspaces/:workspace_id/dashboard-summary`

Notes:

- This page cannot be wired cleanly without at least a case list endpoint.

### `/dashboard/cases`

Frontend needs:

- list all cases in workspace
- filter by status
- search by case id

Backend support:

- `POST /v1/cases` only

Missing endpoints:

- `GET /v1/workspaces/:workspace_id/cases`
- optionally `GET /v1/cases/:id`

Notes:

- Backend already has repository support to list cases by workspace, but it is not exposed over HTTP yet.

### `/dashboard/new-case`

Frontend needs:

- create case
- import viewing key
- start scan

Backend support:

- `POST /v1/cases`
- `POST /v1/cases/:id/zcash/import-view-key`
- `POST /v1/cases/:id/namada/import-view-key`
- `POST /v1/cases/:id/scan`

Missing endpoints:

- `GET /v1/workspaces` so the UI can discover the current workspace id

Notes:

- This is the one page that is already mostly compatible with Stage 1.
- Frontend orchestration is:
  1. create case
  2. import key to that case
  3. trigger scan
  4. navigate to case detail using the returned UUID

### `/dashboard/case/:caseId`

Frontend needs:

- case header data
- scan progress
- event timeline
- JSON/PDF downloads
- report hash display

Backend support:

- `GET /v1/cases/:id/status`
- `GET /v1/cases/:id/events`
- `GET /v1/cases/:id/report.json`
- `GET /v1/cases/:id/report.pdf`

Missing endpoints:

- `GET /v1/cases/:id`

Notes:

- This page is partially wireable now if the frontend already knows the case metadata from a case list response.
- If the page is loaded directly, a single-case endpoint is the cleanest way to populate the header.
- Report downloads are already supported once status reaches `SIGNED`.

### `/dashboard/reports`

Frontend needs:

- list signed reports
- filter by chain
- download JSON/PDF

Backend support:

- `GET /v1/cases/:id/report.json`
- `GET /v1/cases/:id/report.pdf`

Missing endpoints:

- `GET /v1/workspaces/:workspace_id/reports`

Notes:

- Download support exists.
- Discovery does not exist. The frontend has no way to know which cases currently have signed reports.

### `/dashboard/keys`

Frontend needs:

- list imported viewing keys
- show chain, created date, linked cases
- delete key

Backend support:

- case-scoped key import only

Missing endpoints:

- `GET /v1/workspaces/:workspace_id/keys`
- `DELETE /v1/cases/:id/view-key` or `DELETE /v1/keys/:key_ref`

Notes:

- Current backend data model stores one viewing key per case.
- The frontend presents keys like a workspace-level inventory, so the HTTP API needs a list view over case-linked keys.

### `/dashboard/audit`

Frontend needs:

- list audit entries
- filter by action

Backend support:

- audit entries are written by the worker, but not exposed over HTTP

Missing endpoints:

- `GET /v1/workspaces/:workspace_id/audit-logs`

Notes:

- The database already has an `audit_logs` table.
- There is no read repository or read handler exposed for the UI yet.

### `/dashboard/settings`

Frontend needs:

- read workspace name/id
- rename workspace
- reveal/regenerate API key
- delete workspace

Backend support:

- `POST /v1/workspaces` only

Missing endpoints:

- `GET /v1/workspaces`
- `PATCH /v1/workspaces/:id`
- `POST /v1/workspaces/:id/api-keys/rotate` or similar
- `DELETE /v1/workspaces/:id`

Notes:

- Current backend auth is API-key-only. There is no workspace settings API yet.

### `/login`

Frontend needs:

- email/password login
- user session

Backend support:

- none

Missing endpoints:

- full auth/session API if this page is meant to be real

Notes:

- Stage 1 backend does not implement user accounts, passwords, sessions, or JWTs.
- It only authenticates bearer API keys.

### `/request-access`

Frontend needs:

- submit interest form

Backend support:

- none

Missing endpoints:

- optional marketing endpoint such as `POST /v1/access-requests`

Notes:

- This page is independent of the Stage 1 compliance workflow and can stay mock/static for now.

### `/`

Frontend needs:

- marketing content only

Backend support:

- none required

## Shape Mismatches

These are the main data mismatches even where routes already exist.

### Case identifiers

Frontend currently uses ids like `CASE-7f3a8b2c`.

Backend returns UUIDs:

- `id: "550e8400-e29b-41d4-a716-446655440000"`

Required change:

- frontend route params and displays should use backend UUIDs, or format a display label separately from the real id

### Chain and network enums

Frontend uses:

- `Zcash`, `Namada`
- `Mainnet`, `Testnet`

Backend uses:

- `ZCASH`, `NAMADA`
- `MAINNET`, `TESTNET`, `REGTEST`

Required change:

- add frontend adapters between API values and UI labels

### Case status model

Frontend uses:

- `CREATED`
- `SCANNING`
- `SIGNED`
- `FAILED`

Backend exposes a finer scan lifecycle:

- `CREATED`
- `KEY_VALIDATED`
- `CHAIN_SYNCING`
- `DETECTING_NOTES`
- `CLASSIFYING_FLOWS`
- `BUILDING_REPORT`
- `SIGNED`
- `FAILED(...)`

Required change:

- map all in-progress backend states to frontend `SCANNING`
- preserve detailed status for the progress UI on case detail

### Event shape

Frontend expects flat rows like:

- `blockHeight`
- `timestamp`
- `eventType`
- `asset`
- `amount`
- `ownershipProof`
- `counterpartyVisibility`

Backend returns `CanonicalEvent` with nested fields:

- `block_height`
- `timestamp`
- `event_type`
- `asset.symbol`
- `amount`
- `counterparty.visibility`
- `evidence_refs`
- `memo`
- `provenance`

Required change:

- frontend adapter should flatten canonical events for table rendering

Suggested mapping:

- `blockHeight <- block_height`
- `eventType <- event_type`
- `asset <- asset.symbol`
- `counterpartyVisibility <- counterparty.visibility`
- `ownershipProof <- "CRYPTOGRAPHIC"` derived from canonical evidence model

### Report hash

Frontend currently shows a mocked report hash string.

Backend returns artifact bytes and includes the hash in:

- `x-artifact-sha256`

Required change:

- when downloading or probing a report artifact, read `x-artifact-sha256` and display that value

## Minimum Backend Additions To Cover The Current Dashboard

Smallest useful set:

- `GET /v1/workspaces`
- `GET /v1/workspaces/:workspace_id/cases`
- `GET /v1/cases/:id`
- `GET /v1/workspaces/:workspace_id/reports`
- `GET /v1/workspaces/:workspace_id/keys`
- `GET /v1/workspaces/:workspace_id/audit-logs`

Then the frontend can be wired page by page without redesigning the current dashboard.

## Best Immediate Integration Order

1. Wire `/dashboard/new-case` to the existing Stage 1 flow.
2. Add `GET /v1/workspaces` and `GET /v1/workspaces/:workspace_id/cases`.
3. Wire `/dashboard/cases` and `/dashboard/case/:caseId`.
4. Add `GET /v1/workspaces/:workspace_id/reports` and wire `/dashboard/reports`.
5. Add keys and audit-log read endpoints.
6. Treat `/login`, `/settings`, and `/request-access` as later-slice product work, not Stage 1 blockers.
