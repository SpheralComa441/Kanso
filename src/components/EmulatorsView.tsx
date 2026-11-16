import { useMemo } from "react";
import { ArrowLeft, FolderOpen, Play, ScanSearch, SlidersHorizontal } from "lucide-react";
import { useI18n } from "../i18n";
import { resolveCoverUrl } from "../lib/backend";
import { useBrokenCovers } from "../lib/useBrokenCovers";
import type { BadgeEntry, CardInfoItem, DetectedGame, EmulatorInfo } from "../types/game";
import { CoverTile } from "./CategoryGrid";
import { GameList } from "./GameList";
import type { ViewMode } from "./Header";

interface EmulatorsViewProps {
  emulators: EmulatorInfo[];
  /** Every emulated game of the library, whatever its emulator. */
  games: DetectedGame[];
  /** The emulator whose page is open; `null` for the wall of emulators. */
  openId: string | null;
  onOpen: (id: string | null) => void;
  /** Opens the emulator itself, with no game. */
  onLaunchEmulator: (emulator: EmulatorInfo) => void;
  onOpenSettings: () => void;
  /** Scans again, searching the disks for emulators. */
  onSearch: () => void;
  /** Narrows the games of the open emulator. */
  searchQuery: string;

  // What the list of games needs, as in the library
  viewMode: ViewMode;
  isScanning: boolean;
  launchingId: string | null;
  onOpenDetail: (game: DetectedGame) => void;
  onLaunch: (game: DetectedGame) => void;
  onToggleFavorite: (game: DetectedGame) => void;
  badges: Record<string, BadgeEntry[]>;
  runningIds: ReadonlySet<string>;
  cardInfo: readonly CardInfoItem[];
}

/** Covers shown on an emulator's tile. */
const COVERS_PER_TILE = 8;

/**
 * The emulators of this PC and the games each one runs.
 *
 * First a wall of tiles, one per emulator, made of its games' covers like the tiles of the
 * genres. A tile opens the emulator's page: what it emulates, where its games are read from,
 * a button that opens the emulator itself, and its games, which behave as library games do
 * (sheet, favourite, status, play time) and start in that emulator.
 */
