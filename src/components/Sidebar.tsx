import {
  ChevronDown,
  CloudDownload,
  EyeOff,
  HardDrive,
  History,
  LayoutGrid,
  Library,
  Star,
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
import { offeredPlatforms, platformLabel } from "../lib/platforms";
import { offeredGroups } from "../lib/themes";
import type {
  GameSource,
  GameStatus,
  HideableSection,
  NavGroup,
  ThemeNav,
} from "../types/game";
import type { CategoryKind } from "./CategoryGrid";
import { STATUS_LABEL_KEYS, StatusIcon } from "./StatusIcon";

/** The library, or one of the pages beside it; `genres` and `tags` are the walls of tiles. */
export type MainView = "home" | "store" | "news" | "library" | "stats" | "emulators" | CategoryKind;

const SECTION_PREF_KEYS: Record<CategoryKind, string> = {
  genres: "kanso.ui.sidebar.genres",
  tags: "kanso.ui.sidebar.tags",
};

interface SidebarProps {
  isOpen: boolean;
  view: MainView;
  /** Shows the tiles of the genres or of the tags. */
  onOpenOverview: (kind: CategoryKind) => void;
  filters: LibraryFilters;
  /** Changing a filter always shows the library. */
  onFiltersChange: (next: LibraryFilters) => void;
  /** With the tiles of genres or tags open, a platform entry counts them on that platform. */
  onTilesPlatformChange: (platform: GameSource | null) => void;
  /** The global switch: leave out everything that is not on disk. */
  installedOnly: boolean;
  onInstalledOnlyChange: (next: boolean) => void;
  counts: LibraryCounts;
  genres: GenreSummary[];
  /** Steam community tags found in the library, most common first. */
  tags: GenreSummary[];
  /** What the active theme puts in the panel: which groups and in which order. */
  nav: ThemeNav;
  /** The parts of the panel the user chose to leave out, whatever the theme says. */
  hidden: readonly HideableSection[];
  /** Rendered page tabs when a theme places them inside the side panel. */
  pagesSlot?: ReactNode;
  pagesPosition?: "sidebarTop" | "sidebarBottom";
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
      className={`nav-item flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs ${
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
 * Collapsible side panel: the groups of the library (its collections, statuses, platforms,
 * genres, tags). The active theme says which groups there are and in which order; the user
 * can hide some in Settings. Library filters combine; clicking an active one switches it off.
 */
export function Sidebar({
  isOpen,
  view,
  onOpenOverview,
  filters,
  onFiltersChange,
  onTilesPlatformChange,
  installedOnly,
  onInstalledOnlyChange,
  counts,
  genres,
  tags,
  nav,
  hidden,
  pagesSlot,
  pagesPosition,
}: SidebarProps) {
  const { t } = useI18n();
  const [showAllTags, setShowAllTags] = useState(false);
  // The two long lists start folded: their titles open them, together with their tiles
  const [unfolded, setUnfolded] = useState<Record<CategoryKind, boolean>>(() => ({
    genres: readUiPreference(SECTION_PREF_KEYS.genres, ["open", "closed"], "closed") === "open",
    tags: readUiPreference(SECTION_PREF_KEYS.tags, ["open", "closed"], "closed") === "open",
  }));
  const inLibrary = view === "library";
  const onTiles = view === "genres" || view === "tags";

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
    const libraryPlatform = filters.platform === "emulator" ? null : filters.platform;
    if (collectionActive(collection)) {
      onFiltersChange({ ...filters, platform: libraryPlatform, collection: "all" });
    } else if (collection === "hidden") {
      onFiltersChange({ ...EMPTY_FILTERS, collection });
    } else {
      onFiltersChange({ ...filters, platform: libraryPlatform, collection });
    }
  };

  /** Platform and genre describe visible games: picking one leaves the hidden and emulator pools. */
  const withVisiblePool = (next: Partial<LibraryFilters>): LibraryFilters => ({
    ...filters,
    collection: viewingHidden ? "all" : filters.collection,
    platform: filters.platform === "emulator" ? null : filters.platform,
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

  // The stores, then the libraries the plugins brought, each under its own name
  const platformsWithGames = offeredPlatforms(counts.byPlatform, filters.platform, t);

  const statusRow = (status: GameStatus) => {
    const active = inLibrary && filters.status === status;
    return (
      <NavRow
        key={status}
        id={`filter-status-${status}`}
        label={t(STATUS_LABEL_KEYS[status])}
        count={counts.byStatus[status]}
        active={active}
        icon={<StatusIcon status={status} className="h-3.5 w-3.5 shrink-0" />}
        onClick={() => onFiltersChange(withVisiblePool({ status: active ? null : status }))}
      />
    );
  };
  const platformRow = (platform: GameSource) => {
    // With the tiles of genres or tags open, a platform counts them: the page stays
    const active = (inLibrary || onTiles) && filters.platform === platform;
    return (
      <NavRow
        key={platform}
        id={`filter-platform-${platform}`}
        label={platformLabel(platform, t)}
        count={counts.byPlatform[platform] ?? 0}
        active={active}
        onClick={() =>
          onTiles
            ? onTilesPlatformChange(active ? null : platform)
            : onFiltersChange(withVisiblePool({ platform: active ? null : platform }))
        }
      />
    );
  };
  const genreRow = (genre: GenreSummary) => {
    const active = inLibrary && filters.genre === genre.key;
    return (
      <NavRow
        key={genre.key}
        id={`filter-genre-${genre.key}`}
        label={genre.label}
        count={genre.count}
        active={active}
        onClick={() => onFiltersChange(withVisiblePool({ genre: active ? null : genre.key }))}
      />
    );
  };
  const tagRow = (tag: GenreSummary) => {
    const active = inLibrary && filters.tag === tag.key;
    return (
      <NavRow
        key={tag.key}
        id={`filter-tag-${tag.key}`}
        label={tag.label}
        count={tag.count}
        active={active}
        onClick={() => onFiltersChange(withVisiblePool({ tag: active ? null : tag.key }))}
      />
    );
  };

  // The groups, each keeping its own conditions (statuses once in use, platforms with games…)
  const groupSections: Record<NavGroup, ReactNode> = {
    library: (
      <section key="library" data-nav-group="library">
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
    ),

    status: statusesInUse ? (
      <section key="status" data-nav-group="status">
        <SectionTitle>{t("sidebar.status")}</SectionTitle>
        <div className="space-y-0.5">{GAME_STATUSES.map(statusRow)}</div>
      </section>
    ) : null,

    platform:
      platformsWithGames.length > 0 ? (
        <section key="platform" data-nav-group="platform">
          <SectionTitle>{t("sidebar.platform")}</SectionTitle>
          <div className="space-y-0.5">{platformsWithGames.map(platformRow)}</div>
        </section>
      ) : null,

    genres: (
      <section key="genres" data-nav-group="genres">
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
              {shownGenres.map(genreRow)}
            </div>
          </>
        )}
      </section>
    ),

    tags:
      tags.length > 0 ? (
        <section key="tags" data-nav-group="tags">
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
            {shownTags.map(tagRow)}
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
      ) : null,
  };

  /*
   * A group the theme leaves out can still be filtering the library: a genre picked on a
   * game's sheet, a filter chosen before the theme changed. What is selected stays in
   * sight here too — the group shows up with its title and that one entry, which switches
   * the filter off — and goes away with it. (The tags the user hid in Settings stay hidden.)
   */
  const leftOut = (group: NavGroup) => !nav.groups.includes(group);
  const activeGenre = genres.find((genre) => genre.key === filters.genre);
  const activeTag = tags.find((tag) => tag.key === filters.tag);
  const selectedOnly: [NavGroup, string, ReactNode][] = [];
  if (leftOut("status") && filters.status !== null) {
    selectedOnly.push(["status", t("sidebar.status"), statusRow(filters.status)]);
  }
  if (leftOut("platform") && filters.platform !== null && filters.platform !== "emulator") {
    selectedOnly.push(["platform", t("sidebar.platform"), platformRow(filters.platform)]);
  }
  if (leftOut("genres") && activeGenre) {
    selectedOnly.push(["genres", t("sidebar.genre"), genreRow(activeGenre)]);
  }
  if (leftOut("tags") && activeTag && !hidden.includes("tags")) {
    selectedOnly.push(["tags", t("sidebar.tags"), tagRow(activeTag)]);
  }

  const groupList = (
    <>
      {offeredGroups(nav, hidden).map((group) => groupSections[group])}
      {selectedOnly.map(([group, title, row]) => (
        <section key={`selected-${group}`} data-nav-group={group} data-selected-only>
          <SectionTitle>{title}</SectionTitle>
          <div className="space-y-0.5">{row}</div>
        </section>
      ))}
    </>
  );

  const pagesAtBottom = Boolean(pagesSlot) && pagesPosition === "sidebarBottom";

  return (
    <aside
      id="library-sidebar"
      aria-label={t("sidebar.label")}
      data-pages={pagesPosition}
      className={`h-full shrink-0 overflow-hidden transition-[width,visibility] duration-200 ease-out ${
        isOpen ? "app-sidebar visible w-[var(--sidebar-width)]" : "invisible w-0"
      }`}
    >
      {pagesAtBottom ? (
        <nav className="flex h-full flex-col">
          <div className="app-sidebar-nav scroll-area fade-ends flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto">
            {groupList}
          </div>
          <div className="app-sidebar-pages scroll-area">{pagesSlot}</div>
        </nav>
      ) : (
        <nav className="app-sidebar-nav scroll-area fade-top flex h-full flex-col gap-5 overflow-y-auto">
          {pagesSlot}
          {groupList}
        </nav>
      )}
    </aside>
  );
}
