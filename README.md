# QueryLab

A browser-based SQL playground for PostgreSQL. Connect a database, write and run
SQL in a real editor, explore schemas, inspect execution plans and keep a history
of everything you ran — from a fast, dark-first developer UI.

| Layer    | Stack                                                                  |
| -------- | ---------------------------------------------------------------------- |
| Frontend | Next.js (App Router), TypeScript, Tailwind CSS, shadcn/ui, Monaco      |
| Backend  | Rust, Axum, Tokio, SQLx, Tower, Tracing                                 |
| Data     | PostgreSQL (application metadata), user-supplied PostgreSQL targets    |

## Repository layout

```
querylab/
├── frontend/          Next.js application (deploys to Vercel)
├── backend/           Rust Axum API (deploys to Railway / Render)
├── scripts/sample-db/ Seed data for the local sample target database
└── docker-compose.yml Local PostgreSQL services
```

## Architecture

```
Browser ──► Next.js (Vercel) ──rewrite /api/*──► Rust API (Railway/Render)
                                                   │
                                                   ├──► Metadata PostgreSQL (users, connections, history)
                                                   └──► User target databases (dedicated, short-lived connections)
```

- The browser only talks to the Next.js origin. `/api/*` is rewritten to the
  Rust API, so the session cookie is first-party and HttpOnly.
- The frontend and backend are deployed independently. Vercel serves the
  Next.js app only; the Rust server runs on a container platform.

## Local development

### Prerequisites

- Node.js 20+ and npm
- Rust (stable) — `rustup` installs it
- PostgreSQL 15+ — via `docker compose` or a local install

### 1. Databases

```bash
docker compose up -d
```

This starts:

| Service       | Port | Purpose                                                  |
| ------------- | ---- | -------------------------------------------------------- |
| `metadata-db` | 5432 | QueryLab's own data                                      |
| `sample-db`   | 5433 | Demo target database with a read-only `querylab_reader`  |

### 2. Backend

```bash
cd backend
cp .env.example .env
cargo run
```

The API listens on `http://127.0.0.1:8080`. Check `GET /health`.

### 3. Frontend

```bash
cd frontend
cp .env.example .env.local
npm install
npm run dev
```

Open `http://localhost:3000`.

## Testing

```bash
# Backend: unit + offline API tests
cd backend && cargo test

# Backend: include database-backed tests. DATABASE_URL must point at a
# PostgreSQL 15+ server where the role can CREATE DATABASE (each test gets
# its own throwaway database).
DATABASE_URL=postgres://querylab:querylab@localhost:5432/querylab \
  cargo test -- --include-ignored

# Frontend
cd frontend && npm run lint && npm run typecheck && npm test && npm run build
```

## Backend architecture

```
src/
├── main.rs        binary entry: config, telemetry, migrations, server
├── lib.rs         library crate (router is constructible in tests)
├── config/        environment configuration
├── routes/        route table + cross-cutting middleware (CORS, tracing, limits)
├── handlers/      HTTP layer: extract/validate input, call services
├── services/      business logic (ownership, encryption, execution rules)
├── database/      pool, migrations and repositories (parameterised SQL only)
├── models/        DB records and API DTOs (+ validation)
├── errors/        AppError → consistent JSON error envelope
└── utils/         crypto (AES-256-GCM), validation helpers
```

**Error envelope** — every non-2xx response has the same shape:

```json
{ "error": { "code": "validation_error", "message": "request validation failed",
             "details": { "fields": [{ "field": "host", "message": "..." }] } } }
```

**Credential encryption** — connection passwords are encrypted with
AES-256-GCM using a random 96-bit nonce. The associated data binds each
ciphertext to its owner and connection id, so a ciphertext copied to another
row fails to decrypt. Keys carry an id, so `ENCRYPTION_PREVIOUS_KEYS` lets you
rotate keys without downtime.

## Authentication

- Passwords are hashed with **Argon2id** (hashing runs on a blocking thread).
- Sessions are opaque 256-bit random tokens. Only a SHA-256 digest is stored,
  so a database leak does not expose usable tokens. Sessions slide on
  activity (`SESSION_TTL_HOURS`) up to an absolute cap (`SESSION_MAX_AGE_DAYS`)
  and are revoked server-side on logout.
