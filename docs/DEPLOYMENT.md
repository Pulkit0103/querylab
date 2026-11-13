# Deploying QueryLab

Production topology:

```
browser ──HTTPS──▶ Vercel (Next.js) ──/api/* rewrite, HTTPS──▶ Render or Railway (Rust API) ──TLS──▶ Neon (metadata PostgreSQL)
                                                                         └──TLS──▶ user target databases
```

The browser talks only to the Vercel origin. Vercel rewrites `/api/*` to
the API, so:

- the session cookie is first-party: `__Host-ql_session`, which is
  `HttpOnly`, `Secure` and `SameSite=Lax`
- the browser makes no cross-origin calls

The API is a single container image (`backend/Dockerfile`), which has these
properties:

- release build, running as a non-root user
- migrations embedded and applied on start
- `GET /health` for the platform health check

## 1. Metadata database (Neon)

1. Create a Neon project and a database, e.g. `querylab`.
2. Copy the connection string and keep `?sslmode=require`:
   `postgres://USER:PASSWORD@ep-xxx.region.aws.neon.tech/querylab?sslmode=require`
3. Migrations run automatically when the API starts (`RUN_MIGRATIONS=true`).
   The database role needs to be able to create tables in its database. The
   Neon owner role can.

> Use the **direct** endpoint, not the one ending in `-pooler`. The API
> keeps its own small connection pool and uses prepared statements, which
> transaction-mode poolers may not support. Keep `DATABASE_MAX_CONNECTIONS`
> below your plan's connection limit.

## 2. Encryption key

Stored database passwords are encrypted with AES-256-GCM under this key.

```bash
openssl rand -base64 32
```

Store it in a secret manager **and back it up**. If the key is lost, every
saved connection must be re-entered. The development key in
`.env.example` is rejected when `APP_ENV=production`.

**Rotating the key:**

1. Generate a new key.
2. Set the new key as `ENCRYPTION_KEY` and give it a new `ENCRYPTION_KEY_ID`.
3. Move the old key to `ENCRYPTION_PREVIOUS_KEYS=1:<old base64>`.

Existing credentials stay readable. A connection is re-encrypted under the
new key when its password is updated. Keep the old key until every
connection has been updated.

## 3. API on Render

The repository includes a Blueprint (`render.yaml`).

1. Go to **New → Blueprint**, select the repository and apply it. This
   creates `querylab-api` from `backend/Dockerfile`, with `/health` as the
   health check.
2. Fill in the variables marked *sync: false*:
   - `DATABASE_URL`: the Neon URL
   - `ENCRYPTION_KEY`: from step 2
   - `CORS_ORIGINS`: your Vercel URL, e.g. `https://querylab.vercel.app`
3. Deploy and confirm the health check:
   `curl https://querylab-api.onrender.com/health`, which should return
   `{"status":"ok",...,"database":"ok"}`.

## 3b. API on Railway (alternative)

1. Create a new service from the repository and set **Root Directory** to
   `backend`.
2. Set **Config file path** to `/backend/railway.json`. This selects the
   Dockerfile builder, the `/health` health check and the restart policy.
3. Add the variables from the table below. Railway injects `PORT`, which
   the API reads.
4. Generate a public domain under **Settings → Networking**.

## 4. Frontend on Vercel

1. Import the repository and set **Root Directory** to `frontend`. The
   framework is detected as Next.js.
2. Add the environment variable `BACKEND_URL`, set to the API's public URL
   (e.g. `https://querylab-api.onrender.com`) **for the Production and
   Preview environments**. The `/api` rewrite is resolved at build time, so
   redeploy after changing it.
3. Deploy, then open the site. `https://<your-app>.vercel.app/api/health`
   should return the API health JSON through the rewrite.

## Production environment variables (API)

