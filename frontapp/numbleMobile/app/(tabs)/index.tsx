import { useRouter } from 'expo-router';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import * as SecureStore from 'expo-secure-store';

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

type CellColor = 'idle' | 'green' | 'yellow' | 'orange' | 'red';

const rowSize = 6;
const totalCells = 36;
const totalRows = totalCells / rowSize;

function nextTarget() {
  return Math.floor(Math.random() * 1000000).toString().padStart(6, '0');
}

function colorForDiff(diff: number): CellColor {
  if (diff === 0) return 'green';
  if (diff < 3) return 'yellow';
  if (diff < 4) return 'orange';
  return 'red';
}

export default function Home() {
  const router = useRouter();
  const inputRefs = useRef<Array<TextInput | null>>([]);
  const [values, setValues] = useState<string[]>(() => Array(totalCells).fill(''));
  const [colors, setColors] = useState<CellColor[]>(Array(totalCells).fill('idle'));
  const [currentRow, setCurrentRow] = useState(0);
  const [submittedRows, setSubmittedRows] = useState<boolean[]>(Array(totalRows).fill(false));
  const [hasWon, setHasWon] = useState(false);
  const [target, setTarget] = useState(nextTarget);
  const [isAuthorized, setIsAuthorized] = useState(false);
  const [isCheckingAuth, setIsCheckingAuth] = useState(true);
  const [username, setUsername] = useState('');
  const [score, setScore] = useState(0);
  const [leaderboard, setLeaderboard] = useState<LeaderboardEntry[]>([]);
  const [hasRecordedResult, setHasRecordedResult] = useState(false);
  const attemptsUsed = submittedRows.filter(Boolean).length;
  const attemptsLeft = totalRows - attemptsUsed;
  const isGameOver = hasWon || submittedRows.every(Boolean);
  const apiBaseUrl = useMemo(
    () => process.env.EXPO_PUBLIC_API_URL ?? 'http://192.168.1.74:3001',
    [],
  );

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
      const token = await SecureStore.getItemAsync('numble_token');
      if (!token) {
        router.replace('../login');
        setIsCheckingAuth(false);
        return;
      }

      try {
        const response = await fetch(`${apiBaseUrl}/auth/me`, {
          headers: {
            Authorization: `Bearer ${token}`,
          },
        });

        if (!response.ok) {
          await SecureStore.deleteItemAsync('numble_token');
          router.replace('../login');
          return;
        }

        const payload = (await response.json()) as MeResponse;
        setUsername(payload.username);
        setScore(payload.score ?? 0);
        await fetchLeaderboard();
        setIsAuthorized(true);
      } catch {
        await SecureStore.deleteItemAsync('numble_token');
        router.replace('../login');
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

    let isCancelled = false;

    const submitResult = async () => {
      const token = await SecureStore.getItemAsync('numble_token');
      if (!token) {
        return;
      }

      try {
        const response = await fetch(`${apiBaseUrl}/scores/record`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
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

  const resetGame = () => {
    setValues(Array(totalCells).fill(''));
    setColors(Array(totalCells).fill('idle'));
    setCurrentRow(0);
    setSubmittedRows(Array(totalRows).fill(false));
    setHasWon(false);
    setHasRecordedResult(false);
    setTarget(nextTarget());
  };

  const handleLogout = async () => {
    await SecureStore.deleteItemAsync('numble_token');
    router.replace('../login');
  };

  const handleChangeCell = (index: number, rawText: string) => {
    if (isGameOver) {
      return;
    }

    const row = Math.floor(index / rowSize);
    if (row !== currentRow || submittedRows[row]) {
      return;
    }

    const value = rawText.replace(/\D/g, '').slice(-1);
    setValues((prev) => {
      const next = [...prev];
      next[index] = value;
      return next;
    });

    if (value) {
      const rowEnd = row * rowSize + (rowSize - 1);
      if (index < rowEnd) {
        inputRefs.current[index + 1]?.focus();
      }
    }
  };

  const submitCurrentRow = () => {
    if (isGameOver || currentRow >= totalRows) {
      return;
    }

    const rowStart = currentRow * rowSize;
    const rowValues = values.slice(rowStart, rowStart + rowSize);
    if (rowValues.some((value) => value === '')) {
      return;
    }

    const guess = rowValues.join('');
    const isWinningRow = guess === target;

    setColors((prev) => {
      const next = [...prev];
      for (let i = 0; i < rowSize; i++) {
        const cellIndex = rowStart + i;
        const diff = Math.abs(Number(target[i]) - Number(rowValues[i]));
        next[cellIndex] = colorForDiff(diff);
      }
      return next;
    });

    setSubmittedRows((prev) => {
      const next = [...prev];
      next[currentRow] = true;
      return next;
    });

    if (isWinningRow) {
      setHasWon(true);
      return;
    }

    if (currentRow + 1 < totalRows) {
      const nextRow = currentRow + 1;
      setCurrentRow(nextRow);
      inputRefs.current[nextRow * rowSize]?.focus();
    }
  };

  if (isCheckingAuth) {
    return (
      <View style={styles.centeredScreen}>
        <ActivityIndicator size="large" color="#3b82f6" />
        <Text style={styles.loadingText}>Checking authentication...</Text>
      </View>
    );
  }

  if (!isAuthorized) {
    return null;
  }

  return (
    <ScrollView contentContainerStyle={styles.screen}>
      <View style={styles.header}>
        <View>
          <Text style={styles.kicker}>Guess the hidden number</Text>
          <Text style={styles.title}>NUMBLE</Text>
          <Text style={styles.greeting}>Hello, {username || 'Player'}</Text>
        </View>
        <Pressable style={styles.secondaryButton} onPress={handleLogout}>
          <Text style={styles.secondaryButtonText}>Logout</Text>
        </Pressable>
      </View>

      <View style={styles.statsRow}>
        <Text style={styles.statPill}>Attempts: {attemptsLeft}</Text>
        <Text style={styles.statPill}>Digits: {rowSize}</Text>
        <Text style={styles.statPill}>Score: {score}</Text>
      </View>

      <View style={styles.grid}>
        {Array.from({ length: totalCells }).map((_, index) => {
          const row = Math.floor(index / rowSize);
          const editable = row === currentRow && !submittedRows[row] && !isGameOver;
          return (
            <TextInput
              key={index}
              ref={(node) => {
                inputRefs.current[index] = node;
              }}
              value={values[index]}
              onChangeText={(text) => handleChangeCell(index, text)}
              editable={editable}
              maxLength={1}
              keyboardType="number-pad"
              textAlign="center"
              style={[styles.cell, cellColorStyle[colors[index]], !editable && styles.cellLocked]}
              placeholder="-"
              placeholderTextColor="#64748b"
            />
          );
        })}
      </View>

      {!isGameOver ? (
        <Pressable style={styles.primaryButton} onPress={submitCurrentRow}>
          <Text style={styles.primaryButtonText}>Submit Guess</Text>
        </Pressable>
      ) : null}

      {hasWon ? <Text style={styles.statusWin}>You won!</Text> : null}
      {!hasWon && isGameOver ? (
        <Text style={styles.statusLose}>Game over. Target was {target}.</Text>
      ) : null}
      {isGameOver ? (
        <Pressable style={styles.primaryButton} onPress={resetGame}>
          <Text style={styles.primaryButtonText}>Play Again</Text>
        </Pressable>
      ) : null}

      <View style={styles.leaderboardPanel}>
        <Text style={styles.leaderboardTitle}>Leaderboard</Text>
        {leaderboard.length === 0 ? (
          <Text style={styles.leaderboardEmpty}>No scores yet</Text>
        ) : (
          leaderboard.map((player, index) => (
            <View key={player.username} style={styles.leaderRow}>
              <Text style={styles.leaderRank}>#{index + 1}</Text>
              <Text style={styles.leaderName}>{player.username}</Text>
              <Text style={styles.leaderScore}>{player.score}</Text>
            </View>
          ))
        )}
      </View>
    </ScrollView>
  );
}

const cellColorStyle = StyleSheet.create({
  idle: { backgroundColor: '#0b1220' },
  green: { backgroundColor: '#166534' },
  yellow: { backgroundColor: '#a16207' },
  orange: { backgroundColor: '#c2410c' },
  red: { backgroundColor: '#991b1b' },
});

const styles = StyleSheet.create({
  centeredScreen: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#020617',
    gap: 12,
  },
  loadingText: {
    color: '#e2e8f0',
    fontSize: 15,
  },
  screen: {
    paddingHorizontal: 16,
    paddingVertical: 24,
    backgroundColor: '#020617',
    gap: 16,
  },
  header: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    gap: 12,
  },
  kicker: {
    color: '#94a3b8',
    fontSize: 12,
    textTransform: 'uppercase',
    letterSpacing: 1,
  },
  title: {
    color: '#f8fafc',
    fontSize: 28,
    fontWeight: '800',
  },
  greeting: {
    color: '#cbd5e1',
    fontSize: 14,
  },
  secondaryButton: {
    borderWidth: 1,
    borderColor: '#334155',
    borderRadius: 10,
    paddingHorizontal: 14,
    paddingVertical: 10,
    backgroundColor: '#0f172a',
  },
  secondaryButtonText: {
    color: '#e2e8f0',
    fontWeight: '600',
  },
  statsRow: {
    flexDirection: 'row',
    gap: 8,
    flexWrap: 'wrap',
  },
  statPill: {
    color: '#e2e8f0',
    backgroundColor: '#0f172a',
    borderWidth: 1,
    borderColor: '#334155',
    borderRadius: 999,
    paddingHorizontal: 12,
    paddingVertical: 6,
    overflow: 'hidden',
  },
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
    justifyContent: 'center',
  },
  cell: {
    width: 44,
    height: 52,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: '#334155',
    color: '#f8fafc',
    fontSize: 24,
    fontWeight: '700',
  },
  cellLocked: {
    opacity: 0.85,
  },
  primaryButton: {
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 10,
    paddingVertical: 12,
    backgroundColor: '#2563eb',
  },
  primaryButtonText: {
    color: '#ffffff',
    fontSize: 16,
    fontWeight: '700',
  },
  statusWin: {
    color: '#86efac',
    textAlign: 'center',
    fontSize: 16,
    fontWeight: '700',
  },
  statusLose: {
    color: '#fca5a5',
    textAlign: 'center',
    fontSize: 16,
    fontWeight: '700',
  },
  leaderboardPanel: {
    borderWidth: 1,
    borderColor: '#334155',
    borderRadius: 14,
    padding: 14,
    backgroundColor: '#0f172a',
    gap: 8,
  },
  leaderboardTitle: {
    color: '#f8fafc',
    fontSize: 18,
    fontWeight: '700',
  },
  leaderboardEmpty: {
    color: '#94a3b8',
  },
  leaderRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    borderBottomWidth: 1,
    borderBottomColor: '#1e293b',
    paddingBottom: 8,
  },
  leaderRank: {
    color: '#93c5fd',
    width: 36,
    fontWeight: '700',
  },
  leaderName: {
    color: '#e2e8f0',
    flex: 1,
  },
  leaderScore: {
    color: '#f8fafc',
    fontWeight: '700',
  },
});