- The token is delivered as an `HttpOnly; SameSite=Lax` cookie (`Secure` and
  the `__Host-` prefix in production). API clients may instead send
  `Authorization: Bearer <token>`.
- **CSRF**: every state-changing `/api` request must carry `X-QueryLab-CSRF`.
  Browsers cannot attach custom headers cross-site without a CORS preflight,
  and preflights only succeed for `CORS_ORIGINS`.
- Login is rate-limited per IP and per account; unknown emails and wrong
  passwords produce identical responses and comparable timing.
- Every resource query is scoped by `user_id` in SQL; another user's ids return
  `404`, so ids cannot be probed.

The frontend's `proxy.ts` redirects visitors without a session cookie to
`/login`; the API remains the authority and a `401` anywhere sends the user
back to sign in.

## Connecting to user databases

Each operation against a user's database opens a **dedicated, short-lived
connection**. User SQL never runs on QueryLab's metadata pool.

- Options are built with `PgConnectOptions::new_without_pgpass()`, so the
  server's `PG*` environment variables and `.pgpass` never leak into a user
  connection.
- Statement logging is disabled for target connections, so user SQL and values
  never reach the logs.
- Read-only connections set `default_transaction_read_only=on` at startup.
  Use a role that only has `SELECT` grants for defence in depth (see
  `scripts/sample-db` for an example role).
- **SSRF protection**:
  - On save, loopback, private, link-local, CGNAT, ULA and metadata
    addresses, internal hostnames (`localhost`, `*.internal`, `*.local`, …)
    and ports below 1024 are rejected.
  - Right before connecting, the hostname is resolved and *every* address
    must be public.
  - `ALLOW_PRIVATE_NETWORKS=true` lifts the address rule for local
    development only.
  - DNS can change between the check and the connect, so production should
    also restrict egress at the network layer.
- Connection tests (`POST /api/connections/test` for unsaved drafts,
  `POST /api/connections/{id}/test` for saved ones) report the server
  version, role and read-only status. They warn when the role is a
  superuser, read-write mode is on, or TLS is not required.

## SQL execution engine

`POST /api/query/execute` runs one statement on a dedicated connection:

1. Ownership is checked, credentials are decrypted, and the network policy is enforced.
2. `BEGIN READ ONLY` (read-only connections) or `BEGIN`, with
   `statement_timeout` set through `set_config()` (parameterised).
3. The statement is prepared with the **extended query protocol**. PostgreSQL
   accepts exactly one statement there, so `SELECT 1; COMMIT; DROP …` is
   rejected by the server instead of escaping the read-only transaction.
4. Rows are streamed up to the row limit and a ~32 MiB byte budget.
5. Read-only transactions are rolled back. Read-write transactions are
   committed after the statement has been fully drained, so a truncated
   `UPDATE … RETURNING` is never half-applied.
6. The connection is closed, so session state never leaks between runs.

Read-only enforcement relies on PostgreSQL itself — never on regex filtering
of SQL text. Use a role with only `SELECT` grants for defence in depth.

- **Cancellation**: clients send an `execution_id`.
  `POST /api/query/{execution_id}/cancel` issues `pg_cancel_backend` for the
  running backend; only its owner can cancel it.
- **Limits**: per-user rate limit, per-user concurrency limit, statement
  timeout (capped below the HTTP timeout), and a row and byte cap.
- **Values**: `numeric`, `money` and int8 values beyond 2^53 are returned as
  strings to preserve precision. `bytea` is returned as hex. Dates and
  timestamps (including `±infinity`), intervals, inet/cidr, json/jsonb, uuid
  and common arrays are decoded. Other types fall back to text.
- **Errors** carry SQLSTATE, position, detail and hint. The editor marks the
  reported position.
- `POST /api/query/explain` returns `EXPLAIN (FORMAT JSON)`.
  `analyze: true` additionally requires `confirm_analyze: true` and always
  runs inside a READ ONLY transaction that is rolled back, so ANALYZE cannot
  modify data even on read-write connections.

## SQL editor

