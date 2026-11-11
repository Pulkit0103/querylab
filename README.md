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
# Backend
cd backend && cargo test

# Frontend
cd frontend && npm run lint && npm run build
```

## API

| Method | Path          | Description                               |
| ------ | ------------- | ----------------------------------------- |
| GET    | `/health`     | Liveness + metadata DB readiness (503 if degraded) |
| GET    | `/api/health` | Same, reachable through the frontend proxy |

## License

MIT
