import { EyeOff, History, Home, Library, Star } from "lucide-react";
import type { ReactNode } from "react";
import { useI18n } from "../i18n";
import {
  EMPTY_FILTERS,
  hasActiveFilters,
  type GenreSummary,
  type LibraryCollection,
  type LibraryCounts,
  type LibraryFilters,
} from "../lib/library";
import { PLATFORM_ORDER, platformLabel } from "../lib/platforms";

export type MainView = "home" | "library";

interface SidebarProps {
  isOpen: boolean;
  view: MainView;
  onOpenHome: () => void;
  filters: LibraryFilters;
  /** Changing a filter always shows the library. */
  onFiltersChange: (next: LibraryFilters) => void;
  counts: LibraryCounts;
  genres: GenreSummary[];
}

interface NavRowProps {
  id: string;
  label: string;
  count?: number;
  active: boolean;
  icon?: ReactNode;
  onClick: () => void;
}

function NavRow({ id, label, count, active, icon, onClick }: NavRowProps) {
  return (
    <button
      id={id}
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={`flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors ${
        active
          ? "bg-oled-elevated font-medium text-zinc-100"
          : "text-zinc-400 hover:bg-oled-surface hover:text-zinc-200"
      }`}
    >
      {icon}
      <span className="min-w-0 flex-1 truncate" title={label}>
        {label}
      </span>
      {count !== undefined && (
        <span
          className={`shrink-0 tabular-nums ${
            active ? "text-accent" : "text-zinc-500"
          }`}
        >
          {count}
        </span>
      )}
    </button>
  );
}

function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="px-2.5 pb-1.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-zinc-500">
      {children}
    </h2>
  );
}

/**
 * Collapsible side panel: the Home (deals) entry, then the library with its collections
 * (favorites, recently played, hidden), platforms and the genres found in the metadata.
 * Library filters combine; clicking an active one switches it off.
 */
export function Sidebar({
  isOpen,
  view,
  onOpenHome,
  filters,
  onFiltersChange,
  counts,
  genres,
}: SidebarProps) {
  const { t } = useI18n();
  const inLibrary = view === "library";
  const nothingFiltered = !hasActiveFilters(filters);
  const viewingHidden = filters.collection === "hidden";
  const collectionActive = (collection: LibraryCollection) =>
    inLibrary && filters.collection === collection;

  /** Toggles a collection. Hidden games form a separate pool, so entering it drops the rest. */
  const toggleCollection = (collection: LibraryCollection) => {
    if (collectionActive(collection)) {
      onFiltersChange({ ...filters, collection: "all" });
    } else if (collection === "hidden") {
      onFiltersChange({ ...EMPTY_FILTERS, collection });
    } else {
      onFiltersChange({ ...filters, collection });
    }
  };

  /** Platform and genre describe visible games: picking one leaves the hidden pool. */
  const withVisiblePool = (next: Partial<LibraryFilters>): LibraryFilters => ({
    ...filters,
    collection: viewingHidden ? "all" : filters.collection,
    ...next,
  });

  const platformsWithGames = PLATFORM_ORDER.filter(
    (platform) =>
      counts.byPlatform[platform] > 0 || filters.platform === platform
  );

  return (
    <aside
      id="library-sidebar"
      aria-label={t("sidebar.label")}
      className={`h-full shrink-0 overflow-hidden border-oled-border transition-[width,visibility] duration-200 ease-out ${
        isOpen ? "visible w-[var(--sidebar-width)] border-r" : "invisible w-0"
      }`}
    >
      <nav className="flex h-full w-[var(--sidebar-width)] flex-col gap-5 overflow-y-auto px-2.5 py-4">
        <NavRow
          id="nav-home"
          label={t("sidebar.home")}
          active={view === "home"}
          icon={<Home className="h-3.5 w-3.5 shrink-0" />}
          onClick={onOpenHome}
        />

        <section>
          <SectionTitle>{t("sidebar.library")}</SectionTitle>
          <div className="space-y-0.5">
            <NavRow
              id="filter-all"
              label={t("collection.all")}
              count={counts.visible}
              active={inLibrary && nothingFiltered}
              icon={<Library className="h-3.5 w-3.5 shrink-0" />}
              onClick={() => onFiltersChange(EMPTY_FILTERS)}
            />
            <NavRow
              id="filter-favorites"
              label={t("collection.favorites")}
              count={counts.favorites}
              active={collectionActive("favorites")}
              icon={
                <Star
                  className={`h-3.5 w-3.5 shrink-0 ${
                    collectionActive("favorites") ? "fill-accent text-accent" : ""
                  }`}
                />
              }
              onClick={() => toggleCollection("favorites")}
            />
            <NavRow
              id="filter-recent"
              label={t("collection.recent")}
              count={counts.recent}
              active={collectionActive("recent")}
              icon={<History className="h-3.5 w-3.5 shrink-0" />}
              onClick={() => toggleCollection("recent")}
            />
            {(counts.hidden > 0 || viewingHidden) && (
              <NavRow
                id="filter-hidden"
                label={t("collection.hidden")}
                count={counts.hidden}
                active={collectionActive("hidden")}
                icon={<EyeOff className="h-3.5 w-3.5 shrink-0" />}
                onClick={() => toggleCollection("hidden")}
              />
            )}
          </div>
        </section>

        {platformsWithGames.length > 0 && (
          <section>
            <SectionTitle>{t("sidebar.platform")}</SectionTitle>
            <div className="space-y-0.5">
              {platformsWithGames.map((platform) => {
                const active = inLibrary && filters.platform === platform;
                return (
                  <NavRow
                    key={platform}
                    id={`filter-platform-${platform}`}
                    label={platformLabel(platform, t)}
                    count={counts.byPlatform[platform]}
                    active={active}
                    onClick={() =>
                      onFiltersChange(
                        withVisiblePool({ platform: active ? null : platform })
                      )
                    }
                  />
                );
              })}
            </div>
          </section>
        )}

        <section className="min-h-0">
          <SectionTitle>{t("sidebar.genre")}</SectionTitle>
          {genres.length === 0 ? (
            <p className="px-2.5 text-[11px] leading-relaxed text-zinc-500">
              {t("sidebar.genresEmpty")}
            </p>
          ) : (
            <div className="space-y-0.5">
              {genres.map((genre) => {
                const active = inLibrary && filters.genre === genre.key;
                return (
                  <NavRow
                    key={genre.key}
                    id={`filter-genre-${genre.key}`}
                    label={genre.label}
                    count={genre.count}
                    active={active}
                    onClick={() =>
                      onFiltersChange(
                        withVisiblePool({ genre: active ? null : genre.key })
                      )
                    }
                  />
                );
              })}
            </div>
          )}
        </section>
      </nav>
    </aside>
  );
}
