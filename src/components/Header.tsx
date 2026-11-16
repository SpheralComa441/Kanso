import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  LayoutGrid,
  List,
  PanelLeftClose,
  PanelLeftOpen,
  Puzzle,
  RefreshCw,
  Search,
  Settings,
} from "lucide-react";
import { useI18n } from "../i18n";
import { KansoMark } from "./KansoMark";
import {
  isRunningInTauri,
  watchWindowMaximized,
  windowAction,
  type WindowAction,
} from "../lib/backend";

export type ViewMode = "grid" | "list";

/** A command an enabled plugin adds to the Plugins menu. */
export interface PluginCommandEntry {
  pluginId: string;
  commandId: string;
  title: string;
  description: string;
}

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
  /** How many dialogs are open: the bar is dimmed like the app behind them. */
  overlayDepth: number;
  /** The theme editor previews its own draft: Settings would fight over it. */
  settingsDisabled: boolean;
  onScan: () => void;
  onOpenSettings: () => void;
  /** Commands of the enabled plugins; the menu is hidden when there are none. */
  pluginCommands: PluginCommandEntry[];
  onRunPluginCommand: (command: PluginCommandEntry) => void;
}

interface PluginMenuProps {
  commands: PluginCommandEntry[];
  onRun: (command: PluginCommandEntry) => void;
}

/** The Plugins button and the list of commands it opens. */
function PluginMenu({ commands, onRun }: PluginMenuProps) {
  const { t } = useI18n();
  const [isOpen, setIsOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const closeOnOutside = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setIsOpen(false);
    };
    const closeOnEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") setIsOpen(false);
    };
    window.addEventListener("pointerdown", closeOnOutside);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("pointerdown", closeOnOutside);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [isOpen]);

  return (
    <div ref={rootRef} className="relative">
      <button
        id="open-plugin-menu-button"
        type="button"
        onClick={() => setIsOpen((open) => !open)}
        aria-haspopup="menu"
        aria-expanded={isOpen}
        title={t("plugins.menu")}
        aria-label={t("plugins.menu")}
        className={`rounded-lg border border-oled-border bg-oled-surface p-2 transition-colors hover:border-zinc-700 hover:bg-oled-elevated hover:text-zinc-100 ${
          isOpen ? "text-zinc-100" : "text-zinc-400"
        }`}
      >
        <Puzzle className="h-3.5 w-3.5" />
      </button>

      {isOpen && (
        <div
          id="plugin-menu"
          role="menu"
          aria-label={t("plugins.menu")}
          className="absolute right-0 top-full z-30 mt-2 w-72 rounded-xl border border-oled-border bg-oled-surface p-1.5 shadow-2xl"
        >
          {commands.map((command) => (
            <button
              key={`${command.pluginId}:${command.commandId}`}
              id={`plugin-command-${command.pluginId}-${command.commandId}`}
              type="button"
              role="menuitem"
              onClick={() => {
                setIsOpen(false);
                onRun(command);
              }}
              className="block w-full rounded-lg px-3 py-2 text-left transition-colors hover:bg-oled-elevated"
            >
              <span className="block text-xs font-medium text-zinc-100">{command.title}</span>
              {command.description && (
                <span className="mt-0.5 block text-2xs text-zinc-500">{command.description}</span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** Window glyphs on a 10-unit grid, drawn with hairlines like the system ones. */
function Glyph({ children }: { children: ReactNode }) {
  return (
    <svg
      viewBox="0 0 10 10"
      fill="none"
      stroke="currentColor"
      strokeWidth="1"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

/**
 * Minimize / maximize / close. Kanso draws its own title bar (the window has no system
 * frame), so these buttons drive the native window. They stay usable while a dialog is open.
 */
function WindowControls() {
  const { t } = useI18n();
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let active = true;
    let stopWatching: (() => void) | null = null;
    void watchWindowMaximized((value) => {
      if (active) setIsMaximized(value);
    }).then((stop) => {
      if (active) stopWatching = stop;
      else stop();
    });
    return () => {
      active = false;
      stopWatching?.();
    };
  }, []);

  if (!isRunningInTauri()) return null;

  const buttons: [WindowAction, string, string, ReactNode][] = [
    ["minimize", "minimize", t("window.minimize"), <path d="M1 5.5h8" />],
    [
      "toggleMaximize",
      "maximize",
      isMaximized ? t("window.restore") : t("window.maximize"),
      isMaximized ? (
        <path d="M3.5 3.5v-2h5v5h-2M1.5 3.5h5v5h-5z" />
      ) : (
        <path d="M1.5 1.5h7v7h-7z" />
      ),
    ],
    ["close", "close", t("window.close"), <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" />],
  ];

  return (
    <div className="window-controls">
      {buttons.map(([action, name, label, glyph]) => (
        <button
          key={action}
          id={`window-${name}-button`}
          type="button"
          data-action={name}
          title={label}
          aria-label={label}
          // Like system caption buttons, they are not part of the tab order
          tabIndex={-1}
          onClick={() => void windowAction(action).catch(() => {})}
          className="window-control"
        >
          <Glyph>{glyph}</Glyph>
        </button>
      ))}
    </div>
  );
}

/**
 * The top bar: it is also the window's title bar, so its empty areas drag the window and a
 * double click maximizes it (`data-tauri-drag-region="deep"`; buttons and fields are
 * excluded automatically).
 */
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
  overlayDepth,
  settingsDisabled,
  onScan,
  onOpenSettings,
  pluginCommands,
  onRunPluginCommand,
}: HeaderProps) {
  const { t, plural } = useI18n();
  const contentRef = useRef<HTMLDivElement>(null);
  const SidebarIcon = isSidebarOpen ? PanelLeftClose : PanelLeftOpen;
  const sidebarLabel = isSidebarOpen
    ? t("header.hideFilters")
    : t("header.showFilters");

  // Behind a dialog the bar's own controls are out of reach for the keyboard too
  useEffect(() => {
    if (contentRef.current) contentRef.current.inert = overlayDepth > 0;
  }, [overlayDepth]);

  return (
    <header
      data-tauri-drag-region="deep"
      data-veiled={overlayDepth > 0 ? "" : undefined}
      className={`app-header relative flex shrink-0 items-stretch ${
        isRunningInTauri() ? "" : "pr-4"
      }`}
    >
      {/* The right padding keeps the last action clear of the window buttons' hover area */}
      <div
        ref={contentRef}
        className={`flex min-w-0 flex-1 items-center gap-3 pl-4 ${
          isRunningInTauri() ? "pr-3" : ""
        }`}
      >
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
            <KansoMark className="h-6 w-6 shrink-0 text-zinc-100" />
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
              className="hidden items-center gap-1.5 text-2xs text-zinc-400 lg:inline-flex"
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

          {pluginCommands.length > 0 && (
            <PluginMenu commands={pluginCommands} onRun={onRunPluginCommand} />
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
            disabled={settingsDisabled}
            title={t("header.settings")}
            aria-label={t("header.settings")}
            className="relative rounded-lg border border-oled-border bg-oled-surface p-2 text-zinc-400 transition-colors hover:border-zinc-700 hover:bg-oled-elevated hover:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-50"
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

      {/* One veil per open dialog, matching their backdrops; it still drags the window */}
      {Array.from({ length: overlayDepth }, (_, index) => (
        <div
          key={index}
          aria-hidden="true"
          className="absolute inset-0 bg-black/75 backdrop-blur-sm"
        />
      ))}

      <WindowControls />
    </header>
  );
}
