import {
  ChartNoAxesColumn,
  ChevronDown,
  CloudDownload,
  EyeOff,
  HardDrive,
  History,
  Home,
  Joystick,
  LayoutGrid,
  Library,
  Star,
  Tag,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { useI18n } from "../i18n";
import {
  EMPTY_FILTERS,
  GAME_STATUSES,
  hasActiveFilters,
  readUiPreference,
  writeUiPreference,
  type GenreSummary,
  type LibraryCollection,
  type LibraryCounts,
  type LibraryFilters,
} from "../lib/library";
import { PLATFORM_ORDER, platformLabel } from "../lib/platforms";
import type { HideableSection } from "../types/game";
import type { CategoryKind } from "./CategoryGrid";
import { STATUS_LABEL_KEYS, StatusIcon } from "./StatusIcon";

/** The library, or one of the pages beside it; `genres` and `tags` are the walls of tiles. */
export type MainView = "home" | "store" | "library" | "stats" | "emulators" | CategoryKind;

const SECTION_PREF_KEYS: Record<CategoryKind, string> = {
  genres: "kanso.ui.sidebar.genres",
  tags: "kanso.ui.sidebar.tags",
};

interface SidebarProps {
  isOpen: boolean;
  view: MainView;
  onOpenHome: () => void;
  onOpenStore: () => void;
  /** Liked deals about to end: the Store entry wears their number. */
  storeAlerts: number;
  onOpenStats: () => void;
  /** Shows the emulators of this PC. */
  onOpenEmulators: () => void;
  /** How many emulators there are: the entry says so. */
  emulatorCount: number;
  /** Shows the tiles of the genres or of the tags. */
  onOpenOverview: (kind: CategoryKind) => void;
  filters: LibraryFilters;
  /** Changing a filter always shows the library. */
  onFiltersChange: (next: LibraryFilters) => void;
  /** The global switch: leave out everything that is not on disk. */
  installedOnly: boolean;
  onInstalledOnlyChange: (next: boolean) => void;
  counts: LibraryCounts;
  genres: GenreSummary[];
  /** Steam community tags found in the library, most common first. */
  tags: GenreSummary[];
  /** The parts of the panel the user chose to leave out. */
  hidden: readonly HideableSection[];
}

/** Tags shown before "show all": the list can run to a hundred entries. */
const TAGS_SHOWN_AT_FIRST = 10;

interface NavRowProps {
  id: string;
  label: string;
  count?: number;
  /** What the count is, when it is not a number of games: said to who cannot see it. */
  countLabel?: string;
  active: boolean;
  icon?: ReactNode;
  /** One of the pages beside the library: it wears the color the theme gives to those. */
  page?: boolean;
  onClick: () => void;
}

function NavRow({ id, label, count, countLabel, active, icon, page = false, onClick }: NavRowProps) {
  return (
    <button
      id={id}
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={`nav-item flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs ${
        page
          ? `nav-page font-medium ${active ? "bg-oled-elevated" : "hover:bg-oled-surface"}`
          : active
            ? "bg-oled-elevated font-medium text-zinc-100"
            : "text-zinc-400 hover:bg-oled-surface hover:text-zinc-200"
      }`}
    >
      {icon}
      <span className="min-w-0 flex-1 truncate" title={label}>
        {label}
      </span>
      {count !== undefined &&
        (countLabel ? (
          <span
            className="nav-alert shrink-0 rounded-full bg-amber-400 px-1.5 text-2xs font-semibold tabular-nums text-zinc-950"
            title={countLabel}
            aria-label={countLabel}
          >
            {count}
          </span>
        ) : (
          <span
            className={`shrink-0 tabular-nums ${
              page ? "opacity-80" : active ? "text-accent" : "text-zinc-500"
            }`}
          >
            {count}
          </span>
        ))}
    </button>
  );
}

function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="nav-heading px-2.5 pb-1.5 text-2xs font-bold uppercase tracking-[0.14em]">
      {children}
    </h2>
  );
}

interface SectionToggleProps {
  id: string;
  label: string;
  hint: string;
  count: number;
  expanded: boolean;
  /** Its tiles are what the main area shows. */
  active: boolean;
  controls: string;
  onClick: () => void;
}

/**
 * A section title that is also a way in: it opens the tiles of its entries in the main area
 * and unfolds their list below. Pressed again while the tiles are showing, it folds the list.
 */
function SectionToggle({
  id,
  label,
  hint,
  count,
  expanded,
  active,
  controls,
  onClick,
}: SectionToggleProps) {
  return (
    <h2 className="pb-1">
      <button
        id={id}
        type="button"
        onClick={onClick}
        aria-expanded={expanded}
        aria-controls={controls}
        aria-current={active ? "page" : undefined}
        title={hint}
        className={`nav-heading nav-item flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-2xs font-bold uppercase tracking-[0.14em] ${
          active ? "bg-oled-elevated" : "hover:bg-oled-surface"
        }`}
      >
        <LayoutGrid className="h-3 w-3 shrink-0" />
        <span className="min-w-0 flex-1 truncate">{label}</span>
        <span className="shrink-0 font-medium tabular-nums tracking-normal text-zinc-500">{count}</span>
        <ChevronDown
          className={`h-3 w-3 shrink-0 transition-transform duration-200 ${
            expanded ? "" : "-rotate-90"
          }`}
        />
      </button>
    </h2>
  );
}

interface SwitchRowProps {
  id: string;
  label: string;
  hint: string;
  checked: boolean;
  onChange: (next: boolean) => void;
}

