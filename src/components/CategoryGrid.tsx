import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type RefObject,
} from "react";
import { useI18n } from "../i18n";
import { resolveCoverUrl } from "../lib/backend";
import { genreKey, type GenreSummary } from "../lib/library";
import { platformLabel } from "../lib/platforms";
import { useBrokenCovers } from "../lib/useBrokenCovers";
import type { DetectedGame, GameSource } from "../types/game";
import { FilterChip } from "./FilterChip";

/** What the tiles stand for: the store genres or the Steam community tags. */
export type CategoryKind = "genres" | "tags";

/** A platform the tiles can be counted on, with how many of the games browsed it has. */
export interface TilePlatform {
  source: GameSource;
  count: number;
}

interface CategoryGridProps {
  kind: CategoryKind;
  /** What to show a tile for, in the order of the side panel (or of `platform`'s games). */
  categories: GenreSummary[];
  /** The games the categories were counted on: those of `platform`, when one is chosen. */
  games: DetectedGame[];
  onOpen: (key: string) => void;
  /** The platforms offered, as the side panel lists them. */
  platforms: TilePlatform[];
  /** How many games are browsed on every platform together: the count of "All". */
  allCount: number;
  /** The platform the tiles are counted on; `null` for all of them. */
  platform: GameSource | null;
  onPlatformChange: (platform: GameSource | null) => void;
  /** The theme draws the row of platforms (`categories.platforms`). */
  showPlatforms: boolean;
}

/** Covers a tile's strip is made of at most. */
const COVERS_IN_STRIP = 8;
/** Spare covers kept per tile, for the ones that fail to load. */
const COVERS_KEPT = 12;
/** A strip needs this many covers to fill its tile while it loops: fewer are repeated. */
const MIN_STRIP_LENGTH = 4;
/** How long a cover takes to drift by its own width: slow enough to be looked at. */
const SECONDS_PER_COVER = 8;

/**
 * The covers behind each tile: the most played games of the category first, since those are
 * the ones recognised at a glance, but a cover already used by an earlier tile gives way to
 * one not seen yet, so that "Action" and "Adventure" do not show the same games.
 */
function pickCovers(
  kind: CategoryKind,
  categories: GenreSummary[],
  games: DetectedGame[]
): Map<string, string[]> {
  const withArt = games
    .map((game) => ({ game, url: resolveCoverUrl(game.coverPath, game.updatedAt) }))
    .filter((entry): entry is { game: DetectedGame; url: string } => entry.url !== null)
    .sort(
      (a, b) =>
        (b.game.playtimeMinutes ?? 0) - (a.game.playtimeMinutes ?? 0) ||
        (b.game.lastPlayedAt ?? 0) - (a.game.lastPlayedAt ?? 0) ||
        a.game.title.localeCompare(b.game.title)
    );

  const byCategory = new Map<string, string[]>();
  for (const { game, url } of withArt) {
    const keys = new Set((kind === "genres" ? game.genres : game.tags).map(genreKey));
    for (const key of keys) {
      const urls = byCategory.get(key);
      if (urls) urls.push(url);
      else byCategory.set(key, [url]);
    }
  }

  const timesShown = new Map<string, number>();
  const picked = new Map<string, string[]>();
  for (const category of categories) {
    // The sort is stable: among covers shown equally often, the most played still lead
    const chosen = [...(byCategory.get(category.key) ?? [])]
      .sort((a, b) => (timesShown.get(a) ?? 0) - (timesShown.get(b) ?? 0))
      .slice(0, COVERS_KEPT);
    for (const url of chosen.slice(0, COVERS_IN_STRIP)) {
      timesShown.set(url, (timesShown.get(url) ?? 0) + 1);
    }
    picked.set(category.key, chosen);
  }
  return picked;
}

