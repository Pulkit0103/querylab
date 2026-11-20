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
├── docs/              Deployment guide
├── PROJECT_PLAN.md    Product roadmap (10 phases, 3 releases)
├── render.yaml        Render Blueprint for the API
└── docker-compose.yml Local PostgreSQL services (+ API image with --profile full)
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

**Connection manager:**

- **Environments:** each connection is tagged `development`, `staging` or
  `production`. Staging and production connections show a badge and a
  persistent banner while they are active.
- **Organizing:** connections can be filed into user-defined **groups**,
  by drag and drop or through the **Move to group** menu. They can also
  have a colour label, and can be searched and filtered by environment.
- **Status:**
  - Testing a saved connection records its status on the server:
    `online`, `unreachable`, `auth_failed`, `tls_error` or `error`, with the
    time, latency and error message.
  - **Reconnect** re-tests the connection and reloads its cached schema.
  - Changing the host, port, database, user, password or TLS settings
    clears the stale status.
- **TLS verification:** with `verify-ca` or `verify-full`, a custom CA
  bundle (PEM) can be supplied to verify the server, e.g. a provider's CA.
- **Duplicate** copies a connection. Its password is re-encrypted, because
  each ciphertext is bound to its connection id.

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

## Library and workspace

- **History filters:** search, status, activity (editor queries or Data
  Studio changes), duration, date range and pinned. Filters combine, and
  pages stay correct because the keyset cursor applies after them.
- **Pins:** saved queries and history entries can be pinned. Pinned items
  are listed first, appear in the dashboard's **Pinned** panel and in
  **Ctrl+P**, a quick-open available on every page.
- **Folders and tags:**
  - Folders nest up to three levels. Moves are checked on the server
    inside a transaction that locks the user's folders, so a folder can
    never be moved into its own subtree, even by concurrent requests.
  - Names are unique among siblings.
  - Tags are lower-cased and trimmed: at most 20 per query and 40
    characters each. Filtering by several tags returns queries carrying
    all of them.
- **Tabs follow you:**
  - Open editor tabs (title, SQL, saved-query link), the active tab and
    the active connection are saved to the server shortly after each
    change and restored when you sign in elsewhere.
  - The newer side wins at sign-in. While several browsers are open the
    last write wins.
  - Local storage remains an offline cache, and tabs cached for another
    account in the same browser are never uploaded.
- **Retention:** history older than the account's retention setting
  (Settings, 90 days by default) is deleted hourly in batches. Pinned
  entries are kept.

## ER diagrams

**ER Diagrams** (`/diagrams`) draws a schema as tables and foreign keys.

- **Relationships** use crow's foot notation. A foreign key is one-to-one
  when a unique constraint or index (valid, not partial, no expressions)
  covers a subset of its columns, otherwise one-to-many. A nullable foreign
  key column makes the reference optional. Lines attach to the key columns.
- **Scope:** one schema at a time, optionally with views. Schemas with more
  than 150 tables open with the 150 most connected ones and a notice; the
  **Tables** picker shows any selection of up to 500. Foreign keys to tables
  that are not shown (other schemas or outside the selection) are counted,
  not drawn.
- **Layout:** automatic (ELK layered: referenced tables on the left) the
  first time; dragged positions are saved per connection and schema and
  restored on the next visit. **Reset layout** forgets them.
- **Navigation:** zoom, pan, minimap, **Find table** (highlights the table
  and its neighbours; Enter centres it), and clicking a table opens its
  columns, indexes, constraints and DDL in a side panel.
- **Export:** PNG, SVG, or PDF through the browser's print dialog, of the
  whole diagram, in the current theme or on a light background. Everything
  runs in the browser.

## Table Editor (Data Studio)

The **Table Editor** (`/tables`, or **Open in Data Studio** on a table in the
explorer) browses and edits table rows without writing SQL.

- **Browsing:**
  - Pages are fetched by the server: 50 to 1,000 rows per page.
  - Tables with a primary key (or a unique key whose columns are all
    NOT NULL) use keyset pagination when sorted by that key. Other sorts,
    and views, use offset pages with the key as a tiebreaker.
  - The first page reports the matching row count, counted up to 10,000
    and estimated beyond that.
  - Quick search matches any column as text; LIKE wildcards in the term are
    literal.
- **Filter builder:** column, operator and value rows combined with
  AND/OR groups, nested up to three levels. Operators:
  `= != > >= < <= like ilike in between is_null is_not_null`. The SQL
  shown under the builder is rendered by the server from the same model it
  executes.
- **Editing:**
  - Double-click a cell to edit it. Editors depend on the type: text,
    numbers, booleans and enums as lists, JSON in a larger box, with a NULL
    button for nullable columns.
  - **+ Row** adds a new row; columns left empty use their default.
  - The trash icon marks a row for deletion.
  - Generated columns, identity `ALWAYS` columns, arrays and `bytea` are
    read-only.
