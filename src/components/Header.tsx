import {
  LayoutGrid,
  List,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  Search,
  Settings,
} from "lucide-react";

export type ViewMode = "grid" | "list";

interface HeaderProps {
  totalGames: number;
  searchQuery: string;
  onSearchChange: (value: string) => void;
  isSidebarOpen: boolean;
  onToggleSidebar: () => void;
  activeFilterCount: number;
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  isScanning: boolean;
  isSyncingMetadata: boolean;
  hasApiKey: boolean;
  onScan: () => void;
  onOpenSettings: () => void;
}

export function Header({
  totalGames,
  searchQuery,
  onSearchChange,
  isSidebarOpen,
  onToggleSidebar,
  activeFilterCount,
  viewMode,
  onViewModeChange,
  isScanning,
  isSyncingMetadata,
  hasApiKey,
  onScan,
  onOpenSettings,
}: HeaderProps) {
  const SidebarIcon = isSidebarOpen ? PanelLeftClose : PanelLeftOpen;

  return (
    <header className="shrink-0 border-b border-oled-border bg-oled-bg">
      <div className="flex h-[var(--header-height)] items-center gap-3 px-4">
        {/* Sidebar toggle, brand & count */}
        <div className="flex shrink-0 items-center gap-3">
          <button
            id="toggle-sidebar-button"
            type="button"
            onClick={onToggleSidebar}
            aria-expanded={isSidebarOpen}
            aria-controls="library-sidebar"
            title={isSidebarOpen ? "Nascondi filtri" : "Mostra filtri"}
            aria-label={isSidebarOpen ? "Nascondi filtri" : "Mostra filtri"}
            className="relative rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-100"
          >
            <SidebarIcon className="h-4 w-4" />
            {!isSidebarOpen && activeFilterCount > 0 && (
              <span className="absolute right-0.5 top-0.5 h-1.5 w-1.5 rounded-full bg-accent" />
            )}
          </button>

          <div className="flex items-center gap-2.5">
            <span
              aria-hidden="true"
              className="h-3.5 w-3.5 rounded-full border-2 border-accent border-r-transparent"
            />
            <h1 className="text-base font-semibold tracking-tight text-zinc-100">
              Kanso
            </h1>
            <span className="rounded-md border border-oled-border bg-oled-surface px-2 py-0.5 text-xs font-medium tabular-nums text-zinc-400">
              {totalGames} {totalGames === 1 ? "gioco" : "giochi"}
            </span>
          </div>
        </div>

        {/* Instant search */}
        <div className="relative mx-auto w-full max-w-xl min-w-0 flex-1">
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

        {/* Actions */}
        <div className="flex shrink-0 items-center gap-2">
          {isSyncingMetadata && (
            <span
              role="status"
              className="hidden items-center gap-1.5 text-[11px] text-zinc-400 lg:inline-flex"
            >
              <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-accent" />
              Sincronizzazione metadata...
            </span>
          )}

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
              aria-label="Vista griglia copertine"
              aria-pressed={viewMode === "grid"}
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
              aria-label="Vista elenco dettagliato"
              aria-pressed={viewMode === "list"}
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
            title="Scansiona di nuovo i giochi installati"
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-surface px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-700 hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-60"
          >
            <RefreshCw
              className={`h-3.5 w-3.5 ${
                isScanning ? "animate-spin text-accent" : "text-zinc-400"
              }`}
            />
            <span>{isScanning ? "Scansione..." : "Scansiona"}</span>
          </button>

          <button
            id="open-settings-button"
            type="button"
            onClick={onOpenSettings}
            title="Impostazioni"
            aria-label="Impostazioni"
            className="relative rounded-lg border border-oled-border bg-oled-surface p-2 text-zinc-400 transition-colors hover:border-zinc-700 hover:bg-oled-elevated hover:text-zinc-100"
          >
            <Settings className="h-3.5 w-3.5" />
            {!hasApiKey && (
              <span
                title="SteamGridDB API Key non configurata"
                className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full border-2 border-oled-bg bg-accent"
              />
            )}
          </button>
        </div>
      </div>
    </header>
  );
}
