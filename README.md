# numble

Small full-stack auth demo:
- Frontend: Next.js (port 3000)
- Backend: Rust + Axum (port 3001)
- Database: PostgreSQL (Docker) with embedded sled fallback for local/dev backup

## Environment setup

Copy and customize env files before running in serious environments:

```bash
cp .env.example .env
cp backend/.env.example backend/.env
cp front/.env.local.example front/.env.local
```

Security-sensitive values to always customize:
- `JWT_SECRET` (minimum 32 chars outside development)
- `POSTGRES_PASSWORD`
- `DATABASE_URL`
- `CORS_ORIGIN`

## Quick start (Docker)

From the project root:

```bash
docker compose up -d --build
```

Open:
- Frontend: http://localhost:3000
- Backend: http://localhost:3001
- Health: http://localhost:3001/health

Stop everything:

```bash
docker compose down
```

## Run locally (without Docker)

### Backend

```bash
cd backend
$env:APP_ENV="development"
$env:JWT_SECRET="replace-with-a-long-random-secret-at-least-32-chars"
$env:CORS_ORIGIN="http://localhost:3000"
$env:DATABASE_URL="postgres://numble:numble@localhost:5432/numble"
$env:DB_PATH="./users_db"
$env:BIND_ADDRESS="0.0.0.0:3001"
cargo run
```

`DATABASE_URL` is used first (PostgreSQL). If it is missing, the backend falls back to sled at `DB_PATH`.

### Frontend

In a second terminal:

```bash
cd front
npm install
npm run dev
```

## Run tests

### Backend tests

```bash
cd backend
cargo test
```

### Frontend tests

```bash
cd front
npm test
```

## API endpoints

- `POST /auth/register`
- `POST /auth/login`
- `GET /auth/me` (requires `Authorization: Bearer <token>`)
- `POST /scores/record` (requires `Authorization: Bearer <token>`, body: `{ "won": true|false, "guesses_used": 1..6 }`)
- `GET /scores/leaderboard`
- `GET /health`

## Quick API test (PowerShell)

Register:

```powershell
Invoke-RestMethod -Method Post -Uri http://localhost:3001/auth/register `
  -ContentType "application/json" `
  -Body '{"username":"alice","password":"password123"}'
```

Login:

```powershell
$login = Invoke-RestMethod -Method Post -Uri http://localhost:3001/auth/login `
  -ContentType "application/json" `
  -Body '{"username":"alice","password":"password123"}'
$token = $login.access_token
```

/me:

```powershell
Invoke-RestMethod -Method Get -Uri http://localhost:3001/auth/me `
  -Headers @{ Authorization = "Bearer $token" }
```

## Architecture Notes

- Users and scores are persisted in PostgreSQL by default.
- PostgreSQL data is persisted in Docker volume `postgres-data`.
- `DB_PATH` sled storage is available as fallback when `DATABASE_URL` is not set.
- Each finished game updates player stats and score.
- Score is guess-based (fewer guesses means higher points), not fixed increments.
- Leaderboard returns the top 10 players sorted by score.

## Security Defaults Added

- `.env`-driven configuration for secrets and origins.
- CORS restricted to configured `CORS_ORIGIN`.
- Request body size limit (`8 KB`) to reduce abuse surface.
- Response hardening headers:
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY`
  - `Referrer-Policy: no-referrer`
  - `Permissions-Policy` for camera/mic/geolocation
  - strict API `Content-Security-Policy`
- Production guardrail: app panics if `JWT_SECRET` is weak/default outside development.

## Roadmap

- Add refresh tokens + rotation for stronger session security.
- Add request tracing and metrics endpoint for observability.
- Add one end-to-end browser test for register -> play -> leaderboard update.