- **Applying:**
  - Changes are staged. **Review SQL** shows each generated statement, and
    **Apply** runs them in one transaction: deletes, then updates, then
    inserts.
  - Each update and delete must match exactly one row by its key, and
    every edited cell must still hold the value that was loaded.
    Otherwise everything is rolled back and the failing row is
    highlighted.
  - Editing requires a read-write connection, a row key and the matching
    table privilege.
  - Production connections require typing the connection name; staging
    connections ask for confirmation.
  - Every apply is recorded in query history with its statements.
- **Columns, copying and export:**
  - Hidden columns are remembered per table in the browser.
  - Ctrl+C copies the focused cell. Selected rows (or the page) copy as
    TSV or JSON.
  - **Export** streams CSV, JSON or SQL `INSERT` statements for all
    matching rows (up to 100,000) or for the selected rows, with the
    visible columns.
  - CSV text that a spreadsheet would run as a formula is prefixed with
    `'`.

**How values stay safe** (`backend/src/services/sql_builder.rs`):

- Column names are checked against the live catalog and quoted.
- Every value is bound as a text parameter and cast to the column's
  catalog type, e.g. `CAST($1 AS integer)`. PostgreSQL's own input
  functions parse it.
- Rows are read as text, so keys, cursors and the stale-value check
  compare exactly what was loaded.

## Security and operations