function SwitchRow({ id, label, hint, checked, onChange }: SwitchRowProps) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      title={hint}
      onClick={() => onChange(!checked)}
      className={`flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-oled-surface hover:text-zinc-200 ${
        checked ? "text-zinc-100" : "text-zinc-400"
      }`}
    >
      <HardDrive className="h-3.5 w-3.5 shrink-0" />
      <span className="min-w-0 flex-1 truncate">{label}</span>
      <span
        aria-hidden="true"
        className={`relative h-4 w-7 shrink-0 rounded-full transition-colors ${
          checked ? "bg-accent" : "bg-zinc-500"
        }`}
      >
        {/* The knob takes the page's own color: it stands out on a dark and on a light theme */}
        <span
          className={`absolute left-0.5 top-0.5 h-3 w-3 rounded-full transition-transform duration-200 ${
            checked ? "translate-x-3 bg-zinc-950" : "bg-oled-bg"
          }`}
        />
      </span>
    </button>
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
  onOpenEmulators,
  emulatorCount,
  onOpenOverview,
  filters,
  onFiltersChange,
  installedOnly,
  onInstalledOnlyChange,
  counts,
  genres,
  tags,
  hidden,
  onOpenStore,
  storeAlerts,
}: SidebarProps) {
  const { t } = useI18n();
  const [showAllTags, setShowAllTags] = useState(false);
  // The two long lists start folded: their titles open them, together with their tiles
  const [unfolded, setUnfolded] = useState<Record<CategoryKind, boolean>>(() => ({
    genres: readUiPreference(SECTION_PREF_KEYS.genres, ["open", "closed"], "closed") === "open",
    tags: readUiPreference(SECTION_PREF_KEYS.tags, ["open", "closed"], "closed") === "open",
  }));
  const inLibrary = view === "library";

  const setSectionUnfolded = (kind: CategoryKind, open: boolean) => {
    setUnfolded((current) => ({ ...current, [kind]: open }));
    writeUiPreference(SECTION_PREF_KEYS[kind], open ? "open" : "closed");
  };

  /** A section title: shows the tiles and unfolds the list; with the tiles in view, folds it. */
  const browse = (kind: CategoryKind) => {
    if (view === kind) {
      setSectionUnfolded(kind, !unfolded[kind]);
    } else {
      onOpenOverview(kind);
      setSectionUnfolded(kind, true);
    }
  };
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

  // What is selected stays in sight, in a folded list as among the less common tags
  const shownGenres = unfolded.genres
    ? genres
    : genres.filter((genre) => genre.key === filters.genre);
  const shownTags = !unfolded.tags
    ? tags.filter((tag) => tag.key === filters.tag)
    : showAllTags
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
          {!hidden.includes("home") && (
            <NavRow
              id="nav-home"
              page
              label={t("sidebar.home")}
              active={view === "home"}
              icon={<Home className="h-3.5 w-3.5 shrink-0" />}
              onClick={onOpenHome}
            />
          )}
          {!hidden.includes("store") && (
            <NavRow
              id="nav-store"
              page
              label={t("sidebar.store")}
              count={storeAlerts > 0 ? storeAlerts : undefined}
              countLabel={t("store.endingSoon")}
              active={view === "store"}
              icon={<Tag className="h-3.5 w-3.5 shrink-0" />}
              onClick={onOpenStore}
            />
          )}
          <NavRow
            id="nav-stats"
            page
            label={t("sidebar.stats")}
            active={view === "stats"}
            icon={<ChartNoAxesColumn className="h-3.5 w-3.5 shrink-0" />}
            onClick={onOpenStats}
          />
          <NavRow
            id="nav-emulators"
            page
            label={t("sidebar.emulators")}
            count={emulatorCount > 0 ? emulatorCount : undefined}
            active={view === "emulators"}
            icon={<Joystick className="h-3.5 w-3.5 shrink-0" />}
            onClick={onOpenEmulators}
          />
        </div>

        <section>
          <SectionTitle>{t("sidebar.library")}</SectionTitle>
          <div className="space-y-0.5">
            {/* Above the lists it changes; pointless while every game is on disk */}
            {(counts.notInstalled > 0 || installedOnly) && (
              <SwitchRow
                id="installed-only-switch"
                label={t("sidebar.installedOnly")}
                hint={t("sidebar.installedOnlyHint")}
                checked={installedOnly}
                onChange={onInstalledOnlyChange}
              />
            )}
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
          {genres.length === 0 ? (
            <>
              <SectionTitle>{t("sidebar.genre")}</SectionTitle>
              <p className="px-2.5 text-2xs leading-relaxed text-zinc-500">
                {t("sidebar.genresEmpty")}
              </p>
            </>
          ) : (
            <>
              <SectionToggle
                id="browse-genres"
                label={t("sidebar.genre")}
                hint={t("sidebar.browseGenres")}
                count={genres.length}
                expanded={unfolded.genres}
                active={view === "genres"}
                controls="sidebar-genres"
                onClick={() => browse("genres")}
              />
              {/* Keyed by its state: unfolding is a new list, which comes in as such */}
              <div
                id="sidebar-genres"
                key={unfolded.genres ? "open" : "closed"}
                className="motion-list space-y-0.5"
              >
                {shownGenres.map((genre) => {
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
            </>
          )}
        </section>

        {tags.length > 0 && !hidden.includes("tags") && (
          <section>
            <SectionToggle
              id="browse-tags"
              label={t("sidebar.tags")}
              hint={t("sidebar.browseTags")}
              count={tags.length}
              expanded={unfolded.tags}
              active={view === "tags"}
              controls="sidebar-tags"
              onClick={() => browse("tags")}
            />
            <div
              id="sidebar-tags"
              key={unfolded.tags ? "open" : "closed"}
              className="motion-list space-y-0.5"
            >
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
            {unfolded.tags && tags.length > TAGS_SHOWN_AT_FIRST && (
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