| Variable | Value | Notes |
| --- | --- | --- |
| `APP_ENV` | `production` | JSON logs, secure cookies, dev key rejected |
| `DATABASE_URL` | Neon URL with `sslmode=require` | required |
| `ENCRYPTION_KEY` / `ENCRYPTION_KEY_ID` | base64 key / `1` | required; back up |
| `CORS_ORIGINS` | `https://<your-app>.vercel.app` | only needed for direct browser calls; harmless otherwise |
| `COOKIE_SECURE` | `true` | the default in production; also enables HSTS |
| `TRUSTED_PROXY_HOPS` | `2` | Vercel rewrite + platform load balancer; see below |
| `ALLOW_PRIVATE_NETWORKS` | `false` | **must stay false** (SSRF protection) |
| `LOG_FORMAT` | `json` | the default in production |
| `RUST_LOG` | `querylab_api=info,tower_http=info,sqlx=warn` | |
| `DATABASE_MAX_CONNECTIONS` / `DATABASE_MIN_CONNECTIONS` | `10` / `2` | stay within the Neon plan's connection limit |
| `REQUEST_TIMEOUT_SECS`, `QUERY_*`, `*_RATE_LIMIT_*` | defaults | see the README configuration table |

**`TRUSTED_PROXY_HOPS`** decides which `X-Forwarded-For` entry counts as
the client address for rate limiting.

- Behind Vercel plus Render or Railway there are two proxies.
- After deploying, check the logs from a few failed logins. Rate limiting
  should key on real client addresses, not on a Vercel egress IP.
- If every client shows the same IP, increase the value by one.
- Never set it higher than the number of proxies you control. Otherwise
  clients can spoof their address.

## HTTPS and CORS

- Vercel, Render and Railway terminate TLS. The API sets `Secure`
  `__Host-` cookies and HSTS when `COOKIE_SECURE=true`.
- In production the browser only talks to Vercel. `CORS_ORIGINS` matters
  only if you call the API directly from another origin, and even then
  state-changing calls also need the `X-QueryLab-CSRF` header.

## Health checks and logging

- `GET /health` (also `/api/health`) returns `200` with
  `"database":"ok"`, or `503` with `"status":"degraded"` while the metadata
  database is unreachable. Both platforms restart or stop routing to
  unhealthy instances.
- Logs are JSON, one line per request, with these fields:
  - `method`, `path`, `status`, `latency_ms`
  - `request_id`, `user_id`
- Server errors and handler panics are logged at `ERROR`. Forward platform
  logs to your log tool, such as Render log streams or Railway log drains,
  and alert on `level=ERROR`.
- Users see the `request_id` as a "reference" in error messages.

## Production smoke test

After every production deploy, check all of the following:

1. `GET /api/health` through the Vercel domain returns `200`.
2. Register a user. The response sets `__Host-ql_session` with `Secure`
   and `HttpOnly` (check in browser dev tools).
3. Add a connection to a **public** database and click **Test**. The test
   reports the server version.
4. Add a connection to `127.0.0.1` or `10.0.0.5`. It is rejected
   (SSRF protection).
5. Run `SELECT 1` and a `DELETE` on a read-only connection. The `DELETE`
   fails with `25006`.
6. Run Explain and Explain Analyze. The confirmation dialog appears and the
   plan renders.
7. Check that history, saved queries and the dashboard show the activity.
8. Log out. `/api/auth/me` returns `401`.

## Scaling notes

- Rate limits and the per-user concurrency limit are held **in memory**.
  So is the registry used to cancel running queries.
- Run **one API instance**. Both blueprints pin replicas to 1.
- To scale out, move the limiter and registry to Redis (e.g. Upstash) and
  route cancellation to the instance running the query.
- Vertical scaling is straightforward. The metadata pool is small. Each
  query uses its own short-lived connection to the target database, so
  load on target databases tracks `MAX_CONCURRENT_QUERIES_PER_USER` × the
  number of active users.

## Running the production image locally

```bash
docker compose --profile full up --build   # API on :8080 + both databases
cd frontend && BACKEND_URL=http://127.0.0.1:8080 npm run build && npm start
```
