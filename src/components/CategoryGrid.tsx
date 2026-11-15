import { useMemo, type CSSProperties } from "react";
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

/** Covers fanned out under a tile's name. */
const COVERS_SHOWN = 6;
/** Spare covers kept per tile, for the ones that fail to load. */
const COVERS_KEPT = 9;

/** Hues a tile is washed with, far enough apart to be told from one another. */
const TILE_HUES = [4, 28, 46, 140, 170, 196, 218, 250, 285, 330];

/**
 * A hue for each tile. It follows from the category's name, so a category keeps its color
 * from one day to the next; only when that would repeat the color of one of the two tiles
 * before it does it move on to the next hue, so that no row is all of one color.
 */
function pickHues(categories: GenreSummary[]): number[] {
  const hues: number[] = [];
  for (const { key } of categories) {
    let hash = 0;
    for (const char of key) hash = (hash * 31 + (char.codePointAt(0) ?? 0)) >>> 0;
    let hue = TILE_HUES[hash % TILE_HUES.length];
    for (let step = 1; hues.slice(-2).includes(hue) && step < TILE_HUES.length; step++) {
      hue = TILE_HUES[(hash + step) % TILE_HUES.length];
    }
    hues.push(hue);
  }
  return hues;
}

/**
 * The covers behind each tile: the most played games of the category first, since those are
 * the ones recognised at a glance, but a cover already used by an earlier tile gives way to
 * one not seen yet, so that "Action" and "Adventure" do not show the same six games.
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
    for (const url of chosen.slice(0, COVERS_SHOWN)) {
      timesShown.set(url, (timesShown.get(url) ?? 0) + 1);
    }
    picked.set(category.key, chosen);
  }
  return picked;
}

/**
 * Genres (or tags) as a wall of tiles, the way the Steam client shows its collections: a fan
 * of covers under a wash of color, the name and how many games it holds. A tile opens the
 * library on that genre.
 */
export function CategoryGrid({ kind, categories, games, onOpen }: CategoryGridProps) {
  const { t, plural } = useI18n();
  const { usableCover, markBroken } = useBrokenCovers();
  const covers = useMemo(() => pickCovers(kind, categories, games), [kind, categories, games]);
  const hues = useMemo(() => pickHues(categories), [categories]);

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
          {categories.map((category, position) => {
            const usable = (covers.get(category.key) ?? [])
              .filter((url) => usableCover(url) !== null)
              .slice(0, COVERS_SHOWN);
            // A category with a couple of games still fills its tile: the covers repeat
            const cells =
              usable.length === 0
                ? []
                : Array.from({ length: COVERS_SHOWN }, (_, i) => usable[i % usable.length]);

            return (
              <button
                key={category.key}
                type="button"
                data-category={category.key}
                onClick={() => onOpen(category.key)}
                aria-label={t("overview.open", {
                  label: category.label,
                  games: plural("games", category.count),
                })}
                className="game-card category-card"
                style={{ "--category-hue": hues[position] } as CSSProperties}
              >
                {cells.length > 0 && (
                  <span aria-hidden="true" className="category-card-collage">
                    {cells.map((url, index) => (
                      <img
                        key={index}
                        src={url}
                        alt=""
                        loading="lazy"
                        decoding="async"
                        draggable={false}
                        onError={() => markBroken(url)}
                      />
                    ))}
                  </span>
                )}
                <span aria-hidden="true" className="category-card-tint" />
                <span aria-hidden="true" className="category-card-shade" />
                <span className="category-card-text">
                  <span className="category-card-name">
                    {category.label}
                  </span>
                  <span className="category-card-count">{plural("games", category.count)}</span>
                </span>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}
