"use client";

import { useRouter } from "next/navigation";
import { type ChangeEvent, type KeyboardEvent, useState } from "react";
import { useEffect } from "react";

type MeResponse = {
  username: string;
  score: number;
  wins: number;
  losses: number;
  games_played: number;
};

type LeaderboardEntry = {
  username: string;
  score: number;
  wins: number;
  losses: number;
  games_played: number;
};

type LeaderboardResponse = {
  players: LeaderboardEntry[];
};

type ScoreUpdateResponse = {
  username: string;
  score: number;
  wins: number;
  losses: number;
  games_played: number;
};

type ScoreUpdateRequest = {
  won: boolean;
  guesses_used: number;
};

export default function Home() {
  const router = useRouter();
  const rowSize = 6;
  const totalCells = 36;
  const totalRows = totalCells / rowSize;
  const revealStepMs = 140;
  const revealDurationMs = 420;
  const cells = Array.from({ length: totalCells });
  const [values, setValues] = useState<string[]>(() => Array(36).fill(""));
  const [colors, setColors] = useState<string[]>(Array(36).fill("white"));
  const [currentIndex, setCurrentIndex] = useState(0);
  const [submittedRows, setSubmittedRows] = useState<boolean[]>(
    Array(totalRows).fill(false),
  );
  const [revealedRows, setRevealedRows] = useState<boolean[]>(
    Array(totalRows).fill(false),
  );
  const [animatingRow, setAnimatingRow] = useState<number | null>(null);
  const [hasWon, setHasWon] = useState(false);
  const [target, setTarget] = useState(() =>
    Math.floor(Math.random() * 1000000).toString().padStart(6, "0"),
  );
  const [isAuthorized, setIsAuthorized] = useState(false);
  const [isCheckingAuth, setIsCheckingAuth] = useState(true);
  const [username, setUsername] = useState("");
  const [score, setScore] = useState(0);
  const [leaderboard, setLeaderboard] = useState<LeaderboardEntry[]>([]);
  const [hasRecordedResult, setHasRecordedResult] = useState(false);
  const attemptsUsed = submittedRows.filter(Boolean).length;
  const attemptsLeft = totalRows - attemptsUsed;
  const isGameOver = hasWon || submittedRows.every(Boolean);
  const apiBaseUrl = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3001";

  const fetchLeaderboard = async () => {
    try {
      const response = await fetch(`${apiBaseUrl}/scores/leaderboard`);
      if (!response.ok) {
        return;
      }

      const payload = (await response.json()) as LeaderboardResponse;
      setLeaderboard(payload.players ?? []);
    } catch {
      setLeaderboard([]);
    }
  };

  useEffect(() => {
    const checkAuth = async () => {
      const token = localStorage.getItem("numble_token");
      if (!token) {
        router.replace("/login");
        return;
      }

      try {
        const response = await fetch(`${apiBaseUrl}/auth/me`, {
          headers: {
            Authorization: `Bearer ${token}`,
          },
        });

        if (!response.ok) {
          localStorage.removeItem("numble_token");
          router.replace("/login");
          return;
        }

        const payload = (await response.json()) as MeResponse;
        setUsername(payload.username);
        setScore(payload.score ?? 0);
        await fetchLeaderboard();
        setIsAuthorized(true);
      } catch {
        localStorage.removeItem("numble_token");
        router.replace("/login");
      } finally {
        setIsCheckingAuth(false);
      }
    };

    checkAuth();
  }, [apiBaseUrl, router]);

  useEffect(() => {
    if (!isAuthorized || !isGameOver || hasRecordedResult) {
      return;
    }

    const token = localStorage.getItem("numble_token");
    if (!token) {
      return;
    }

    let isCancelled = false;

    const submitResult = async () => {
      try {
        const response = await fetch(`${apiBaseUrl}/scores/record`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${token}`,
          },
          body: JSON.stringify({
            won: hasWon,
            guesses_used: Math.max(1, attemptsUsed),
          } as ScoreUpdateRequest),
        });

        if (!response.ok) {
          return;
        }

        const payload = (await response.json()) as ScoreUpdateResponse;
        if (!isCancelled) {
          setScore(payload.score ?? 0);
          await fetchLeaderboard();
        }
      } finally {
        if (!isCancelled) {
          setHasRecordedResult(true);
        }
      }
    };

    submitResult();

    return () => {
      isCancelled = true;
    };
  }, [apiBaseUrl, attemptsUsed, hasRecordedResult, hasWon, isAuthorized, isGameOver]);

  if (isCheckingAuth) {
    return (
      <div className="main-layout">
        <header className="numble-header">
          <div>
            <p className="numble-kicker">Guess the hidden number</p>
            <h1>NUMBLE</h1>
          </div>
        </header>
        <section className="numble-panel">
          <p className="game-status">Checking authentication...</p>
        </section>
      </div>
    );
  }

  if (!isAuthorized) {
    return null;
  }

  const resetGame = () => {
    setValues(Array(totalCells).fill(""));
    setColors(Array(totalCells).fill("white"));
    setCurrentIndex(0);
    setSubmittedRows(Array(totalRows).fill(false));
    setRevealedRows(Array(totalRows).fill(false));
    setAnimatingRow(null);
    setHasWon(false);
    setHasRecordedResult(false);
    setTarget(Math.floor(Math.random() * 1000000).toString().padStart(6, "0"));
  };

  const handleLogout = () => {
    localStorage.removeItem("numble_token");
    router.replace("/login");
  };

  const handleChange = (index: number, event: ChangeEvent<HTMLInputElement>) => {
    const activeRow = Math.floor(currentIndex / rowSize);
    const row = Math.floor(index / rowSize);
    if (row !== activeRow || submittedRows[row] || isGameOver) {
      return;
    }

    const nextValue = event.target.value.replace(/\D/g, "").slice(-1);

    setValues((prev) => {
      const next = [...prev];
      next[index] = nextValue;
      return next;
    });

    if (nextValue && index < activeRow * rowSize + (rowSize - 1)) {
      setCurrentIndex(index + 1);
    }
  };

  const handleKeyDown = (index: number, event: KeyboardEvent<HTMLInputElement>) => {
    const row = Math.floor(index / rowSize);
    const activeRow = Math.floor(currentIndex / rowSize);
    const rowStart = row * rowSize;

    if (row !== activeRow || submittedRows[row] || isGameOver) {
      return;
    }

    if (event.key === "Backspace" && !values[index] && index > rowStart) {
      setCurrentIndex(index - 1);
      return;
    }

    if (event.key !== "Enter") {
      return;
    }

    event.preventDefault();

    const rowValues = values.slice(rowStart, rowStart + rowSize);
    if (rowValues.some((value) => value === "")) {
      return;
    }

    const guess = rowValues.join("");
    const isWinningRow = guess === target;

    setColors((prev) => {
      const nextColors = [...prev];

      for (let i = 0; i < rowSize; i++) {
        const cellIndex = rowStart + i;
        const diff = Math.abs(Number(target[i]) - Number(rowValues[i]));

        if (diff === 0) {
          nextColors[cellIndex] = "green";
        } else if (diff < 3) {
          nextColors[cellIndex] = "yellow";
        } else if (diff < 4) {
          nextColors[cellIndex] = "orange";
        } else {
          nextColors[cellIndex] = "red";
        }
      }

      return nextColors;
    });

    setSubmittedRows((prev) => {
      const next = [...prev];
      next[row] = true;
      return next;
    });

    if (isWinningRow) {
      setHasWon(true);
    }

    setAnimatingRow(row);

    const totalRevealTime = (rowSize - 1) * revealStepMs + revealDurationMs;
    window.setTimeout(() => {
      setRevealedRows((prev) => {
        const next = [...prev];
        next[row] = true;
        return next;
      });
      setAnimatingRow(null);
    }, totalRevealTime);

    if (!isWinningRow && rowStart + rowSize < totalCells) {
      setCurrentIndex(rowStart + rowSize);
    }
  };

  return (
    <div className="main-layout">
      <header className="numble-header">
        <div>
          <p className="numble-kicker">Guess the hidden number</p>
          <h1>NUMBLE</h1>
          <p className="player-greeting">Hello, {username || "Player"}</p>
        </div>
        <button type="button" className="action-button action-secondary" onClick={handleLogout}>
          Logout
        </button>
      </header>

      <section className="numble-panel">
        <div className="game-shell">
          <div>
            <div className="numble-stats">
              <p className="stat-pill">Attempts left: <strong>{attemptsLeft}</strong></p>
              <p className="stat-pill">Digits: <strong>{rowSize}</strong></p>
              <p className="stat-pill">Score: <strong>{score}</strong></p>
            </div>

            <div className="grid">
              {cells.map((_, index) => (
                <div
                  key={index}
                  className={`grid-cell ${colors[index]} ${
                    animatingRow === Math.floor(index / rowSize) ? "reveal" : ""
                  } ${
                    revealedRows[Math.floor(index / rowSize)] ? "revealed" : ""
                  }`}
                  style={{
                    ["--reveal-delay" as string]: `${(index % rowSize) * revealStepMs}ms`,
                  }}
                >
                  {(() => {
                    const row = Math.floor(index / rowSize);
                    const activeRow = Math.floor(currentIndex / rowSize);
                    const isEditable =
                      row === activeRow && !submittedRows[row] && !isGameOver;

                    return (
                      <input
                        value={values[index]}
                        inputMode="numeric"
                        maxLength={1}
                        disabled={!isEditable}
                        autoFocus={index === currentIndex}
                        onFocus={() => setCurrentIndex(index)}
                        onChange={(event) => handleChange(index, event)}
                        onKeyDown={(event) => handleKeyDown(index, event)}
                      />
                    );
                  })()}
                </div>
              ))}
            </div>
          </div>

          <aside className="leaderboard-panel" aria-label="Leaderboard">
            <h2>Leaderboard</h2>
            {leaderboard.length === 0 ? (
              <p className="leaderboard-empty">No scores yet</p>
            ) : (
              <ol className="leaderboard-list">
                {leaderboard.map((player, index) => (
                  <li key={player.username} className="leaderboard-item">
                    <span className="leader-rank">#{index + 1}</span>
                    <span className="leader-name">{player.username}</span>
                    <span className="leader-score">{player.score}</span>
                  </li>
                ))}
              </ol>
            )}
          </aside>
        </div>

        {hasWon ? <p className="game-status status-win">You won!</p> : null}
        {!hasWon && isGameOver ? <p className="game-status status-lose">Game over. Target was {target}.</p> : null}
        {isGameOver ? (
          <button type="button" className="action-button" onClick={resetGame}>
            Play Again
          </button>
        ) : null}
      </section>
    </div>
  );
}
