"use client";

import { useRouter } from "next/navigation";
import { type ChangeEvent, type KeyboardEvent, useState } from "react";
import { useEffect } from "react";

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
  const attemptsUsed = submittedRows.filter(Boolean).length;
  const attemptsLeft = totalRows - attemptsUsed;

  useEffect(() => {
    const checkAuth = async () => {
      const token = localStorage.getItem("numble_token");
      if (!token) {
        router.replace("/login");
        return;
      }

      const apiBaseUrl = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3001";

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

        setIsAuthorized(true);
      } catch {
        localStorage.removeItem("numble_token");
        router.replace("/login");
      } finally {
        setIsCheckingAuth(false);
      }
    };

    checkAuth();
  }, [router]);

  const isGameOver = hasWon || submittedRows.every(Boolean);

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
        </div>
        <button type="button" className="action-button action-secondary" onClick={handleLogout}>
          Logout
        </button>
      </header>

      <section className="numble-panel">
        <div className="numble-stats">
          <p className="stat-pill">Attempts left: <strong>{attemptsLeft}</strong></p>
          <p className="stat-pill">Digits: <strong>{rowSize}</strong></p>
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
