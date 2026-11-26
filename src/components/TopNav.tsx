import {
  ChartNoAxesColumn,
  Home,
  Joystick,
  Newspaper,
  Tag,
} from "lucide-react";
import type { ReactNode } from "react";
import { useI18n } from "../i18n";
import type { NavPage, ThemeNav } from "../types/game";
import type { MainView } from "./Sidebar";

interface TopNavProps {
  view: MainView;
  /** The pages on offer, in order: the theme's and the user's word. */
  pages: readonly NavPage[];
  /** Theme navigation layout and tab customization settings. */
  nav: ThemeNav;
  onOpenHome: () => void;
  onOpenStore: () => void;
  onOpenNews: () => void;
  /** Liked deals about to end: the Store entry wears their number. */
  storeAlerts: number;
  onOpenStats: () => void;
  onOpenEmulators: () => void;
  /** How many emulators there are: the entry says so. */
  emulatorCount: number;
}

interface NavPageEntry {
  id: string;
  label: string;
  icon: ReactNode;
  active: boolean;
  count?: number;
  countLabel?: string;
  onClick: () => void;
}

/**
 * Navigation tabs for the five main pages (Home, Store, News, Statistics, Emulators).
 * Adapts to any position (`top`, `bottom`, `sidebarTop`, `sidebarBottom`) and reflects
 * every color, graphic, alignment, and animation setting declared by the active theme JSON.
 */
export function TopNav({
  view,
  pages,
  nav,
  onOpenHome,
  onOpenStore,
  onOpenNews,
  storeAlerts,
  onOpenStats,
  onOpenEmulators,
  emulatorCount,
}: TopNavProps) {
  const { t } = useI18n();

  if (pages.length === 0) return null;

  const position = nav.pagesPosition;
  const inSidebar = position === "sidebarTop" || position === "sidebarBottom";
  const showIcons = nav.tabIcons;
  const showLabels = nav.tabLabels || !nav.tabIcons;
  const showCounts = nav.tabCounts;

  const entries: Record<NavPage, NavPageEntry> = {
    home: {
      id: "nav-home",
      label: t("sidebar.home"),
      icon: <Home className="h-3.5 w-3.5 shrink-0" />,
      active: view === "home",
      onClick: onOpenHome,
    },
    store: {
      id: "nav-store",
      label: t("sidebar.store"),
      icon: <Tag className="h-3.5 w-3.5 shrink-0" />,
      active: view === "store",
      count: storeAlerts > 0 ? storeAlerts : undefined,
      countLabel: t("store.endingSoon"),
      onClick: onOpenStore,
    },
    news: {
      id: "nav-news",
      label: t("sidebar.news"),
      icon: <Newspaper className="h-3.5 w-3.5 shrink-0" />,
      active: view === "news",
      onClick: onOpenNews,
    },
    stats: {
      id: "nav-stats",
      label: t("sidebar.stats"),
      icon: <ChartNoAxesColumn className="h-3.5 w-3.5 shrink-0" />,
      active: view === "stats",
      onClick: onOpenStats,
    },
    emulators: {
      id: "nav-emulators",
      label: t("sidebar.emulators"),
      icon: <Joystick className="h-3.5 w-3.5 shrink-0" />,
      active: view === "emulators",
      count: emulatorCount > 0 ? emulatorCount : undefined,
      onClick: onOpenEmulators,
    },
  };

  const buttons = pages.map((page) => {
    const item = entries[page];
    return (
      <button
        key={page}
        id={item.id}
        type="button"
        onClick={item.onClick}
        aria-current={item.active ? "page" : undefined}
        aria-pressed={item.active}
        aria-label={!showLabels ? item.label : undefined}
        title={!showLabels ? item.label : undefined}
        className="top-nav-tab"
      >
        {showIcons && item.icon}
        {showLabels && (
          <span className={inSidebar ? "min-w-0 flex-1 truncate" : undefined}>
            {item.label}
          </span>
        )}
        {showCounts &&
          item.count !== undefined &&
          (item.countLabel ? (
            <span
              className="nav-alert shrink-0 rounded-full bg-amber-400 px-1.5 text-2xs font-semibold tabular-nums text-zinc-950"
              title={item.countLabel}
              aria-label={item.countLabel}
            >
              {item.count}
            </span>
          ) : (
            <span className="top-nav-count shrink-0 tabular-nums">
              {item.count}
            </span>
          ))}
      </button>
    );
  });

  if (inSidebar) {
    return (
      <div
        id="sidebar-pages"
        data-position={position}
        className="app-sidebar-page-list"
      >
        {buttons}
      </div>
    );
  }

  return (
    <nav
      id="top-nav"
      aria-label={t("sidebar.label")}
      data-position={position}
      className="app-top-nav shrink-0"
    >
      <div className="app-top-nav-list">
        <div className="app-top-nav-group">{buttons}</div>
      </div>
    </nav>
  );
}
