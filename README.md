# numble

Small full-stack auth demo:
- Frontend: Next.js (port 3000)
- Backend: Rust + Axum (port 3001)
- Database: PostgreSQL (Docker) with embedded sled fallback for local/dev backup

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
$env:JWT_SECRET="dev-secret-change-me"
$env:DATABASE_URL="postgres://numble:numble@localhost:5432/numble"
$env:DB_PATH="./users_db"
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
- `POST /scores/record` (requires `Authorization: Bearer <token>`, body: `{ "won": true|false }`)
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
- Leaderboard returns the top 10 players sorted by score.

## Roadmap

- Add refresh tokens + rotation for stronger session security.
- Add request tracing and metrics endpoint for observability.
- Add one end-to-end browser test for register -> play -> leaderboard update.
