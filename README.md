# numble

Small full-stack auth demo:
- Frontend: Next.js (port 3000)
- Backend: Rust + Axum (port 3001)

## Quick start (Docker)

From the project root:

```bash
docker compose up -d --build
```

Open:
- Frontend: http://localhost:3000
- Backend: http://localhost:3001

Stop everything:

```bash
docker compose down
```

## Run locally (without Docker)

### Backend

```bash
cd backend
$env:JWT_SECRET="dev-secret-change-me"
$env:DB_PATH="./users_db"
cargo run
```

### Frontend

In a second terminal:

```bash
cd front
npm install
npm run dev
```

## API endpoints

- `POST /auth/register`
- `POST /auth/login`
- `GET /auth/me` (requires `Authorization: Bearer <token>`)

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

## Notes

- Users are stored in a persistent sled database.
- Use `DB_PATH` to choose where user data is stored.
- In Docker, user data is persisted in the `backend-data` volume.
