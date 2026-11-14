import { LayoutGrid, List, RefreshCw, Search, Settings } from "lucide-react";
import type { GamePlatform } from "../types/game";

export type PlatformFilter = "all" | GamePlatform;
export type ViewMode = "grid" | "list";

interface HeaderProps {
  totalGames: number;
  searchQuery: string;
  onSearchChange: (value: string) => void;
  platformFilter: PlatformFilter;
  onPlatformFilterChange: (filter: PlatformFilter) => void;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  isScanning: boolean;
  isSyncingMetadata: boolean;
  hasApiKey: boolean;
  onScan: () => void;
  onOpenSettings: () => void;
}

const FILTER_OPTIONS: { value: PlatformFilter; label: string }[] = [
  { value: "all", label: "Tutti" },
  { value: "steam", label: "Steam" },
  { value: "epic", label: "Epic" },
  { value: "Battle.net", label: "Battle.net" },
  { value: "standalone", label: "Locale" },
];

export function Header({
  totalGames,
  searchQuery,
  onSearchChange,
  platformFilter,
  onPlatformFilterChange,
  viewMode,
  onViewModeChange,
  isScanning,
  isSyncingMetadata,
  hasApiKey,
  onScan,
  onOpenSettings,
}: HeaderProps) {
  return (
    <header className="sticky top-0 z-20 border-b border-oled-border bg-oled-bg/95 backdrop-blur-md">
      <div className="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 px-6 py-3.5">
        {/* Brand & count */}
        <div className="flex items-center gap-3">
          <div
            aria-hidden="true"
            className="flex h-7 w-7 items-center justify-center rounded-lg border border-oled-border bg-oled-surface"
          >
            <span className="h-3.5 w-3.5 rounded-full border-2 border-accent border-r-transparent" />
          </div>
          <div className="flex items-baseline gap-2.5">
            <h1 className="text-base font-semibold tracking-tight text-zinc-100">
              Kanso
            </h1>
            <span className="rounded-md border border-oled-border bg-oled-surface px-2 py-0.5 text-xs font-medium tabular-nums text-zinc-400">
              {totalGames} {totalGames === 1 ? "gioco" : "giochi"}
            </span>
            {isSyncingMetadata && (
              <span className="hidden md:inline-flex items-center gap-1.5 text-[11px] text-zinc-400">
                <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-accent" />
                Sincronizzazione metadata...
              </span>
            )}
          </div>
        </div>

        {/* Instant search + quick platform filters */}
        <div className="flex flex-1 items-center justify-center gap-3 min-w-[260px] max-w-2xl">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-500" />
            <input
              id="library-search-input"
              type="search"
              value={searchQuery}
              onChange={(e) => onSearchChange(e.target.value)}
              placeholder="Cerca nella libreria..."
              className="w-full rounded-lg border border-oled-border bg-oled-surface py-1.5 pl-9 pr-3 text-sm text-zinc-100 placeholder-zinc-500 transition-colors focus:border-accent/60 focus:outline-none"
            />
          </div>

          <nav
            aria-label="Filtro piattaforma"
            className="hidden lg:flex items-center rounded-lg border border-oled-border bg-oled-surface p-0.5"
          >
            {FILTER_OPTIONS.map((opt) => {
              const active = platformFilter === opt.value;
              return (
                <button
                  key={opt.value}
                  id={`filter-${opt.value}`}
                  type="button"
                  onClick={() => onPlatformFilterChange(opt.value)}
                  className={`rounded-md px-2.5 py-1 text-xs font-medium transition-colors ${
                    active
                      ? "bg-oled-elevated text-zinc-100"
                      : "text-zinc-400 hover:text-zinc-200"
                  }`}
                >
                  {opt.label}
                </button>
              );
            })}
          </nav>
        </div>

        {/* Actions */}
        <div className="flex items-center gap-2">
          <div
            role="group"
            aria-label="Modalità vista"
            className="flex items-center rounded-lg border border-oled-border bg-oled-surface p-0.5"
          >
            <button
              id="view-mode-grid"
              type="button"
              onClick={() => onViewModeChange("grid")}
              title="Vista griglia copertine"
              className={`rounded-md p-1.5 transition-colors ${
                viewMode === "grid"
                  ? "bg-oled-elevated text-zinc-100"
                  : "text-zinc-400 hover:text-zinc-200"
              }`}
            >
              <LayoutGrid className="h-3.5 w-3.5" />
            </button>
            <button
              id="view-mode-list"
              type="button"
              onClick={() => onViewModeChange("list")}
              title="Vista elenco dettagliato"
              className={`rounded-md p-1.5 transition-colors ${
                viewMode === "list"
                  ? "bg-oled-elevated text-zinc-100"
                  : "text-zinc-400 hover:text-zinc-200"
              }`}
            >
              <List className="h-3.5 w-3.5" />
            </button>
          </div>

          <button
            id="scan-library-button"
            type="button"
            onClick={onScan}
            disabled={isScanning}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-surface px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-700 hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-60"
          >
            <RefreshCw
              className={`h-3.5 w-3.5 text-zinc-400 ${
                isScanning ? "animate-spin text-accent" : ""
              }`}
            />
            <span>{isScanning ? "Scansione..." : "Scansiona"}</span>
          </button>

          <button
            id="open-settings-button"
            type="button"
            onClick={onOpenSettings}
            className="relative inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-surface px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-700 hover:bg-oled-elevated"
          >
            <Settings className="h-3.5 w-3.5 text-zinc-400" />
            <span>Impostazioni</span>
            {!hasApiKey && (
              <span
                title="SteamGridDB API Key non configurata"
                className="h-1.5 w-1.5 rounded-full bg-accent"
              />
            )}
          </button>
        </div>
      </div>
    </header>
  );
}
