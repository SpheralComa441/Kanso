import {
  ChartNoAxesColumn,
  CloudDownload,
  EyeOff,
  History,
  Home,
  Library,
  Star,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { useI18n } from "../i18n";
import {
  EMPTY_FILTERS,
  GAME_STATUSES,
  hasActiveFilters,
  type GenreSummary,
  type LibraryCollection,
  type LibraryCounts,
  type LibraryFilters,
} from "../lib/library";
import { PLATFORM_ORDER, platformLabel } from "../lib/platforms";
import { STATUS_LABEL_KEYS, StatusIcon } from "./StatusIcon";

export type MainView = "home" | "library" | "stats";

interface SidebarProps {
  isOpen: boolean;
  view: MainView;
  onOpenHome: () => void;
  onOpenStats: () => void;
  filters: LibraryFilters;
  /** Changing a filter always shows the library. */
  onFiltersChange: (next: LibraryFilters) => void;
  counts: LibraryCounts;
  genres: GenreSummary[];
  /** Steam community tags found in the library, most common first. */
  tags: GenreSummary[];
}

/** Tags shown before "show all": the list can run to a hundred entries. */
const TAGS_SHOWN_AT_FIRST = 10;

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
    <h2 className="px-2.5 pb-1.5 text-3xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
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
  onOpenStats,
  filters,
  onFiltersChange,
  counts,
  genres,
  tags,
}: SidebarProps) {
  const { t } = useI18n();
  const [showAllTags, setShowAllTags] = useState(false);
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

  // The selected tag stays in sight even when it is not among the most common ones
  const shownTags = showAllTags
    ? tags
    : tags.filter((tag, index) => index < TAGS_SHOWN_AT_FIRST || tag.key === filters.tag);

  // Statuses are set by hand: the section appears once the user has started using them
  const statusesInUse =
    filters.status !== null || GAME_STATUSES.some((status) => counts.byStatus[status] > 0);

  const platformsWithGames = PLATFORM_ORDER.filter(
    (platform) =>
      counts.byPlatform[platform] > 0 || filters.platform === platform
  );

  return (
    <aside
      id="library-sidebar"
      aria-label={t("sidebar.label")}
      className={`h-full shrink-0 overflow-hidden transition-[width,visibility] duration-200 ease-out ${
        isOpen ? "app-sidebar visible w-[var(--sidebar-width)]" : "invisible w-0"
      }`}
    >
      <nav className="app-sidebar-nav scroll-area fade-top flex h-full flex-col gap-5 overflow-y-auto">
        {/* The two pages that are not the library */}
        <div className="space-y-0.5">
          <NavRow
            id="nav-home"
            label={t("sidebar.home")}
            active={view === "home"}
            icon={<Home className="h-3.5 w-3.5 shrink-0" />}
            onClick={onOpenHome}
          />
          <NavRow
            id="nav-stats"
            label={t("sidebar.stats")}
            active={view === "stats"}
            icon={<ChartNoAxesColumn className="h-3.5 w-3.5 shrink-0" />}
            onClick={onOpenStats}
          />
        </div>

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
            {(counts.notInstalled > 0 || collectionActive("notInstalled")) && (
              <NavRow
                id="filter-not-installed"
                label={t("collection.notInstalled")}
                count={counts.notInstalled}
                active={collectionActive("notInstalled")}
                icon={<CloudDownload className="h-3.5 w-3.5 shrink-0" />}
                onClick={() => toggleCollection("notInstalled")}
              />
            )}
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

        {statusesInUse && (
          <section>
            <SectionTitle>{t("sidebar.status")}</SectionTitle>
            <div className="space-y-0.5">
              {GAME_STATUSES.map((status) => {
                const active = inLibrary && filters.status === status;
                return (
                  <NavRow
                    key={status}
                    id={`filter-status-${status}`}
                    label={t(STATUS_LABEL_KEYS[status])}
                    count={counts.byStatus[status]}
                    active={active}
                    icon={<StatusIcon status={status} className="h-3.5 w-3.5 shrink-0" />}
                    onClick={() =>
                      onFiltersChange(withVisiblePool({ status: active ? null : status }))
                    }
                  />
                );
              })}
            </div>
          </section>
        )}

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

        <section>
          <SectionTitle>{t("sidebar.genre")}</SectionTitle>
          {genres.length === 0 ? (
            <p className="px-2.5 text-2xs leading-relaxed text-zinc-500">
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

        {tags.length > 0 && (
          <section>
            <SectionTitle>{t("sidebar.tags")}</SectionTitle>
            <div className="space-y-0.5">
              {shownTags.map((tag) => {
                const active = inLibrary && filters.tag === tag.key;
                return (
                  <NavRow
                    key={tag.key}
                    id={`filter-tag-${tag.key}`}
                    label={tag.label}
                    count={tag.count}
                    active={active}
                    onClick={() =>
                      onFiltersChange(withVisiblePool({ tag: active ? null : tag.key }))
                    }
                  />
                );
              })}
            </div>
            {tags.length > TAGS_SHOWN_AT_FIRST && (
              <button
                id="toggle-all-tags-button"
                type="button"
                onClick={() => setShowAllTags((all) => !all)}
                aria-expanded={showAllTags}
                className="mt-1 w-full rounded-md px-2.5 py-1.5 text-left text-2xs text-zinc-500 transition-colors hover:bg-oled-surface hover:text-zinc-200"
              >
                {showAllTags
                  ? t("sidebar.showFewerTags")
                  : t("sidebar.showAllTags", { count: tags.length })}
              </button>
            )}
          </section>
        )}
      </nav>
    </aside>
  );
}
