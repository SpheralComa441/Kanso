import { useSyncExternalStore } from "react";
import { compareDotted, dlssStatus } from "./backend";
import type { DlssLibrary, DlssPart, DlssStatus } from "../types/game";

export const DLSS_PARTS: DlssPart[] = ["superResolution", "frameGeneration", "rayReconstruction"];

/**
 * What is known about DLSS across the library, shared by the places that show it: the
 * game's sheet, the settings, the mark at the top. It is asked for when one of them needs
 * it, not at every start: looking through every game's folder is not free.
 */
let current: DlssStatus | null = null;
const listeners = new Set<() => void>();
const publish = (next: DlssStatus | null) => {
  current = next;
  listeners.forEach((listener) => listener());
};

export function useDlssStatus(): DlssStatus | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => current
  );
}

/** Asks again; `refresh` also looks through the games' folders and asks NVIDIA afresh. */
export async function loadDlssStatus(refresh = false): Promise<DlssStatus> {
  const next = await dlssStatus(refresh);
  publish(next);
  return next;
}

export function setDlssStatus(next: DlssStatus) {
  publish(next);
}

/** After a game's libraries changed: what everyone shows of it follows. */
export function noteDlssChange(game: { id: string; title: string }, libraries: DlssLibrary[]) {
  if (!current) return;
  const games = current.games.filter((entry) => entry.gameId !== game.id);
  if (libraries.length > 0) games.push({ gameId: game.id, title: game.title, libraries });
  games.sort((a, b) => a.title.localeCompare(b.title));
  publish({ ...current, games });
}

/**
 * Whether a library is older than `version` and could be replaced by it. DLSS 1 is a
 * different thing: newer libraries do not stand in for it.
 */
export function isOlder(library: DlssLibrary, version: string | null | undefined): boolean {
  return Boolean(version) && !library.version.startsWith("1.") && compareDotted(library.version, version as string) < 0;
}

/** The parts of a game that NVIDIA's newest version would bring up to date. */
export function outdated(libraries: DlssLibrary[], latest: string | null | undefined): DlssLibrary[] {
  return libraries.filter((library) => isOlder(library, latest));
}

/** How many games have something older than NVIDIA's newest version. */
export function gamesToUpdate(status: DlssStatus | null): number {
  if (!status?.latest) return 0;
  return status.games.filter((game) => outdated(game.libraries, status.latest).length > 0).length;
}

export interface DlssDonor {
  gameId: string;
  title: string;
  /** The newest version it would bring, among the parts it improves. */
  version: string;
}

/** The user's other games that carry a newer copy of a part this game has, best first. */
export function donorsFor(status: DlssStatus | null, gameId: string, libraries: DlssLibrary[]): DlssDonor[] {
  if (!status) return [];
  return status.games
    .filter((other) => other.gameId !== gameId)
    .flatMap((other) => {
      const better = other.libraries
        .filter((offered) => libraries.some((own) => own.part === offered.part && isOlder(own, offered.version)))
        .map((offered) => offered.version)
        .sort(compareDotted);
      return better.length > 0 ? [{ gameId: other.gameId, title: other.title, version: better[better.length - 1] }] : [];
    })
    .sort((a, b) => compareDotted(b.version, a.version) || a.title.localeCompare(b.title));
}
