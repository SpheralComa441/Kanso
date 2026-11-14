import {
  LayoutGrid,
  List,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  Search,
  Settings,
} from "lucide-react";
import { useI18n } from "../i18n";

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
  /** The grid/list switch only applies to the library. */
  showViewMode: boolean;
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
  showViewMode,
  isScanning,
  isSyncingMetadata,
  hasApiKey,
  onScan,
  onOpenSettings,
}: HeaderProps) {
  const { t, plural } = useI18n();
  const SidebarIcon = isSidebarOpen ? PanelLeftClose : PanelLeftOpen;
  const sidebarLabel = isSidebarOpen
    ? t("header.hideFilters")
    : t("header.showFilters");

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
            title={sidebarLabel}
            aria-label={sidebarLabel}
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
              {plural("games", totalGames)}
            </span>
          </div>
        </div>

        {/* Instant search */}
        <div className="relative mx-auto w-full min-w-0 max-w-xl flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-500" />
          <input
            id="library-search-input"
            type="search"
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={t("header.searchPlaceholder")}
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
              {t("header.syncing")}
            </span>
          )}

          {showViewMode && (
            <div
              role="group"
              aria-label={t("header.viewMode")}
              className="flex items-center rounded-lg border border-oled-border bg-oled-surface p-0.5"
            >
              {(
                [
                  ["grid", LayoutGrid, t("header.gridView")],
                  ["list", List, t("header.listView")],
                ] as const
              ).map(([mode, Icon, label]) => (
                <button
                  key={mode}
                  id={`view-mode-${mode}`}
                  type="button"
                  onClick={() => onViewModeChange(mode)}
                  title={label}
                  aria-label={label}
                  aria-pressed={viewMode === mode}
                  className={`rounded-md p-1.5 transition-colors ${
                    viewMode === mode
                      ? "bg-oled-elevated text-zinc-100"
                      : "text-zinc-400 hover:text-zinc-200"
                  }`}
                >
                  <Icon className="h-3.5 w-3.5" />
                </button>
              ))}
            </div>
          )}

          <button
            id="scan-library-button"
            type="button"
            onClick={onScan}
            disabled={isScanning}
            title={t("header.scanTitle")}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-surface px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-700 hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-60"
          >
            <RefreshCw
              className={`h-3.5 w-3.5 ${
                isScanning ? "animate-spin text-accent" : "text-zinc-400"
              }`}
            />
            <span>{isScanning ? t("header.scanning") : t("header.scan")}</span>
          </button>

          <button
            id="open-settings-button"
            type="button"
            onClick={onOpenSettings}
            title={t("header.settings")}
            aria-label={t("header.settings")}
            className="relative rounded-lg border border-oled-border bg-oled-surface p-2 text-zinc-400 transition-colors hover:border-zinc-700 hover:bg-oled-elevated hover:text-zinc-100"
          >
            <Settings className="h-3.5 w-3.5" />
            {!hasApiKey && (
              <span
                title={t("header.apiKeyMissing")}
                className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full border-2 border-oled-bg bg-accent"
              />
            )}
          </button>
        </div>
      </div>
    </header>
  );
}