/** Whether the element is on screen or about to be: tiles far away stay bare. */
function useNearViewport<T extends Element>(): [RefObject<T>, boolean] {
  const ref = useRef<T>(null);
  const [near, setNear] = useState(false);
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    if (typeof IntersectionObserver === "undefined") {
      setNear(true);
      return;
    }
    const observer = new IntersectionObserver(([entry]) => setNear(entry.isIntersecting), {
      rootMargin: "320px 0px",
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return [ref, near];
}

interface CoverTileProps {
  /** What the tile stands for: a genre's key, an emulator's id. */
  id: string;
  name: string;
  /** A second, smaller line under the name. */
  detail?: string;
  /** Covers of the tile's games, the ones to show first. */
  covers: string[];
  label: string;
  countLabel: string;
  onOpen: () => void;
  onBrokenCover: (url: string) => void;
}

/**
 * One tile: a strip of its games' covers drifting slowly across the tile, in the light
 * of the cover that is passing through the middle. The light is the same strip, enlarged
 * until one cover fills the tile and blurred, moving in step with the covers: whatever
 * passes, the tile takes its colors, with nothing computed and nothing to keep in sync.
 */
export function CoverTile({
  id,
  name,
  detail,
  covers,
  label,
  countLabel,
  onOpen,
  onBrokenCover,
}: CoverTileProps) {
  const [ref, near] = useNearViewport<HTMLButtonElement>();

  // The strip holds the sequence twice, so that it can loop without a seam
  const sequence = useMemo(() => {
    if (covers.length === 0) return [];
    const repeats = Math.ceil(MIN_STRIP_LENGTH / covers.length);
    return Array.from({ length: repeats }, () => covers).flat();
  }, [covers]);
  const strip = [...sequence, ...sequence];

  // Tiles start at different points of their loop: a wall moving in step looks mechanical
  const period = sequence.length * SECONDS_PER_COVER;
  let hash = 0;
  for (const char of id) hash = (hash * 31 + (char.codePointAt(0) ?? 0)) >>> 0;
  const motion = {
    "--strip-duration": `${period}s`,
    "--strip-delay": `-${period > 0 ? hash % period : 0}s`,
  } as CSSProperties;

  return (
    <button
      ref={ref}
      type="button"
      data-category={id}
      onClick={onOpen}
      aria-label={label}
      className="game-card category-card"
      style={motion}
    >
      {near && strip.length > 0 && (
        <>
          <span aria-hidden="true" className="category-ambient">
            <span className="category-strip">
              {strip.map((url, index) => (
                <img key={index} src={url} alt="" decoding="async" draggable={false} />
              ))}
            </span>
          </span>
          <span aria-hidden="true" className="category-veil" />
          <span aria-hidden="true" className="category-covers">
            <span className="category-strip">
              {strip.map((url, index) => (
                <img
                  key={index}
                  src={url}
                  alt=""
                  decoding="async"
                  draggable={false}
                  onError={() => onBrokenCover(url)}
                />
              ))}
            </span>
          </span>
        </>
      )}
      <span className="category-card-text">
        <span className="min-w-0">
          <span className="category-card-name">{name}</span>
          {detail && <span className="category-card-detail">{detail}</span>}
        </span>
        <span className="category-card-count">{countLabel}</span>
      </span>
    </button>
  );
}

/**
 * Genres (or tags) as a wall of tiles: each shows the covers of its games drifting by, in
 * their own colors, with the category's name and how many games it holds. A tile opens the
 * library on that genre. Above the tiles, the platforms: one chosen counts the tiles on its
 * games alone, and the library a tile opens keeps it.
 */
export function CategoryGrid({
  kind,
  categories,
  games,
  onOpen,
  platforms,
  allCount,
  platform,
  onPlatformChange,
  showPlatforms,
}: CategoryGridProps) {
  const { t, plural } = useI18n();
  const { usableCover, markBroken } = useBrokenCovers();
  const covers = useMemo(() => pickCovers(kind, categories, games), [kind, categories, games]);
  const platformName = platform === null ? null : platformLabel(platform, t);
  // A choice between one platform and all of them would be no choice: the row needs two. One
  // already chosen stays in sight, so that it can be undone
  const offersRow = showPlatforms && (platforms.length > 1 || platform !== null);
  const pill = (active: boolean) =>
    `inline-flex items-center gap-2 rounded-lg border px-3.5 py-1.5 text-xs font-medium transition-colors ${
      active
        ? "border-accent bg-accent text-zinc-950"
        : "border-oled-border bg-oled-surface/60 text-zinc-300 hover:bg-oled-elevated"
    }`;

  return (
    <section aria-labelledby="category-grid-title" className="space-y-4">
      <header className="flex items-baseline gap-2.5">
        <h1
          id="category-grid-title"
          className="text-lg font-semibold tracking-tight text-zinc-50"
        >
          {t(kind === "genres" ? "overview.genres" : "overview.tags")}
        </h1>
        {categories.length > 0 && (
          <span className="text-xs tabular-nums text-zinc-500">
            {plural("overview.count", categories.length)}
          </span>
        )}
        {/* Without the row, a platform already chosen is still said, and undone in one click */}
        {!showPlatforms && platform !== null && platformName !== null && (
          <span className="self-center">
            <FilterChip
              id="category-platform-chosen"
              label={platformName}
              onRemove={() => onPlatformChange(null)}
            />
          </span>
        )}
      </header>

      {offersRow && (
        <div
          id="category-platforms"
          role="group"
          aria-label={t("sidebar.platform")}
          className="flex flex-wrap gap-1.5"
        >
          <button
            id="category-platform-all"
            type="button"
            aria-pressed={platform === null}
            onClick={() => onPlatformChange(null)}
            className={pill(platform === null)}
          >
            <span>{t("overview.allPlatforms")}</span>
            <span className={`tabular-nums ${platform === null ? "opacity-75" : "text-zinc-500"}`}>
              {allCount}
            </span>
          </button>
          {platforms.map(({ source, count }) => {
            const active = platform === source;
            return (
              <button
                key={source}
                id={`category-platform-${source}`}
                type="button"
                aria-pressed={active}
                // A second click on the chosen one goes back to every platform, as in the panel
                onClick={() => onPlatformChange(active ? null : source)}
                className={pill(active)}
              >
                <span>{platformLabel(source, t)}</span>
                <span className={`tabular-nums ${active ? "opacity-75" : "text-zinc-500"}`}>{count}</span>
              </button>
            );
          })}
        </div>
      )}

      {categories.length === 0 ? (
        <div className="rounded-xl border border-oled-border bg-oled-surface/40 px-6 py-12 text-center">
          <p className="text-sm text-zinc-400">
            {platformName !== null
              ? t(kind === "genres" ? "overview.genresEmptyOn" : "overview.tagsEmptyOn", {
                  platform: platformName,
                })
              : t(kind === "genres" ? "sidebar.genresEmpty" : "overview.tagsEmpty")}
          </p>
        </div>
      ) : (
        <div className="category-grid">
          {categories.map((category) => {
            const countLabel = plural("games", category.count);
            return (
              <CoverTile
                key={category.key}
                id={category.key}
                name={category.label}
                covers={(covers.get(category.key) ?? [])
                  .filter((url) => usableCover(url) !== null)
                  .slice(0, COVERS_IN_STRIP)}
                label={t("overview.open", { label: category.label, games: countLabel })}
                countLabel={countLabel}
                onOpen={() => onOpen(category.key)}
                onBrokenCover={markBroken}
              />
            );
          })}
        </div>
      )}
    </section>
  );
}