- Monaco with custom dark and light themes, multiple persistent tabs,
  bracket matching, auto-indent, find/replace (`Ctrl+F` / `Ctrl+H`) and
  comment toggling (`Ctrl+/`).
- `Ctrl+Enter` runs the selection, or else the **statement under the
  cursor**. The splitter understands quotes, `E''` strings, dollar quoting
  and nested comments, and briefly highlights the executed range.
- Schema-aware autocomplete from `/catalog`:
  - tables (schema-qualified outside `public`), after `FROM`/`JOIN`
  - `schema.` → that schema's tables
  - `alias.` → columns with their types
  - keywords and common functions
- `Ctrl+S` saves, `Ctrl+Shift+F` formats (PostgreSQL dialect). A keyboard
  shortcut reference sits in the toolbar.

## Execution plans

- **Explain** shows the planner's estimates. **Explain Analyze…** (the menu
  next to Explain) shows the statement and asks for confirmation before
  running it. ANALYZE runs with `BUFFERS` inside a rolled-back read-only
  transaction. Data-modifying statements, including writable CTEs, fail with
  SQLSTATE 25006 and change nothing.
- The plan tree (`lib/plan.ts`, rendered by `components/results/plan-view.tsx`)
  shows, for every node:
  - its type, the relation and index it uses, and its join type
  - estimated → actual rows
  - its loop count
  - its **self** time (or self cost for plain EXPLAIN)
  - a bar showing its share of the total
- The time a node spends on its own excludes the time of its children.
  Inclusive time is time per loop multiplied by loops, except under
  `Gather`, where loops are workers running in parallel.
- **Hot spots:** nodes with at least 50% of the time are marked *hot*, and
  those with at least 20% are marked *warm*. The summary bar links to the
  slowest node and to the worst row estimate.
- **Warnings:**
  - row estimates off by 10× or more
  - sequential scans whose filter discards at least 90% of at least 1,000
    rows
  - sorts or hashes that spill to disk
  - temp-file writes
- Each node expands to show its conditions, sort and group keys, buffers,
  output columns and memory. The raw JSON plan is one click away and can be
  copied.

## Configuration (backend)

| Variable                   | Default                  | Description |
| -------------------------- | ------------------------ | ----------- |
| `DATABASE_URL`             | —                        | Metadata database (required) |
| `ENCRYPTION_KEY`           | —                        | Base64 32-byte key (required; dev key rejected in production) |
| `ENCRYPTION_KEY_ID`        | `1`                      | Id stamped on new ciphertexts |
| `ENCRYPTION_PREVIOUS_KEYS` | —                        | `id:base64,...` keys accepted for decryption |
| `RUN_MIGRATIONS`           | `true`                   | Apply migrations on startup |
| `DATABASE_MAX_CONNECTIONS` / `DATABASE_MIN_CONNECTIONS` | `10` / `2` | Metadata pool size (min kept warm) |
| `APP_ENV`                  | `development`            | `production` enables JSON logs and strict checks |
| `HOST` / `PORT`            | `0.0.0.0` / `8080`       | Bind address |
| `CORS_ORIGINS`             | `http://localhost:3000`  | Comma-separated origins for direct API access |
| `LOG_FORMAT`               | `pretty` (dev)           | `pretty` or `json` |
| `REQUEST_TIMEOUT_SECS`     | `120`                    | Hard per-request timeout |
| `SESSION_TTL_HOURS`        | `168`                    | Sliding session lifetime |
| `SESSION_MAX_AGE_DAYS`     | `30`                     | Absolute session lifetime |
| `COOKIE_SECURE`            | `true` in production     | `Secure` + `__Host-` session cookie |
| `TRUSTED_PROXY_HOPS`       | `0`                      | Reverse proxies whose `X-Forwarded-For` entry is trusted |
| `AUTH_RATE_LIMIT_PER_MINUTE` | `10`                   | Login/registration attempts per IP and per account |
| `ALLOW_PRIVATE_NETWORKS`   | `false`                  | Allow target DBs on private/loopback networks (dev only) |
| `TARGET_CONNECT_TIMEOUT_SECS` | `10`                  | Connect timeout for target databases |
| `TARGET_CONNECT_RATE_LIMIT_PER_MINUTE` | `60`         | Target connections per user per minute |
| `QUERY_TIMEOUT_SECS`       | `30`                     | Default statement timeout |
| `QUERY_MAX_TIMEOUT_SECS`   | `300`                    | Max timeout a client may request (capped below `REQUEST_TIMEOUT_SECS`) |
| `QUERY_DEFAULT_MAX_ROWS`   | `1000`                   | Default row limit |
| `QUERY_MAX_ROWS`           | `10000`                  | Hard row limit |
| `QUERY_RATE_LIMIT_PER_MINUTE` | `120`                 | Executions per user per minute |
| `MAX_CONCURRENT_QUERIES_PER_USER` | `3`               | Simultaneous executions per user |

