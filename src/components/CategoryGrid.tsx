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
import { useBrokenCovers } from "../lib/useBrokenCovers";
import type { DetectedGame } from "../types/game";

/** What the tiles stand for: the store genres or the Steam community tags. */
export type CategoryKind = "genres" | "tags";

interface CategoryGridProps {
  kind: CategoryKind;
  /** What to show a tile for, in the order of the side panel. */
  categories: GenreSummary[];
  /** The games the categories were counted on. */
  games: DetectedGame[];
  onOpen: (key: string) => void;
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

interface CategoryTileProps {
  category: GenreSummary;
  /** Covers of the category's games, the ones to show first. */
  covers: string[];
  label: string;
  countLabel: string;
  onOpen: () => void;
  onBrokenCover: (url: string) => void;
}

/**
 * One category: a strip of its games' covers drifting slowly across the tile, in the light
 * of the cover that is passing through the middle. The light is the same strip, enlarged
 * until one cover fills the tile and blurred, moving in step with the covers: whatever
 * passes, the tile takes its colors, with nothing computed and nothing to keep in sync.
 */
function CategoryTile({
  category,
  covers,
  label,
  countLabel,
  onOpen,
  onBrokenCover,
}: CategoryTileProps) {
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
  for (const char of category.key) hash = (hash * 31 + (char.codePointAt(0) ?? 0)) >>> 0;
  const motion = {
    "--strip-duration": `${period}s`,
    "--strip-delay": `-${period > 0 ? hash % period : 0}s`,
  } as CSSProperties;

  return (
    <button
      ref={ref}
      type="button"
      data-category={category.key}
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
        <span className="category-card-name">{category.label}</span>
        <span className="category-card-count">{countLabel}</span>
      </span>
    </button>
  );
}

/**
 * Genres (or tags) as a wall of tiles: each shows the covers of its games drifting by, in
 * their own colors, with the category's name and how many games it holds. A tile opens the
 * library on that genre.
 */
export function CategoryGrid({ kind, categories, games, onOpen }: CategoryGridProps) {
  const { t, plural } = useI18n();
  const { usableCover, markBroken } = useBrokenCovers();
  const covers = useMemo(() => pickCovers(kind, categories, games), [kind, categories, games]);

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
      </header>

      {categories.length === 0 ? (
        <div className="rounded-xl border border-oled-border bg-oled-surface/40 px-6 py-12 text-center">
          <p className="text-sm text-zinc-400">
            {t(kind === "genres" ? "sidebar.genresEmpty" : "overview.tagsEmpty")}
          </p>
        </div>
      ) : (
        <div className="category-grid">
          {categories.map((category) => {
            const countLabel = plural("games", category.count);
            return (
              <CategoryTile
                key={category.key}
                category={category}
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
