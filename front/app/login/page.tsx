"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { FormEvent, useMemo, useState } from "react";

type LoginResponse = {
  access_token: string;
  token_type: string;
};

type ErrorResponse = {
  error?: string;
};

export default function LoginPage() {
  const router = useRouter();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const apiBaseUrl = useMemo(
    () => process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3001",
    [],
  );

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    if (!username.trim() || !password.trim()) {
      setErrorMessage("Username and password are required.");
      return;
    }

    setIsSubmitting(true);
    setErrorMessage(null);

    try {
      const response = await fetch(`${apiBaseUrl}/auth/login`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          username,
          password,
        }),
      });

      if (!response.ok) {
        const errorPayload = (await response.json().catch(() => null)) as
          | ErrorResponse
          | null;
        setErrorMessage(errorPayload?.error ?? "Login failed.");
        return;
      }

      const payload = (await response.json()) as LoginResponse;
      localStorage.setItem("numble_token", payload.access_token);
      router.push("/");
    } catch {
      setErrorMessage("Unable to reach backend. Check if it is running.");
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <main className="mx-auto flex min-h-screen w-full max-w-md flex-col justify-center px-6 py-10">
      <div className="rounded-2xl border border-white/20 bg-black/30 p-6 shadow-lg backdrop-blur">
        <h1 className="text-3xl font-semibold tracking-wide text-white">Login</h1>
        <p className="mt-2 text-sm text-zinc-300">
          Sign in to play Numble.
        </p>

        <form className="mt-6 space-y-4" onSubmit={handleSubmit}>
          <div>
            <label htmlFor="username" className="mb-1 block text-sm text-zinc-200">
              Username
            </label>
            <input
              id="username"
              name="username"
              type="text"
              autoComplete="username"
              placeholder="your username"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              className="w-full rounded-lg border border-zinc-500/70 bg-zinc-900/70 px-3 py-2 text-white outline-none transition focus:border-zinc-200"
            />
          </div>

          <div>
            <label htmlFor="password" className="mb-1 block text-sm text-zinc-200">
              Password
            </label>
            <input
              id="password"
              name="password"
              type="password"
              autoComplete="current-password"
              placeholder="••••••••"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              className="w-full rounded-lg border border-zinc-500/70 bg-zinc-900/70 px-3 py-2 text-white outline-none transition focus:border-zinc-200"
            />
          </div>

          {errorMessage ? <p className="text-sm text-red-300">{errorMessage}</p> : null}

          <button
            type="submit"
            disabled={isSubmitting}
            className="w-full rounded-lg border border-white/40 bg-white/10 px-4 py-2 font-medium text-white transition hover:bg-white/20"
          >
            {isSubmitting ? "Signing in..." : "Sign in"}
          </button>
        </form>

        <p className="mt-5 text-xs text-zinc-400">
          Need to go back? <Link href="/" className="underline">Home</Link>
        </p>
      </div>
    </main>
  );
}