## API

| Method | Path           | Description                                        |
| ------ | -------------- | -------------------------------------------------- |
| GET    | `/health`      | Liveness + metadata DB readiness (503 if degraded) |
| GET    | `/api/health`  | Same, reachable through the frontend proxy         |
| GET    | `/api/version` | Build and API version                              |
| POST   | `/api/auth/register` | Create an account and start a session        |
| POST   | `/api/auth/login`    | Start a session                              |
| POST   | `/api/auth/logout`   | Revoke the current session                   |
| GET    | `/api/auth/me`       | Current user                                 |
| GET    | `/api/connections`        | List the user's connections (no secrets) |
| POST   | `/api/connections`        | Create a connection (password encrypted) |
| GET    | `/api/connections/{id}`   | Get a connection                         |
| PUT    | `/api/connections/{id}`   | Update; omit `password` to keep it       |
| DELETE | `/api/connections/{id}`   | Delete a connection                      |
| POST   | `/api/connections/test`   | Test an unsaved connection               |
| POST   | `/api/connections/{id}/test` | Test a saved connection               |
| GET    | `/api/connections/{id}/schemas` | Schemas with table counts          |
| GET    | `/api/connections/{id}/tables?schema=` | Tables, views, row estimates, sizes |
| GET    | `/api/connections/{id}/tables/{table}/columns?schema=` | Columns, types, PK/FK |
| GET    | `/api/connections/{id}/tables/{table}/indexes?schema=` | Index definitions   |
| GET    | `/api/connections/{id}/tables/{table}/relationships?schema=` | Incoming/outgoing FKs |
| GET    | `/api/connections/{id}/catalog` | All tables + columns (autocomplete) |
| POST   | `/api/query/execute`      | Execute one SQL statement                |
| POST   | `/api/query/explain`      | EXPLAIN / EXPLAIN ANALYZE (JSON plan)    |
| POST   | `/api/query/{execution_id}/cancel` | Cancel a running execution      |
| GET    | `/api/history?limit=&before=&status=&connection_id=&q=` | Execution history (keyset paginated) |
| GET    | `/api/history/{id}`       | One history entry                        |
| DELETE | `/api/history/{id}`       | Delete one entry                         |
| DELETE | `/api/history`            | Clear history                            |
| GET    | `/api/queries?q=&collection_id=` | List/search saved queries         |
| POST   | `/api/queries`            | Save a query                             |
| GET    | `/api/queries/{id}`       | Get a saved query                        |
| PUT    | `/api/queries/{id}`       | Update (explicit `null` clears a link)   |
| DELETE | `/api/queries/{id}`       | Delete a saved query                     |
| GET/POST | `/api/collections`      | List/create collections (folders)        |
| PUT/DELETE | `/api/collections/{id}` | Rename/delete (queries are kept)       |
| GET    | `/api/stats?days=`        | Dashboard statistics                     |

Every execution is recorded in history with status (`success`, `error`,
`timeout`, `cancelled`), duration, row count and error message. Saved queries
and history reference connections and collections through composite foreign
keys `(id, user_id)`, so a user can never attach another user's resources.
Deleting a connection or collection unlinks rather than deletes queries.

Schema metadata is read from `pg_catalog` with fully parameterised queries
(exact schema/table names, no identifier quoting or string building) on a
read-only connection with a statement timeout. `?schema=` defaults to `public`.

All endpoints except health, version, register and login require a session.

## License

MIT
