import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import Home from "./page";

const replaceMock = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({
    replace: replaceMock,
  }),
}));

describe("Home page", () => {
  beforeEach(() => {
    localStorage.clear();
    replaceMock.mockReset();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("redirects to login when token is missing", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    render(<Home />);

    await waitFor(() => {
      expect(replaceMock).toHaveBeenCalledWith("/login");
    });
  });

  it("shows player greeting, score and leaderboard", async () => {
    localStorage.setItem("numble_token", "token-123");

    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.endsWith("/auth/me")) {
        return {
          ok: true,
          json: async () => ({
            username: "alice",
            score: 12,
            wins: 1,
            losses: 1,
            games_played: 2,
          }),
        } as Response;
      }

      if (url.endsWith("/scores/leaderboard")) {
        return {
          ok: true,
          json: async () => ({
            players: [
              {
                username: "alice",
                score: 12,
                wins: 1,
                losses: 1,
                games_played: 2,
              },
              {
                username: "bob",
                score: 10,
                wins: 1,
                losses: 0,
                games_played: 1,
              },
            ],
          }),
        } as Response;
      }

      return {
        ok: false,
        json: async () => ({}),
      } as Response;
    });

    vi.stubGlobal("fetch", fetchMock);

    render(<Home />);

    expect(await screen.findByText(/Hello, alice/i)).toBeInTheDocument();
    expect(screen.getByText("Leaderboard")).toBeInTheDocument();
    expect(screen.getByText("bob")).toBeInTheDocument();
    expect(screen.getByText(/Score:/i)).toBeInTheDocument();

    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/auth/me"),
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: "Bearer token-123",
        }),
      }),
    );
  });
});
