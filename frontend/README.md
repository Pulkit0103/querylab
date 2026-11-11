# QueryLab — Frontend

Next.js (App Router) client for QueryLab. See the [root README](../README.md)
for the full architecture and setup.

```bash
cp .env.example .env.local   # BACKEND_URL points at the Rust API
npm install
npm run dev                  # http://localhost:3000
npm run build                # production build
npm run lint
```

## Structure

```
app/          Routes and layouts
components/   UI components (ui/ = shadcn primitives)
hooks/        React hooks (TanStack Query wrappers)
lib/          API client and utilities
stores/       Zustand client state
types/        API contract types (mirrors backend models)
```
