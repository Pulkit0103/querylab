# QueryLab Project Plan

QueryLab is a browser-based PostgreSQL management and SQL development
platform. This plan organizes further development into ten phases, grouped
into three releases. Each phase is implemented separately, after its plan
has been reviewed. Each phase gets its own feature branch and pull request,
and CI must pass before merging.

**Scope rules**

- PostgreSQL is the only supported database engine for now.
- No AI-powered features: no LLM integrations, no assistants, and no
  natural-language SQL generation.
- Existing behaviour is preserved. New features extend the current
  architecture (routes → handlers → services → repositories, a typed API
  client, and Zustand stores) and do not replace it.

**Status legend**

| Mark | Meaning |
| --- | --- |
| ✅ | Implemented and in production |
| ◐ | Partly implemented; the remaining work is listed |
| ☐ | Not started |

---

## Contents

1. [Current baseline](#current-baseline)
2. [Architecture principles for new work](#architecture-principles-for-new-work)
3. [Release plan and phase dependencies](#release-plan-and-phase-dependencies)
4. Phases
   - [Phase 1: Core SQL Workspace](#phase-1--core-sql-workspace)
   - [Phase 2: Database Connection Manager](#phase-2--database-connection-manager)
   - [Phase 3: Advanced Database Explorer](#phase-3--advanced-database-explorer)
   - [Phase 4: Visual Table & Data Studio](#phase-4--visual-table--data-studio)
   - [Phase 5: Query History & Workspace Management](#phase-5--query-history--workspace-management)
   - [Phase 6: Schema Visualization & ER Diagrams](#phase-6--schema-visualization--er-diagrams)
   - [Phase 7: Query Performance & Monitoring](#phase-7--query-performance--monitoring)
   - [Phase 8: Schema Diff & Migration Manager](#phase-8--schema-diff--migration-manager)
   - [Phase 9: Import, Export & Mock Data](#phase-9--import-export--mock-data)
   - [Phase 10: Security, Collaboration & Deployment](#phase-10--security-collaboration--deployment)
5. [Target navigation](#target-navigation)
6. [Cross-cutting testing strategy](#cross-cutting-testing-strategy)
7. [Deployment considerations](#deployment-considerations)

---

## Current baseline

Already in production at `querylab-vert.vercel.app`: the Vercel frontend
calls the Render API, which stores its data in Neon.

| Area | What exists |
| --- | --- |
| Auth | Email/password with Argon2id. Opaque session tokens, stored only as hashes. HttpOnly `__Host-` cookie. CSRF header on state-changing requests. Login rate limits. |
| Connections | CRUD and test-before-save. Credentials encrypted with AES-256-GCM, with key rotation. SSRF guard checked both when saving and when connecting. SSL modes. Read-only by default. |
| Explorer | Schemas, tables and views, columns with PK/FK, indexes, relationships, table search, data preview, copy name |
| Editor | Monaco with themes, tabs, schema-aware autocomplete, Ctrl+Enter on the statement under the cursor, formatting, find/replace, shortcuts |
| Execution | Read-only enforced by PostgreSQL. Single statement per run. Timeouts, row and byte limits, cancellation. Typed value decoding. Structured SQL errors. |
| Plans | EXPLAIN and EXPLAIN ANALYZE (confirmed, rolled back). Plan tree with self time and cost, misestimates and hot nodes. |
| Library | History with filters and keyset pagination. Saved queries in collections. Dashboard statistics. |
| Operations | Security headers, panic capture, JSON request logs with request and user ids, pool tuning. CI covers backend, frontend and the Docker image. Deployment guide. |

The **metadata tables** today are `users`, `sessions`,
`database_connections`, `query_collections`, `saved_queries` and
`query_history`. All of them are owned by `user_id`, and composite foreign
keys keep every reference within the same owner.

---

## Architecture principles for new work

These rules apply to every phase. Each phase plan refers back to them.

1. **Privileged operations are separate workflows.** Anything that changes
   a target database must go through an explicit, confirmed endpoint and
   never through the general SQL runner's defaults. This covers:
   - data edits (Phase 4)
   - migrations (Phase 8)
   - imports and mock data (Phase 9)

   Each such endpoint must:
   - require a connection with `read_only = false`
   - run in a single transaction
   - return what it changed
   - write an audit event, once Phase 10 has added audit events. Until
     then, the event is recorded in the action's history entry.
2. **The server builds all generated SQL.** For inserts, updates, deletes,
   filters, migrations and DDL, the client sends structured intent: table,
   key values, column values and filter tree. The server checks
   identifiers against the live catalog, quotes them with `quote_ident`
   semantics and binds every value as a parameter. The client never sends
   SQL fragments that are spliced into statements.
3. **Row identity uses primary keys.** Editing and deleting rows requires
   a primary key, or a unique not-null key. Tables without one are
   read-only in the Data Studio. `ctid` is never used for writes.
4. **Environments add guardrails.** Connections tagged `production` show a
   persistent warning and require typed confirmation, the connection name,
   for any privileged operation. `staging` requires a normal confirmation.
5. **One introspection model.** A typed Rust snapshot of a schema is the
   single source for the explorer, DDL generation, ER diagrams and schema
   diff. It covers tables, columns, constraints, indexes, functions,
   triggers, views and sequences. It is built in Phase 3 and reused in
   Phases 6 and 8.
6. **Bounded work.** Every endpoint that reads a target database has a
   statement timeout, a row or byte cap and a per-user rate or
   concurrency limit. Long operations (imports, exports, mock data) stream
   their data and report progress. They never load everything into memory.
7. **Ownership now, workspaces later.** New tables keep `user_id`
   ownership with composite foreign keys, as today. Phase 10 adds
   `workspace_id` and memberships through one backfilling migration, so
   intermediate phases do not have to plan for multi-tenancy.
8. **Dependencies.** A new library is added only when it replaces
   substantial custom work. Each phase lists its proposed additions for
   review.

---

## Release plan and phase dependencies

| Release | Phases | Outcome |
| --- | --- | --- |
| **Release 1: Core SQL IDE (MVP)** | 1–5 | Daily-use SQL IDE: editor, connection manager, explorer, table editor, history and saved queries |
| **Release 2: Advanced Database Studio** | 6–8 | ER diagrams, performance analysis, monitoring, schema diff and migrations |
| **Release 3: Professional Database Platform** | 9–10 | Import/export, mock data, teams, roles, audit logs, production operations |

```
P1 Core SQL Workspace ──┬─────────────────────────────────────────────┐
P2 Connection Manager ──┼──▶ P3 Explorer (introspection model) ──┬──▶ P6 ER Diagrams
                        │                                         ├──▶ P8 Schema Diff & Migrations ◀── P2 (environments)
                        │                                         └──▶ P4 Data Studio ──▶ P9 Import/Export/Mock
                        └──▶ P5 History & Workspace                     
P7 Performance & Monitoring ◀── P2 (connection status), existing plan analysis
P10 Security & Collaboration ◀── all phases (audit events, workspace ownership)
```

| Phase | Hard dependencies | Why |
| --- | --- | --- |
| 3 | 2 | Explorer actions need the connection status and environment |
| 4 | 3 | Data Studio needs PK and constraint metadata, column types and privileges |
| 6 | 3 | Diagrams render the introspection model |
| 7 | 2 | Monitoring needs connection status and visibility checks |
| 8 | 2, 3 | Diff compares two snapshots; guardrails depend on environments |
| 9 | 3, 4 | Import mapping and mock data need types, FK graphs and write paths |
| 10 | all | Audit events and workspace sharing touch every resource |

**Recommended order:** finish Release 1 (2 → 3 → 4 → 5, plus polish for
Phase 1) before starting Release 2.

---

## Phase 1 — Core SQL Workspace

**Goal:** a professional SQL editor. **Status:** ✅ essentially complete;
this phase is polish only.

| Feature | Status | Notes |
| --- | --- | --- |
| Monaco SQL editor, syntax highlighting | ✅ | Custom dark and light themes |
| SQL autocomplete | ✅ | Tables, `schema.`, `alias.` columns, keywords, functions |
| Multiple query tabs | ✅ | Persisted per browser |
| Run with Ctrl+Enter | ✅ | Runs the selection, or else the statement under the cursor |
| SQL formatting, find and replace | ✅ | |
| Query execution status | ✅ | Elapsed timer, row counts, cancel |
| Resizable panels, dark/light themes, shortcuts | ✅ | |
| Tab rename, duplicate, close others, reopen closed tab | ☐ | Polish |
| Editor preferences: font size, word wrap, minimap, tab size | ☐ | Polish; stored with Settings |
| Bundle Monaco instead of loading it from the CDN | ☐ | Removes a runtime third-party dependency and allows a stricter CSP |
| Run a script as sequential statements, one result tab each | ☐ | Optional; each statement still goes through the single-statement executor |

**Frontend tasks**
- Tab context menu: rename, duplicate, close others, close to the right,
  reopen closed. This needs an `editor-store` history stack.
- `EditorPreferences` in the Settings page and the editor store, applied
  through `editor.updateOptions`.
- Self-hosted Monaco loader (`@monaco-editor/loader` configured with local
  paths). Verify the bundle size impact.

**Backend tasks:** none.

**API endpoints:** none.

**Database changes:** none. Preferences stay in local storage until
Phase 10 adds user settings.

**Acceptance criteria**
- All tab operations survive a reload.
- Preferences apply instantly and persist.
- The editor loads with no third-party script origins once Monaco is
  bundled.

**Tests:** editor store unit tests for the tab operations; a component test
for the tab context menu.

---

## Phase 2 — Database Connection Manager

**Goal:** manage many PostgreSQL databases from one place. **Status:** ✅ delivered.

> **Implementation note:** CA certificates are public data, so they are stored in a plain ssl_root_cert column rather than inside the encrypted credential blob. This keeps partial updates simple and does not weaken credential protection.

| Feature | Status | Notes |
| --- | --- | --- |
| PostgreSQL connections, multiple per user | ✅ | |
| Connection testing | ✅ | Draft and saved tests, server facts, warnings |
| SSL configuration | ✅ | Modes plus a custom CA bundle (PEM) for `verify-ca`/`verify-full`. Client certificates remain optional future work. |
| Secure credential storage | ✅ | AES-GCM with AAD bound to the user and connection; key rotation |
| Connection grouping | ✅ | User-defined groups with drag and drop, move-to menu, rename and delete |
| Dev, Staging and Production environments | ✅ | Environment tag, badges, environment filter and a persistent banner for staging and production. Typed confirmation arrives with the first privileged operation (Phase 4). |
| Connection status indicator | ✅ | Server-recorded `online`, `unreachable`, `auth_failed`, `tls_error` or `error`, with time, latency and error. Cleared when the target changes. |
| Reconnect | ✅ | Re-tests the connection and drops cached schema metadata |
| Duplicate connection, colour label | ✅ | Credentials are re-encrypted under the copy's id |

**Frontend tasks**
- Connections page: list grouped by group and environment, filtering,
  search, and collapsible groups with drag and drop between them.
- Connection dialog additions:
  - environment select
  - group select, or create a new group
  - colour
  - CA certificate upload as PEM text, validated client-side for format
    only
- Status dot in the connection selector and the list. A **Reconnect**
  action calls the test endpoint and updates status.
- A production banner in the editor and explorer while the active
  connection is `production`.

**Backend tasks**
- Migration: add `environment`, `group_id` and `color` to
  `database_connections`, plus a new `connection_groups` table (see
  below).
- Live status: `POST /connections/{id}/test` already exists. Persist the
  result to `last_status` (enum), `last_status_at` and `last_latency_ms`.
  The status is per user and recorded only when the user triggers a
  test. There is no background polling of user databases.
- Custom CA: store it in the encrypted secret blob (version the blob
  payload as JSON with `password` and `ca_pem`). Apply it with
  `PgConnectOptions::ssl_root_cert_from_pem`.
- Duplicate: `POST /connections/{id}/duplicate` copies the record and
  re-encrypts the secret under a new id, since the AAD includes the id.

**API endpoints**

| Method | Path | Purpose |
| --- | --- | --- |
| GET, POST | `/api/connection-groups` | List or create groups |
| PUT, DELETE | `/api/connection-groups/{id}` | Rename or delete a group; its connections become ungrouped |
| POST | `/api/connections/{id}/duplicate` | Copy a connection |
| *(changed)* | `/api/connections` | Accepts and returns `environment`, `group_id`, `color`, `last_status*` |

**Database changes**

```sql
CREATE TABLE connection_groups (
  id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users ON DELETE CASCADE,
  name text NOT NULL, position int NOT NULL DEFAULT 0,
  created_at timestamptz NOT NULL DEFAULT now(), UNIQUE (user_id, name), UNIQUE (id, user_id));
ALTER TABLE database_connections
  ADD COLUMN environment text NOT NULL DEFAULT 'development'
      CHECK (environment IN ('development','staging','production')),
  ADD COLUMN group_id uuid, ADD COLUMN color text,
  ADD COLUMN last_status text, ADD COLUMN last_status_at timestamptz, ADD COLUMN last_latency_ms int,
  ADD FOREIGN KEY (group_id, user_id) REFERENCES connection_groups (id, user_id) ON DELETE SET NULL (group_id);
```

**Acceptance criteria**
- A user can organize at least 20 connections into groups and
  environments.
- Status reflects the latest test.
- Production connections are visually unmistakable.
- A wrong CA certificate fails with a clear TLS error.
- Users cannot reference groups belonging to someone else (`404`).

**Tests**
- Repository tests for group ownership.
- An API test for duplicating a connection, verifying the secret decrypts
  under the new id.
- CA PEM validation tests.
- Component tests for the grouped list and the production banner.

---

## Phase 3 — Advanced Database Explorer

**Goal:** effortless database exploration. **Builds:** the shared
introspection model (principle 5). **Status:** ✅ delivered.

| Feature | Status | Notes |
| --- | --- | --- |
| Schema explorer, tables and views | ✅ | |
| Columns and data types, primary and foreign keys | ✅ | |
| Index explorer, table relationships | ✅ | |
| Constraints explorer | ✅ | Primary, foreign, unique, check and exclusion constraints with definitions |
| Functions and stored procedures | ✅ | Signature, return type, language, volatility, security definer and definition; extension-owned functions hidden |
| Triggers | ✅ | Timing, events, level, function, enabled state, definition |
| Materialized views, sequences, enums and domains | ✅ | Tree sections for tables, views, functions, sequences and types (also composite and range types) |
| Search tables **and columns** | ✅ | Tree filter plus server-side search over tables, columns and functions; exact matches first |
| Right-click table → view records | ✅ | Context menu on tables and views, including **Open in Data Studio** (Phase 4). |
| Copy table name | ✅ | |
| Generate SELECT statement | ✅ | Explicit, quoted column list in a new tab |
| Generate CREATE TABLE statement | ✅ | Identity and generated columns, collations, constraints, indexes, triggers and comments; views and materialized views. Verified by a round-trip test. |
| Generate INSERT/UPDATE templates | ✅ | INSERT template with typed placeholders. UPDATE and DELETE statements are generated from Data Studio change sets (Phase 4). |

**Frontend tasks**
- Tree sections per schema: Tables, Views, Materialized Views, Functions,
  Procedures, Triggers, Sequences and Types. Each section loads lazily.
- Context menu (Base UI `ContextMenu`) on tables and views:
  - View records
  - Open in Data Studio (Phase 4)
  - Copy name
  - Copy qualified name
  - Generate SELECT, INSERT template or CREATE TABLE in a new tab
- Table details gain a **Constraints** tab and a **DDL** tab, shown
  read-only in Monaco.
- Function and trigger detail panels with highlighted definitions.
- Global explorer search across tables, columns and functions, with
  keyboard navigation.

**Backend tasks**
- A `services/introspection` module with typed structs: `Table`, `Column`,
  `Constraint` (with a kind enum), `Index`, `Function`, `Trigger`, `View`,
  `Sequence` and `EnumType`. Queries are parameterized `pg_catalog`
  queries, and each fetch is bounded by a statement timeout.
- A DDL generator: `CREATE TABLE` covering columns, defaults, identity
  columns, NOT NULL and constraints, followed by `CREATE INDEX` statements
  for indexes that don't back constraints. It is cross-checked against
  `pg_get_constraintdef` and `pg_get_indexdef` for fidelity. Functions use
  `pg_get_functiondef`, triggers use `pg_get_triggerdef` and views use
  `pg_get_viewdef`.
- Column search uses one catalog query with `ILIKE` and an escaped
  pattern, limited to 100 results.

**API endpoints**

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/connections/{id}/tables/{table}/constraints?schema=` | All constraints with definitions |
| GET | `/api/connections/{id}/tables/{table}/ddl?schema=` | Generated `CREATE TABLE` and index DDL |
| GET | `/api/connections/{id}/functions?schema=` | Functions and procedures |
| GET | `/api/connections/{id}/functions/{oid}` | Function definition |
| GET | `/api/connections/{id}/triggers?schema=&table=` | Triggers |
| GET | `/api/connections/{id}/objects?schema=&kind=` | Views, materialized views, sequences, types |
| GET | `/api/connections/{id}/search?q=` | Tables, columns and functions matching `q` |

**Database changes:** none. The explorer is read-only and cached
client-side with TanStack Query.

**Acceptance criteria**
- For the sample database and a database with functions and triggers, the
  generated DDL recreates equivalent tables. This is verified in a test:
  apply the DDL to an empty database, re-introspect and compare snapshots.
- Every object type opens a detail view.
- Search finds columns by name in under 500 ms on a schema with 1,000
  tables.

**Tests**
- A DDL round-trip integration test.
- Introspection tests for each object kind against a seeded fixture schema
  (add functions, triggers, enums, check constraints and a materialized
  view to `scripts/sample-db`).
- Component tests for the context menu actions.

---

## Phase 4 — Visual Table & Data Studio

**Goal:** manage records without writing every query by hand.
**Status:** ✅ delivered. **Priority:** high.

> **Implementation notes:**
> - Values travel as PostgreSQL text: rows are read with `CAST(col AS text)`, and every bound value is cast to the column's catalog type (domains resolved, type modifiers dropped so over-long values raise errors instead of being truncated). Keys, cursors and the stale-value check therefore round-trip exactly for every scalar type.
> - The stale-value check compares text forms: `CAST(col AS text) IS NOT DISTINCT FROM $n`. The UI always sends the loaded values of edited cells.
> - A failed apply returns `200` with `committed: false` and the failing change (kind, index, conflict flag, SQL error), so the grid can highlight the row. Validation, guardrail and privilege errors use the normal error envelope.
> - Statements run deletes, then updates, then inserts. A deferred-constraint failure at `COMMIT` is reported without a change index.
> - Until Phase 5 adds `kind`, applies are recorded in `query_history` as a script whose first line is `-- Data Studio: N change(s) to schema.table`.
> - Exports are capped at 100,000 rows. CSV export neutralises spreadsheet formulas, as the result grid already does.
> - `bytea` cells show a hex preview of the first 2 KiB; `bytea` and array columns are read-only, as planned.

| Feature | Notes |
| --- | --- |
| Browse table records with pagination | Keyset pagination when a primary key exists, otherwise offset (read-only) |
| Sort columns; filter, search and paginate | Server-side, built from structured parameters |
| Visual filter builder | Column, operator and value rows, combined with AND/OR groups; shows a preview of the generated SQL |
| Column visibility controls | Per table, saved locally |
| Inline editing; insert and delete records | Requires a writable connection and a primary key. Changes are staged and applied together in one transaction. |
| Copy cell and row data; export selected records | CSV, JSON and SQL `INSERT` |
| Generate INSERT/UPDATE/DELETE SQL | From staged changes, shown before applying |

**Design**
- **Staged changes:** edits accumulate client-side as a change set of
  inserts, updates (key plus changed columns) and deletes (key). The user
  reviews the generated SQL, then clicks **Apply**. The server applies the
  set in one transaction with `statement_timeout`. Each update is guarded
  with `WHERE pk = $1`, and the apply fails if the affected count is not
  exactly 1 for any row (optimistic concurrency).
- **Optional stale-write protection:** the client sends the original
  values of edited columns, and the update adds
  `AND col IS NOT DISTINCT FROM $orig`. A conflict is reported per row.
- **Filter model**, sent as JSON:
  `{op:'and'|'or', items:[{column, operator, value}|group]}`.
  - Operators: `= != > >= < <= like ilike in is_null is_not_null between`.
  - The server checks columns against the catalog, maps operators to SQL
    and binds values with casts derived from the column's catalog type.
  - The same model produces the SQL preview, rendered server-side through
    a dry-run endpoint so that the preview and the executed SQL are
    identical.
- **Type-aware editors:** text, number, boolean, date/time, JSON in a
  Monaco mini editor, enum select and NULL toggle. `bytea` and arrays are
  read-only at first.

**Frontend tasks**
- A `/tables` route, **Table Editor**. It opens from the explorer context
  menu or a table picker.
- A data grid with editable cells. Extend the virtualized results grid
  with an editing layer rather than adding a grid library; revisit only if
  performance requires it.
- Filter builder component with the live SQL preview, a column visibility
  popover and a change-set panel (pending inserts, updates and deletes,
  generated SQL, Apply and Discard).
- Production guardrail: typed confirmation before applying (principle 4).

**Backend tasks**
- `services/data_studio`:
  - `browse(table, filter, sort, page)` runs in a read-only transaction
    and returns the rows, total count (estimated when large) and next
    cursor.
  - `preview(change_set | filter)` returns the SQL text.
  - `apply(change_set)` runs in a read-write transaction and returns
    per-row results.
- A `sql_builder` module: an identifier-quoting builder over the
  introspection model, with exhaustive unit tests including hostile
  identifiers.
- Limits: at most 500 changes per apply and 1,000 rows per page; the
  usual per-user rate limits apply.

**API endpoints**

| Method | Path | Purpose |
| --- | --- | --- |
| POST | `/api/connections/{id}/data/browse` | `{schema, table, filter, sort, page_size, cursor}` → rows |
| POST | `/api/connections/{id}/data/preview` | `{schema, table, filter?, changes?}` → generated SQL |
| POST | `/api/connections/{id}/data/apply` | `{schema, table, changes, confirm}` → per-row results |
| POST | `/api/connections/{id}/data/export` | `{schema, table, filter, keys?, format}` → file stream |

**Database changes:** none required. Each apply is recorded in
`query_history` with a `kind` column (see Phase 5) until Phase 10 adds
audit events.

**Acceptance criteria**
- Users can filter (`age > 25`), sort, paginate, edit a cell, insert a row
  and delete a row, and see the exact SQL before applying.
- Applies are atomic: one failure rolls back everything.
- Read-only connections and tables without a primary key cannot be
  edited.
- Hostile identifiers and values cannot inject SQL (fuzz test).

**Tests**
- `sql_builder` unit and property tests, including quoting and every
  operator.
- Apply integration tests: success, conflict, constraint violation and
  rollback.
- Browse pagination correctness.
- Component tests for the filter builder and the change set.
- An end-to-end edit flow in the browser.

---

## Phase 5 — Query History & Workspace Management

**Goal:** make QueryLab a daily-use tool. **Status:** ✅ delivered.

> **Implementation notes:**
> - Collection names are now unique among siblings (`UNIQUE NULLS NOT DISTINCT (user_id, parent_id, name)`) instead of per user. Deleting a folder deletes its subfolders; their queries are kept, without a folder.
> - Folder moves lock the user's folders (`FOR UPDATE`) before the ancestry check, so concurrent moves cannot combine into a cycle.
> - History gains a `kind` filter. Data Studio applies recorded before this phase were backfilled to `data_apply`.
> - Pinning a saved query does not change `updated_at`: the touch trigger now fires only on content changes.
> - Retention and settings use `users.history_retention_days` (1–3650 days, default 90), edited through `GET/PUT /api/workspace/settings`. Pinned history is never pruned.
> - The tab session also stores the active connection. On sign-in the newer side wins, and while several browsers are open the last write wins. Tabs cached locally for another account are never uploaded.
> - Duration filtering uses presets instead of a slider, and dates use the browser's date inputs. No dependency was added.

| Feature | Status | Notes |
| --- | --- | --- |
| Complete execution history | ✅ | Success, error, timeout and cancelled; keyset pagination |
| Search; filter by database and status | ✅ | |
| Filter by duration | ✅ | `min_ms` / `max_ms` |
| Filter by date range | ✅ | `from` / `to` |
| Re-run previous queries | ✅ | "Run again" and "Open in editor" |
| Pin frequently used queries | ✅ | Pins on saved queries and on history entries |
| Save queries; folders and collections | ✅ | Nested folders, with a depth limit of 3 |
| Query descriptions and tags | ✅ | Descriptions, and tags with filtering |
| Restore previously opened tabs | ✅ | Server-side tab sessions follow the user across devices; local storage is the offline cache |
| History retention setting | ✅ | Automatic pruning older than N days, configurable per user |

**Frontend tasks**
- History filters: a duration range slider and a date range picker.
- Pin toggles, and a **Pinned** section on the dashboard and in the
  editor's quick-open (Ctrl+P).
- Saved queries: a folder tree, a tag chip editor and tag filtering.
- Tab sync: debounced save of open tabs (title, SQL, connection) to the
  server, with restore on login. Local storage remains as an offline
  cache.

**Backend tasks**
- Migrations as below; history filters use indexed predicates.
- Tag handling: normalized to lowercase and trimmed; at most 20 tags of 40
  characters each.
- Retention: the existing cleanup task prunes `query_history` older than
  the user's setting, 90 days by default.

**API endpoints**

| Method | Path | Purpose |
| --- | --- | --- |
| *(changed)* GET | `/api/history` | Adds `min_ms`, `max_ms`, `from`, `to` and `pinned` |
| PUT | `/api/history/{id}/pin`, `/api/queries/{id}/pin` | Pin or unpin (`{pinned: bool}`) |
| GET | `/api/tags` | The user's tags with counts |
| *(changed)* | `/api/queries` | `tags`, `pinned`, folder `parent_id` on collections |
| GET, PUT | `/api/workspace/tabs` | Load or save the open-tab session (body ≤ 2 MiB) |

**Database changes**

```sql
ALTER TABLE query_history ADD COLUMN pinned boolean NOT NULL DEFAULT false,
                          ADD COLUMN kind text NOT NULL DEFAULT 'query';  -- query | data_apply | migration | import
ALTER TABLE saved_queries ADD COLUMN pinned boolean NOT NULL DEFAULT false,
                          ADD COLUMN tags text[] NOT NULL DEFAULT '{}';
CREATE INDEX saved_queries_tags_idx ON saved_queries USING gin (tags);
ALTER TABLE query_collections ADD COLUMN parent_id uuid,
  ADD FOREIGN KEY (parent_id, user_id) REFERENCES query_collections (id, user_id) ON DELETE CASCADE;
CREATE TABLE workspace_sessions (user_id uuid PRIMARY KEY REFERENCES users ON DELETE CASCADE,
  tabs jsonb NOT NULL, updated_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX query_history_duration_idx ON query_history (user_id, execution_time_ms);
```

**Acceptance criteria**
- Filter combinations return correct pages.
- Pinned items appear in every entry point.
- Tabs restore on a second browser after login.
- Folder cycles are impossible: the server checks ancestry on move.

**Tests**
- Repository tests for filters, tags and folder moves (cycle rejection).
- Tab session size limits.
- Component tests for the tag editor and pinning.

**Release 1 is complete when Phases 1–5 are accepted.**

---

## Phase 6 — Schema Visualization & ER Diagrams

**Goal:** make complex databases easy to understand visually.
**Status:** ✅ delivered. **Depends on:** Phase 3.

> **Implementation notes:**
> - Dependencies added: `@xyflow/react` (canvas), `elkjs` (layout, loaded on first use) and `html-to-image` (export, loaded on first use). PDF goes through the browser's print dialog, so `jspdf` was not needed.
> - One-to-one is inferred when a valid, non-partial, expression-free unique index covers a subset of the foreign key columns (`INCLUDE` columns do not count). A partial unique index (for example "one open loan per copy") stays one-to-many.
> - Without a selection, schemas over 150 tables show the 150 tables with the most foreign keys; a selection can hold up to 500. Foreign keys to tables that are not shown are counted (`hidden_edges`), not drawn; cross-schema edges are not drawn in this phase.
> - Positions are keyed by table name. Saving merges with positions saved earlier, so tables outside the current selection keep their place. Tables without a saved position are laid out together and placed below the saved ones.
> - The layout table is keyed by `(connection_id, schema_name)` with a composite foreign key to the owning connection, so deleting a connection deletes its layouts.
> - `scripts/sample-db/03-diagram-fixtures.sql` adds a `library` schema covering every notation case.

| Feature | Status | Notes |
| --- | --- | --- |
| Generate ER diagrams automatically | ✅ | From the introspection snapshot; per schema or for selected tables |
| Tables as nodes, FK relationships as edges | ✅ | Nodes list columns with PK/FK icons |
| One-to-one and one-to-many relationships | ✅ | Inferred: an FK with a unique or PK constraint on its columns is 1:1, otherwise 1:N; nullability shows optionality (crow's foot notation) |
| Drag-and-drop positioning, zoom, pan, search | ✅ | Positions are saved per connection and schema |
| Click a table to inspect its columns | ✅ | Side panel reusing the explorer's table details |
| Export to PNG, SVG and PDF | ✅ | Done client-side |

**Proposed dependencies (for review):**
- `@xyflow/react` for the node and edge canvas, zoom, pan and minimap.
- `elkjs` for automatic layout.
- `html-to-image` for PNG and SVG export.
- PDF export reuses the SVG through a print stylesheet, or `jspdf` if a
  vector PDF is required.

**Frontend tasks**
- An `/diagrams` route with a schema selector and a table multi-select.
  Large schemas (more than 150 tables) start with a filtered subset and a
  warning.
- Custom node component with the table header, columns and key markers;
  edge component with cardinality markers.
- Auto-layout (ELK layered), a **Reset layout** action, and highlighting
  of the search target and its neighbours.
- Export menu. Exports use the current theme or a forced light theme.

**Backend tasks**
- `GET /schema-graph` returns tables, columns, keys and relationships with
  inferred cardinality from the introspection model in a single round
  trip.
- Layout persistence.

**API endpoints**

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/connections/{id}/schema-graph?schema=&tables=` | Nodes and edges with cardinality |
| GET, PUT | `/api/connections/{id}/diagram-layouts/{schema}` | Saved node positions |

**Database changes**

```sql
CREATE TABLE diagram_layouts (
  user_id uuid NOT NULL, connection_id uuid NOT NULL, schema_name text NOT NULL,
  positions jsonb NOT NULL, updated_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (connection_id, schema_name),
  FOREIGN KEY (connection_id, user_id) REFERENCES database_connections (id, user_id) ON DELETE CASCADE);
```

**Acceptance criteria**
- The sample database renders with correct cardinalities.
- A 150-table schema lays out in under 3 seconds and pans smoothly.
- Exports match the screen.
- Saved positions restore.

**Tests**
- Cardinality inference unit tests.
- A component test rendering a known graph.
- A browser screenshot check for export.

---

## Phase 7 — Query Performance & Monitoring

**Goal:** understand and troubleshoot database performance.

| Feature | Status | Notes |
| --- | --- | --- |
| EXPLAIN; EXPLAIN ANALYZE with confirmation | ✅ | |
| Visual plan tree; sequential and index scan detection; join analysis | ✅ | Hot nodes, misestimates, spill warnings |
| Query execution duration; slow query identification | ✅ | Dashboard shows the slowest queries |
| Query performance history | ◐ | **Add:** per-query timelines, grouped by normalized SQL fingerprint, showing p50/p95 over time |
| Plan comparison | ☐ | Compare two saved plans side by side |
| Server statistics from `pg_stat_statements` | ☐ | Top queries by total and mean time, when the extension is visible |
| Active database connections | ☐ | `pg_stat_activity`: state, wait events, durations, query text (truncated) |
| Database size statistics | ☐ | Database, schema, table and index sizes; bloat estimate (optional) |
| Lock and blocking session inspection | ☐ | `pg_locks` plus `pg_blocking_pids()` as a blocking tree |
| Cancel or terminate a backend | ☐ | Privileged: writable connection, confirmation and the right role |

**Design notes**
- Monitoring queries are read-only and bounded. Results depend on the
  connected role's privileges. Without `pg_read_all_stats` or
  `pg_monitor`, other sessions' query text is hidden; the UI explains
  this instead of failing.
- Fingerprinting: normalize literals, whitespace and case in Rust (a small
  tokenizer, not a full parser), then hash. The fingerprint is stored on
  `query_history`.
- No background polling of user databases. Monitoring views refresh while
  open, every 5 seconds at most, and the server limits refresh frequency
  per connection.

**Frontend tasks**
- A `/monitoring` route with tabs for Activity, Locks, Sizes and Top
  Queries, auto-refresh controls and a privilege notice.
- A `/analyzer` route for the query performance timeline per fingerprint,
  using the existing Recharts palette, plus a plan comparison view.
- Saving plans from the plan view ("Save plan") for comparison.

**Backend tasks**
- `services/monitoring` with one function per view, each running in a
  read-only transaction with a short statement timeout (5 s).
- A fingerprint function plus a backfill for existing history.
- Cancel and terminate wrap `pg_cancel_backend` and
  `pg_terminate_backend`. They require a writable connection and an
  explicit `confirm`, and they are audited.

**API endpoints**

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/connections/{id}/monitor/activity` | Sessions |
| GET | `/api/connections/{id}/monitor/locks` | Locks and blocking tree |
| GET | `/api/connections/{id}/monitor/sizes?schema=` | Size statistics |
| GET | `/api/connections/{id}/monitor/top-queries` | `pg_stat_statements` (404 with a reason if unavailable) |
| POST | `/api/connections/{id}/monitor/backends/{pid}/cancel` | Privileged |
| POST | `/api/connections/{id}/monitor/backends/{pid}/terminate` | Privileged |
| GET | `/api/performance/fingerprints?connection_id=&days=` | Performance history |
| GET, POST, DELETE | `/api/plans`, `/api/plans/{id}` | Saved plans for comparison |

**Database changes**

```sql
ALTER TABLE query_history ADD COLUMN fingerprint bigint;
CREATE INDEX query_history_fingerprint_idx ON query_history (user_id, fingerprint, executed_at);
CREATE TABLE saved_plans (id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users ON DELETE CASCADE,
  connection_id uuid, sql_content text NOT NULL, plan jsonb NOT NULL, analyzed boolean NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (connection_id, user_id) REFERENCES database_connections (id, user_id) ON DELETE SET NULL (connection_id));
```

**Acceptance criteria**
- A deliberately blocked session appears in the blocking tree within one
  refresh.
- The size breakdown matches `pg_total_relation_size`.
- Fingerprints group equivalent queries.
- Insufficient privileges show an explanation, not an error.

**Tests**
- Fingerprint unit tests.
- A monitoring integration test that creates a blocking lock between two
  connections and asserts the tree.
- A privilege test using the least-privileged sample role.

---

## Phase 8 — Schema Diff & Migration Manager

**Goal:** support real database development workflows. **Status:** ☐ new.
**Depends on:** Phases 2 and 3.

**Schema comparison**

| Feature | Notes |
| --- | --- |
| Compare two databases, e.g. development vs staging | Source and target connections, plus schema filters |
| Detect missing tables, column changes, missing indexes, constraint differences | Also functions, triggers, views, enums and sequences |
| Generate migration SQL | Ordered by dependency; destructive statements flagged |

**Migration management**

| Feature | Notes |
| --- | --- |
| Create migration scripts | Written by hand, or generated from a diff |
| Track versions and applied migrations | Per target connection |
| Migration history | Who applied which migration, when, duration and outcome |
| Rollback scripts where feasible | Generated inverses for additive changes; destructive changes need a hand-written down script |
| Apply with explicit confirmation | Environment guardrails apply; production requires typed confirmation |

**Design**
- **Diff engine:**
  - It takes two introspection snapshots and produces typed changes such
    as `AddTable`, `DropColumn`, `AlterColumnType`, `AddIndex` and
    `AddConstraint`.
  - Objects are matched by name. A table rename can't be told apart from a
    drop plus an add, so it is offered as a manual hint.
- **Migration SQL generation:**
  - Changes are ordered topologically: types, then tables, then columns,
    then constraints, then indexes, then views, functions and triggers.
    Drops run in reverse order.
  - Destructive changes (drops, narrowing type changes) are flagged and
    left out unless explicitly included.
  - Index creation offers `CONCURRENTLY`, which runs outside the
    transaction and is noted in the plan.
- **Applying a migration:**
  - The migration runs in one transaction where possible.
  - A tracking table in the target database,
    `querylab.schema_migrations (version, name, checksum, applied_at,
    applied_by, duration_ms)`, is the source of truth for applied state.
    It travels with the database, so tools and environments agree.
  - The metadata database keeps the scripts and the run history.
  - A checksum mismatch on an already-applied version blocks further
    applies.
- **Concurrency:** a PostgreSQL advisory lock
  (`pg_advisory_xact_lock(hash('querylab_migrations'))`) prevents two
  applies at once.

**Frontend tasks**
- A `/schema-diff` route: pick two connections and schemas, then see a
  tree of differences, side-by-side DDL for each object and the generated
  script, with **Save as migration**.
- A `/migrations` route listing a project's migration scripts (versioned
  and editable until applied), applied status per connection, the apply
  flow (dry-run preview, then confirmation) and the history timeline.

**Backend tasks**
- `services/schema_diff`, with `diff(snapshot_a, snapshot_b)` and
  `to_sql(changes)`.
- `services/migrations`: CRUD for scripts, status per connection (read the
  tracking table, which may not exist yet), and `apply(version, confirm)`.
  Apply takes the advisory lock, creates the tracking schema on first use,
  runs inside a transaction with the statement timeout and records the
  result.
- Rollback generator for reversible changes.

**API endpoints**

| Method | Path | Purpose |
| --- | --- | --- |
| POST | `/api/schema-diff` | `{source:{connection_id,schemas}, target:{…}}` → changes and SQL |
| GET, POST | `/api/migration-projects` | Groups of migrations, e.g. per application |
| GET, POST | `/api/migration-projects/{id}/migrations` | List or create scripts |
| PUT, DELETE | `/api/migrations/{id}` | Edit or delete, only while not applied anywhere |
| GET | `/api/migration-projects/{id}/status?connection_id=` | Applied, pending and checksum drift |
| POST | `/api/migrations/{id}/apply` | `{connection_id, confirm, confirm_name?}`; privileged |
| POST | `/api/migrations/{id}/rollback` | Privileged, only when a down script exists |
| GET | `/api/migration-runs?project_id=&connection_id=` | History |

**Database changes**

```sql
CREATE TABLE migration_projects (id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users ON DELETE CASCADE,
  name text NOT NULL, created_at timestamptz NOT NULL DEFAULT now(), UNIQUE (user_id, name), UNIQUE (id, user_id));
CREATE TABLE migrations (id uuid PRIMARY KEY, project_id uuid NOT NULL, user_id uuid NOT NULL,
  version bigint NOT NULL, name text NOT NULL, up_sql text NOT NULL, down_sql text,
  checksum bytea NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (project_id, version),
  FOREIGN KEY (project_id, user_id) REFERENCES migration_projects (id, user_id) ON DELETE CASCADE);
CREATE TABLE migration_runs (id uuid PRIMARY KEY, migration_id uuid NOT NULL REFERENCES migrations ON DELETE CASCADE,
  connection_id uuid, user_id uuid NOT NULL, direction text NOT NULL CHECK (direction IN ('up','down')),
  status text NOT NULL, error_message text, duration_ms bigint, started_at timestamptz NOT NULL DEFAULT now());
```

**Acceptance criteria**
- Diffing the sample database against a modified copy finds every seeded
  difference.
- Applying the generated script makes a second diff empty.
- Applies are serialized by the advisory lock.
- A production apply requires typed confirmation.

**Tests**
- Diff and SQL generation unit tests for each change type.
- A round-trip integration test: diff, apply, diff again and expect no
  changes.
- Checksum drift.
- Concurrent apply: one waits or fails cleanly.
- Rollback of reversible changes.

---

## Phase 9 — Import, Export & Mock Data

**Goal:** make QueryLab useful for testing and data management.
**Status:** ◐. Results export to CSV and copy as TSV exist; everything
else is new. **Depends on:** Phases 3 and 4.

| Area | Feature | Notes |
| --- | --- | --- |
| Import | CSV and JSON (array or NDJSON) | Uploaded files up to 50 MiB, streamed |
| | Column mapping, data type detection, preview | First 1,000 rows sampled; types inferred and checked against the target columns |
| | Duplicate handling | `error`, `skip` (`ON CONFLICT DO NOTHING`) or `update` (`ON CONFLICT DO UPDATE`) on a chosen unique key |
| | Error reporting | Per-row errors with line numbers; all-or-nothing or skip-invalid modes |
| Export | CSV, JSON, SQL `INSERT`, Excel (.xlsx) | Whole tables, filtered sets (Phase 4 filters) or selected rows |
| Mock data | Users, products, orders, realistic timestamps | Generators chosen per column (name, email, price, date range, enum, FK reference) |
| | Respects foreign keys; configurable row counts | Tables filled in topological order; FK values drawn from existing or generated parents |

**Design**
- **Imports** are privileged:
  - The client uploads the file and receives an `import_id` and a preview.
  - Mappings are confirmed, and the data is loaded through
    `COPY … FROM STDIN` into a temporary staging table inside a
    transaction. It is then merged into the target with `INSERT … SELECT`
    and the chosen conflict handling.
  - Progress is reported through polling (`GET /imports/{id}`).
  - Files are held in memory or temporary storage only for the length of
    the import and deleted afterwards. There is no persistent file
    storage.
- **Exports:**
  - Data is streamed with `COPY (SELECT …) TO STDOUT`, using the
    structured filter, so memory stays flat.
  - Formats are converted on the fly: CSV directly, JSON and `INSERT`
    statements row by row.
  - Excel is built with `rust_xlsxwriter` and capped at 200,000 rows, the
    practical limit for one sheet.
- **Mock data:**
  - Generated in Rust with the `fake` crate (proposed, for review) and
    batched into `INSERT` statements or `COPY`.
  - Capped at 100,000 rows per table and 1,000,000 per run.
  - Runs in a single transaction with a preview of sample rows.
  - Production connections are refused unless an explicit override is
    configured. The default is to never allow mock data in production.

**Frontend tasks**
- Import wizard: upload, mapping (with type badges and mismatch
  warnings), options, preview, run with a progress display, then a report.
- Export dialog, available from results, the Data Studio and the
  explorer: format, scope (all, filtered or selected) and options.
- Mock data designer: pick tables (FK dependencies are added
  automatically), choose generators per column, set row counts, preview,
  then generate.

**Backend tasks**
- `services/import`: parsing with the `csv` crate (proposed) and
  `serde_json`, type inference, staging-table `COPY` (sqlx
  `copy_in_raw`), merge SQL from `sql_builder`, and a progress registry
  modelled on the execution registry.
- `services/export`: streaming response bodies, with formats as encoders
  over row streams.
- `services/mock_data`: a generator registry keyed by column type and
  name heuristics, FK graph ordering, and handling of self-references and
  cycles. Cyclic dependencies are refused with an explanation.

**API endpoints**

| Method | Path | Purpose |
| --- | --- | --- |
| POST | `/api/connections/{id}/imports` | Multipart upload → `{import_id, preview, inferred_types}` |
| POST | `/api/imports/{import_id}/run` | `{schema, table, mapping, duplicate_mode, error_mode, confirm}`; privileged |
| GET | `/api/imports/{import_id}` | Progress and report |
| DELETE | `/api/imports/{import_id}` | Cancel and discard |
| POST | `/api/connections/{id}/exports` | `{source: table/filter/sql, format}` → streamed file |
| POST | `/api/connections/{id}/mock-data/preview` | Plan and sample rows |
| POST | `/api/connections/{id}/mock-data/run` | Privileged; refused on production |

**Database changes**

```sql
CREATE TABLE data_jobs (id uuid PRIMARY KEY, user_id uuid NOT NULL REFERENCES users ON DELETE CASCADE,
  connection_id uuid, kind text NOT NULL CHECK (kind IN ('import','mock_data','export')),
  status text NOT NULL, target text, rows_processed bigint NOT NULL DEFAULT 0, rows_failed bigint NOT NULL DEFAULT 0,
  report jsonb, started_at timestamptz NOT NULL DEFAULT now(), finished_at timestamptz,
  FOREIGN KEY (connection_id, user_id) REFERENCES database_connections (id, user_id) ON DELETE SET NULL (connection_id));
```

The API configuration also gets a new upload limit, `IMPORT_MAX_BYTES`
(50 MiB). Its own `DefaultBodyLimit` applies to the import route only.

**Acceptance criteria**
- A 100 MB CSV export streams with flat memory.
- A 50 MiB CSV import with mixed valid and invalid rows produces an
  accurate report in both error modes.
- Mock data for the sample schema satisfies every FK and CHECK
  constraint.
- Mock data on production is refused.

**Tests**
- Parser and type-inference unit tests.
- Import integration tests for each duplicate and error mode.
- Export format golden files.
- FK-ordering and constraint satisfaction for mock data.
- Memory regression check on large exports in CI, with a smaller fixture.

---

## Phase 10 — Security, Collaboration & Deployment

**Goal:** a SaaS-ready platform.

| Feature | Status | Notes |
| --- | --- | --- |
| User authentication | ✅ | **Add:** password change, password reset by email, session list with revoke, optional TOTP 2FA |
| Rate limiting | ◐ | In memory per instance. **Move to Redis** (Upstash) for multi-instance deployments |
| Connection isolation; credential encryption | ✅ | |
| Read-only and read-write permissions | ◐ | Per connection today. **Add:** per-member permissions in workspaces |
| Team workspaces | ☐ | Personal workspace by default, plus team workspaces with invitations |
| Role-based access control | ☐ | Roles `owner`, `admin`, `editor`, `viewer`, and per-connection grants (`none`, `read`, `write`, `admin`) |
| Shared saved queries and SQL snippets | ☐ | Queries and collections shared in a workspace; snippets as reusable templates with placeholders |
| Database access permissions | ☐ | Members use shared connections without ever seeing the credentials |
| Query audit logs | ☐ | Append-only events for executions, privileged operations, permission changes and logins |
| Production monitoring; error tracking | ◐ | Structured logs and request ids exist. **Add:** a metrics endpoint and a hook for an error-tracking service |
| Automated tests | ✅ | **Extend:** permission matrix tests and browser end-to-end tests in CI |

**Design**
- **Workspaces:**
  - Each existing user gets a personal workspace, created by the
    migration.
  - Owned resources (connections, collections, saved queries, groups,
    migrations, layouts, plans and jobs) gain a `workspace_id` that is
    backfilled.
  - Authorization becomes "the user is a member of the resource's
    workspace with sufficient role or grant", centralized in one
    `authorize(user, resource, action)` service with exhaustive tests.
  - History stays per user, but admins can view it through audit logs.
- **Shared connections:** a credential stays encrypted under its
  connection id. Members can use it but can never read it: no endpoint
  returns secrets. Write access needs both the connection's `read_only =
  false` and the member's `write` grant.
- **Audit events:**
  - Written in the same transaction as the action wherever possible, and
    otherwise logged with a retry.
  - The table is insert-only for the application role: `REVOKE UPDATE,
    DELETE`.
  - Executed SQL is stored truncated to 10 KB, and its parameters are
    never stored.
- **Scaling:** move the rate limiters and the execution registry (which
  handles cancellation and concurrency) behind Redis. Cancellation is
  routed by storing the backend pid in Redis, so any instance can issue
  `pg_cancel_backend`. Then raise the instance count above 1.

**Frontend tasks**
- A workspace switcher in the sidebar; workspace settings (members,
  invitations, roles); per-connection access grants.
- Shared badges on queries and collections; a snippet library with
  placeholder prompts when a snippet is inserted.
- An **Audit Logs** page with filters (actor, action, connection, date)
  and CSV export.
- Account security: sessions list with revoke, password change, 2FA
  enrolment.

**Backend tasks**
- The workspace and RBAC migration and backfill, the `authorize` service,
  and refactoring every handler to call it. This is the largest change in
  the plan; it is done as its own sub-phase behind tests.
- Invitations: tokenized, expiring and single-use. Email sending goes
  through a provider abstraction, with Resend or SES configurable and a
  no-op in development.
- Audit event writer; the Redis-backed limiter and registry with the
  in-memory versions as fallback; `/metrics` (Prometheus format,
  protected by a token).

**API endpoints (summary)**

| Method | Path | Purpose |
| --- | --- | --- |
| GET, POST | `/api/workspaces` | List or create workspaces |
| GET, PUT, DELETE | `/api/workspaces/{id}` | Settings |
| GET, POST, PUT, DELETE | `/api/workspaces/{id}/members[/{user_id}]` | Membership and roles |
| POST | `/api/workspaces/{id}/invitations`, `/api/invitations/{token}/accept` | Invitations |
| PUT | `/api/connections/{id}/grants/{user_id}` | Per-connection access |
| GET, POST, PUT, DELETE | `/api/snippets[/{id}]` | Snippets |
| GET | `/api/audit-events?workspace_id=&actor=&action=&from=&to=` | Audit log |
| GET, DELETE | `/api/auth/sessions[/{id}]` | Session management |
| POST | `/api/auth/password`, `/api/auth/password-reset[/confirm]` | Password change and reset |
| POST | `/api/auth/2fa/enroll`, `/api/auth/2fa/verify` | TOTP |
| GET | `/metrics` | Operational metrics (token required) |

**Database changes**

```sql
CREATE TABLE workspaces (id uuid PRIMARY KEY, name text NOT NULL, personal boolean NOT NULL DEFAULT false,
  created_by uuid REFERENCES users ON DELETE SET NULL, created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE workspace_members (workspace_id uuid REFERENCES workspaces ON DELETE CASCADE,
  user_id uuid REFERENCES users ON DELETE CASCADE,
  role text NOT NULL CHECK (role IN ('owner','admin','editor','viewer')), PRIMARY KEY (workspace_id, user_id));
CREATE TABLE connection_grants (connection_id uuid REFERENCES database_connections ON DELETE CASCADE,
  user_id uuid REFERENCES users ON DELETE CASCADE,
  access text NOT NULL CHECK (access IN ('read','write','admin')), PRIMARY KEY (connection_id, user_id));
CREATE TABLE workspace_invitations (id uuid PRIMARY KEY, workspace_id uuid REFERENCES workspaces ON DELETE CASCADE,
  email text NOT NULL, role text NOT NULL, token_hash bytea NOT NULL UNIQUE, expires_at timestamptz NOT NULL,
  accepted_at timestamptz);
CREATE TABLE snippets (id uuid PRIMARY KEY, workspace_id uuid REFERENCES workspaces ON DELETE CASCADE,
  created_by uuid REFERENCES users ON DELETE SET NULL, name text NOT NULL, body text NOT NULL,
  tags text[] NOT NULL DEFAULT '{}', created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE audit_events (id bigserial PRIMARY KEY, workspace_id uuid, actor_id uuid, action text NOT NULL,
  target_type text, target_id uuid, connection_id uuid, detail jsonb, ip inet, request_id uuid,
  created_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX audit_events_ws_time_idx ON audit_events (workspace_id, created_at DESC);
-- Every owned table: ADD COLUMN workspace_id uuid REFERENCES workspaces; backfill from personal workspaces; then NOT NULL.
ALTER TABLE users ADD COLUMN totp_secret_encrypted bytea, ADD COLUMN settings jsonb NOT NULL DEFAULT '{}';
```

**Acceptance criteria**
- Every endpoint is covered by a permission-matrix test of role × grant ×
  action.
- Viewers cannot write.
- Members never receive credentials.
- Audit events exist for every privileged action and cannot be modified
  by the application role.
- Two API instances share rate limits and can cancel each other's
  queries.

**Tests**
- Permission matrix generated per endpoint.
- Invitation token lifecycle; TOTP.
- Audit immutability.
- Redis-backed limiter tests, run in CI with a Redis service.
- Multi-instance cancellation in an integration test.

**Release 3 is complete when Phases 9–10 are accepted.**

---

## Target navigation

```
QUERYLAB                          ● Connected · <connection> · <environment>

WORKSPACE            DATABASE TOOLS        MANAGEMENT
  Dashboard            ER Diagrams           Team Workspace
  SQL Editor           Query Analyzer        Audit Logs
  Database Explorer    Schema Diff           Settings
  Table Editor         Migrations
  Query History        Monitoring
  Saved Queries        Import / Export
```

Entries appear as their phase ships. Unfinished tools are not shown as
disabled placeholders.

---

## Cross-cutting testing strategy

| Layer | Tooling | Applies to |
| --- | --- | --- |
| Rust unit tests | `cargo test` | SQL builder, diff engine, fingerprinting, parsers, generators |
| Rust integration tests | `#[sqlx::test]` with the seeded sample database in CI | Every new endpoint: happy path, validation, ownership (404 for other users), privileged guards, limits |
| Frontend unit tests | Vitest | Stores and pure logic (filter model, change sets, cardinality) |
| Frontend component tests | Vitest + Testing Library (jsdom) | Dialogs, builders and confirmation flows |
| Browser end-to-end tests | Headless browser against a local stack | One golden path per phase, added to CI in Phase 10 |
| Security tests | `tests/security_api.rs` pattern | Injection fuzzing for generated SQL, cross-user access, production guardrails |

**Definition of done for every phase:**
- All checks pass: `cargo fmt`, `cargo clippy -D warnings`, `cargo test --
  --include-ignored`, `npm run lint`, `npm run typecheck`, `npm test` and
  `npm run build`.
- The README and this plan are updated.
- The phase is delivered through a pull request from a feature branch.
  CI must pass; it can also be started manually for any branch.

---

## Deployment considerations

| Concern | Plan |
| --- | --- |
| Topology | Vercel (frontend, `/api` rewrite) → Render or Railway (API container) → Neon (metadata). See `docs/DEPLOYMENT.md`. |
| Instance count | **1** until Phase 10 moves limits and the execution registry to Redis |
| Migrations | Embedded and applied at startup. Changes must be additive and backwards compatible within a release: new columns get defaults, and nothing is dropped in the same release. |
| Upload sizes (Phase 9) | The Vercel rewrite has request size and duration limits. Large imports and exports go directly to the API origin over CORS (`CORS_ORIGINS`) with the CSRF header, or through signed short-lived URLs. To be decided in Phase 9 after measuring. |
| Long requests | Monitoring and diff requests are short-lived. Imports, mock data and large exports run as jobs with polling, which avoids proxy timeouts. |
| Secrets | `ENCRYPTION_KEY` is required. Redis credentials (Phase 10) and the email provider key (invitations) are new. |
| Observability | JSON logs with request and user ids exist. Phase 10 adds `/metrics` and an error-tracking hook. Alert on `level=ERROR` until then. |
| Free-tier cold starts | Render free instances sleep. Options: a keep-alive ping, a paid instance, or clearer "server waking up" handling in the client. |
| Data protection | Audit log retention and history retention are configurable. Exported files are never stored server-side. |
