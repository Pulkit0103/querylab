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
cd frontend && npm run lint && npm run build
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

## Configuration (backend)

| Variable                   | Default                  | Description |
| -------------------------- | ------------------------ | ----------- |
| `DATABASE_URL`             | —                        | Metadata database (required) |
| `ENCRYPTION_KEY`           | —                        | Base64 32-byte key (required; dev key rejected in production) |
| `ENCRYPTION_KEY_ID`        | `1`                      | Id stamped on new ciphertexts |
| `ENCRYPTION_PREVIOUS_KEYS` | —                        | `id:base64,...` keys accepted for decryption |
| `RUN_MIGRATIONS`           | `true`                   | Apply migrations on startup |
| `APP_ENV`                  | `development`            | `production` enables JSON logs and strict checks |
| `HOST` / `PORT`            | `0.0.0.0` / `8080`       | Bind address |
| `CORS_ORIGINS`             | `http://localhost:3000`  | Comma-separated origins for direct API access |
| `LOG_FORMAT`               | `pretty` (dev)           | `pretty` or `json` |
| `REQUEST_TIMEOUT_SECS`     | `120`                    | Hard per-request timeout |

## API

| Method | Path           | Description                                        |
| ------ | -------------- | -------------------------------------------------- |
| GET    | `/health`      | Liveness + metadata DB readiness (503 if degraded) |
| GET    | `/api/health`  | Same, reachable through the frontend proxy         |
| GET    | `/api/version` | Build and API version                              |

## License

MIT