- **Response hardening:** every response carries the following headers.
  `Strict-Transport-Security` is added when `COOKIE_SECURE` is on (HTTPS).
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY`
  - `Referrer-Policy: no-referrer`
  - a deny-all `Content-Security-Policy`
  - `Cache-Control: no-store`
- **Request limits:**
  - Bodies over 2 MiB get `413 payload_too_large`.
  - SQL is capped at 1 MiB.
  - Row limits and timeouts requested by clients are clamped to the server
    maximums.
  - Executions are limited per minute and per user, and so are concurrent
    executions. The concurrency limit returns `429` with `Retry-After`.
- **Structured logs:** set `LOG_FORMAT=json`; this is the default in
  production. Each request produces one line with:
  - method and path (never the query string or body)
  - status and `latency_ms`
  - `request_id` and `user_id`

  5xx responses log at `error` level, so alert on `level=ERROR`.
- **Error monitoring:**
  - A handler panic becomes a JSON `500 internal_error` instead of a
    dropped connection, and the panic message is logged.
  - Internal error details are logged, never returned.
  - Every response has an `X-Request-Id`. The UI shows it as a "reference"
    on server errors, so a user report maps to one log line.
- **Pool tuning:**
  - The metadata pool keeps `DATABASE_MIN_CONNECTIONS` warm and recycles
    connections after `DATABASE_MAX_LIFETIME_SECS`, which picks up
    failovers.
  - Target databases are never pooled. Each execution opens and closes its
    own connection, so sessions and credentials of different users cannot
    mix.
- **Tests:**
  - `backend/tests/security_api.rs` covers forged and tampered tokens,
    session independence, racing registrations, payload validation, and
    cross-user access to every connection endpoint.
  - It also checks that credentials never appear in responses, and that the
    concurrency and rate limits hold under parallel requests.
  - Frontend component tests run with Testing Library in jsdom.

## Configuration (backend)

| Variable                   | Default                  | Description |
| -------------------------- | ------------------------ | ----------- |
| `DATABASE_URL`             | —                        | Metadata database (required) |
| `ENCRYPTION_KEY`           | —                        | Base64 32-byte key (required; dev key rejected in production) |
| `ENCRYPTION_KEY_ID`        | `1`                      | Id stamped on new ciphertexts |
| `ENCRYPTION_PREVIOUS_KEYS` | —                        | `id:base64,...` keys accepted for decryption |
| `RUN_MIGRATIONS`           | `true`                   | Apply migrations on startup |
| `DATABASE_MAX_CONNECTIONS` / `DATABASE_MIN_CONNECTIONS` | `10` / `2` | Metadata pool size (min kept warm) |
| `DATABASE_ACQUIRE_TIMEOUT_SECS` | `10`                | Wait for a free pool connection before failing |
| `DATABASE_IDLE_TIMEOUT_SECS` / `DATABASE_MAX_LIFETIME_SECS` | `300` / `1800` | Close idle / recycle old metadata connections |
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
| POST   | `/api/connections/{id}/test` | Test a saved connection and record its status |
| POST   | `/api/connections/{id}/duplicate` | Copy a connection (credentials re-encrypted) |
| GET, POST | `/api/connection-groups` | List or create connection groups         |
| PUT, DELETE | `/api/connection-groups/{id}` | Rename/reorder or delete a group (connections become ungrouped) |
| GET    | `/api/connections/{id}/schemas` | Schemas with table counts          |
| GET    | `/api/connections/{id}/tables?schema=` | Tables, views, row estimates, sizes |
| GET    | `/api/connections/{id}/tables/{table}/columns?schema=` | Columns, types, PK/FK |
| GET    | `/api/connections/{id}/tables/{table}/indexes?schema=` | Index definitions   |
| GET    | `/api/connections/{id}/tables/{table}/relationships?schema=` | Incoming/outgoing FKs |
| GET    | `/api/connections/{id}/catalog` | All tables + columns (autocomplete) |
| GET    | `/api/connections/{id}/tables/{table}/constraints?schema=` | PK, FK, unique, check, exclusion with definitions |
| GET    | `/api/connections/{id}/tables/{table}/ddl?schema=` | Generated CREATE TABLE / VIEW (+ indexes, triggers, comments) |
| GET    | `/api/connections/{id}/functions?schema=` | Functions and procedures (extension-owned hidden) |
| GET    | `/api/connections/{id}/functions/{oid}` | Function detail with `CREATE OR REPLACE` definition |
| GET    | `/api/connections/{id}/triggers?schema=&table=` | Triggers with timing, events and definition |
| GET    | `/api/connections/{id}/objects?schema=` | Sequences and user-defined types |
| GET    | `/api/connections/{id}/search?q=` | Search tables, columns and functions |
| GET    | `/api/connections/{id}/schema-graph?schema=&tables=&include_views=&limit=` | Tables, keys and foreign keys with inferred cardinality |
| GET/PUT/DELETE | `/api/connections/{id}/diagram-layouts/{schema}` | Saved diagram positions (`DELETE` resets the layout) |
| POST   | `/api/connections/{id}/data/browse` | One page of rows: `{schema, table, filter, sort, search, page_size, cursor}` |
| POST   | `/api/connections/{id}/data/preview` | SQL for a filter and/or change set, without running it |
| POST   | `/api/connections/{id}/data/apply` | Apply a change set in one transaction: `{schema, table, changes, confirm}` |
| POST   | `/api/connections/{id}/data/export` | Stream rows as CSV, JSON or SQL `INSERT` statements |
| POST   | `/api/query/execute`      | Execute one SQL statement                |
| POST   | `/api/query/explain`      | EXPLAIN / EXPLAIN ANALYZE (JSON plan)    |
| POST   | `/api/query/{execution_id}/cancel` | Cancel a running execution      |
| GET    | `/api/history?limit=&before=&status=&kind=&connection_id=&q=&min_ms=&max_ms=&from=&to=&pinned=` | Execution history (keyset paginated, combinable filters) |
| PUT    | `/api/history/{id}/pin`   | Pin or unpin an entry (`{pinned}`)       |
| GET    | `/api/history/{id}`       | One history entry                        |
| DELETE | `/api/history/{id}`       | Delete one entry                         |
| DELETE | `/api/history`            | Clear history                            |
| GET    | `/api/queries?q=&collection_id=&tags=&pinned=` | List/search saved queries (pinned first; `tags` must all match) |
| PUT    | `/api/queries/{id}/pin`   | Pin or unpin a saved query               |
| GET    | `/api/tags`               | The user's tags with counts              |
| POST   | `/api/queries`            | Save a query                             |
| GET    | `/api/queries/{id}`       | Get a saved query                        |
| PUT    | `/api/queries/{id}`       | Update (explicit `null` clears a link)   |
| DELETE | `/api/queries/{id}`       | Delete a saved query                     |
| GET/POST | `/api/collections`      | List/create folders (`parent_id` nests them) |
| PUT/DELETE | `/api/collections/{id}` | Rename and/or move (`parent_id`); delete removes subfolders, queries are kept |
| GET    | `/api/stats?days=`        | Dashboard statistics                     |
| GET/PUT | `/api/workspace/tabs`    | Load/save the synced editor session (≤ 50 tabs, ≤ 2 MiB) |
| GET/PUT | `/api/workspace/settings` | History retention in days (1–3650, default 90) |

Every execution is recorded in history with status (`success`, `error`,
`timeout`, `cancelled`), duration, row count and error message. Saved queries
and history reference connections and collections through composite foreign
keys `(id, user_id)`, so a user can never attach another user's resources.
Deleting a connection or collection unlinks rather than deletes queries.

Schema metadata is read from `pg_catalog` with fully parameterised queries
(exact schema/table names, no identifier quoting or string building) on a
read-only connection with a statement timeout. `?schema=` defaults to `public`.

All endpoints except health, version, register and login require a session.

## Roadmap

[PROJECT_PLAN.md](PROJECT_PLAN.md) describes the next stage of the
platform. Each phase lists:

- what already exists and what remains
- frontend and backend tasks
- API endpoints
- metadata schema changes
- dependencies on other phases
- acceptance criteria and tests

Phases are implemented one at a time, after their plan is reviewed.

## Deployment

The guide is in [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md). It covers:

- the frontend on **Vercel**
- the API on **Render** (`render.yaml`) or **Railway**
  (`backend/railway.json`), both building `backend/Dockerfile`
- the metadata database on **Neon**
- production environment variables, HTTPS and CORS, health checks,
  logging, key rotation and a smoke-test checklist

CI builds the production image on every pull request and smoke-tests it.

## License

MIT