export function EmulatorsView({
  emulators,
  games,
  openId,
  onOpen,
  onLaunchEmulator,
  onOpenSettings,
  onSearch,
  searchQuery,
  ...list
}: EmulatorsViewProps) {
  const { t, plural } = useI18n();
  const { usableCover, markBroken } = useBrokenCovers();

  const gamesOf = useMemo(() => {
    const byEmulator = new Map<string, DetectedGame[]>();
    for (const game of games) {
      if (game.isHidden) continue;
      const id = game.platformAppId ?? "";
      const own = byEmulator.get(id);
      if (own) own.push(game);
      else byEmulator.set(id, [game]);
    }
    return byEmulator;
  }, [games]);

  // The most played first: those are the covers recognised at a glance
  const coversOf = (id: string): string[] =>
    [...(gamesOf.get(id) ?? [])]
      .sort(
        (a, b) =>
          (b.playtimeMinutes ?? 0) - (a.playtimeMinutes ?? 0) ||
          (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0)
      )
      .map((game) => usableCover(resolveCoverUrl(game.coverPath, game.updatedAt)))
      .filter((url): url is string => url !== null)
      .slice(0, COVERS_PER_TILE);

  const open = emulators.find((emulator) => emulator.id === openId) ?? null;

  if (open) {
    const own = gamesOf.get(open.id) ?? [];
    const needle = searchQuery.trim().toLowerCase();
    const shown = needle
      ? own.filter(
          (game) =>
            game.title.toLowerCase().includes(needle) ||
            game.genres.some((genre) => genre.toLowerCase().includes(needle))
        )
      : own;

    return (
      <section aria-labelledby="emulator-title" className="space-y-4" data-emulator={open.id}>
        <button
          id="back-to-emulators"
          type="button"
          onClick={() => onOpen(null)}
          className="inline-flex items-center gap-1.5 rounded-full py-0.5 pr-1 text-2xs font-medium text-zinc-400 transition-colors hover:text-zinc-100"
        >
          <ArrowLeft className="h-3 w-3" />
          <span>{t("emulators.back")}</span>
        </button>

        <header className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3">
          <div className="min-w-0 space-y-1">
            <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
              <h1 id="emulator-title" className="text-lg font-semibold tracking-tight text-zinc-50">
                {open.name}
              </h1>
              {open.systems && <span className="text-xs text-zinc-400">{open.systems}</span>}
              <span className="text-xs tabular-nums text-zinc-500">{plural("games", own.length)}</span>
            </div>
            {open.folders.length > 0 && (
              <ul id="emulator-folders" className="space-y-0.5 text-2xs text-zinc-500">
                {open.folders.map((folder) => (
                  <li key={folder.path} className="flex items-center gap-1.5">
                    <FolderOpen className="h-3 w-3 shrink-0" aria-hidden="true" />
                    <span className="select-text truncate font-mono" title={folder.path}>
                      {folder.path}
                    </span>
                    {folder.fromEmulator && (
                      <span className="shrink-0 text-zinc-600">· {t("emulators.fromEmulator")}</span>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <button
              id="manage-emulators-button"
              type="button"
              onClick={onOpenSettings}
              className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
            >
              <SlidersHorizontal className="h-3.5 w-3.5" />
              <span>{t("emulators.manage")}</span>
            </button>
            <button
              id="launch-emulator-button"
              type="button"
              onClick={() => onLaunchEmulator(open)}
              className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3.5 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover"
            >
              <Play className="h-3.5 w-3.5 fill-current" />
              <span>{t("emulators.open", { name: open.name })}</span>
            </button>
          </div>
        </header>

        {shown.length === 0 ? (
          <div
            id="emulator-empty"
            className="rounded-xl border border-oled-border bg-oled-surface/40 px-6 py-12 text-center"
          >
            <p className="mx-auto max-w-xl text-sm leading-relaxed text-zinc-400">
              {t(own.length === 0 ? "emulators.noGames" : "emulators.noMatch", { name: open.name })}
            </p>
          </div>
        ) : (
          <GameList
            {...list}
            games={shown}
            totalDetected={own.length}
            warnings={[]}
            onOpenSettings={onOpenSettings}
          />
        )}
      </section>
    );
  }

  return (
    <section aria-labelledby="emulators-title" className="space-y-4">
      <header className="flex flex-wrap items-baseline justify-between gap-3">
        <div className="flex items-baseline gap-2.5">
          <h1 id="emulators-title" className="text-lg font-semibold tracking-tight text-zinc-50">
            {t("emulators.title")}
          </h1>
          {emulators.length > 0 && (
            <span className="text-xs tabular-nums text-zinc-500">
              {plural("emulators.count", emulators.length)}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            id="search-emulators-button"
            type="button"
            onClick={onSearch}
            disabled={list.isScanning}
            title={t("emulators.searchHint")}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated disabled:opacity-60"
          >
            <ScanSearch className={`h-3.5 w-3.5 ${list.isScanning ? "animate-pulse" : ""}`} />
            <span>{t(list.isScanning ? "emulators.searching" : "emulators.search")}</span>
          </button>
          <button
            id="manage-emulators-button"
            type="button"
            onClick={onOpenSettings}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
          >
            <SlidersHorizontal className="h-3.5 w-3.5" />
            <span>{t("emulators.manage")}</span>
          </button>
        </div>
      </header>

      {emulators.length === 0 ? (
        <div
          id="emulators-empty"
          className="rounded-xl border border-oled-border bg-oled-surface/40 px-6 py-12 text-center"
        >
          <p className="mx-auto max-w-xl text-sm leading-relaxed text-zinc-400">{t("emulators.empty")}</p>
        </div>
      ) : (
        <div className="category-grid">
          {emulators.map((emulator) => {
            const countLabel = plural("games", gamesOf.get(emulator.id)?.length ?? 0);
            return (
              <CoverTile
                key={emulator.id}
                id={emulator.id}
                name={emulator.name}
                detail={emulator.systems}
                covers={coversOf(emulator.id)}
                label={t("overview.open", { label: emulator.name, games: countLabel })}
                countLabel={countLabel}
                onOpen={() => onOpen(emulator.id)}
                onBrokenCover={markBroken}
              />
            );
          })}
        </div>
      )}
    </section>
  );
}
