import { Fragment, createContext, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ChevronLeft, ChevronRight, Eye, Play, Plus, RefreshCw, SlidersHorizontal, X } from "lucide-react";
import { useI18n } from "../i18n";
import type { Ambience } from "../lib/ambience";
import { MAX_UP_NEXT, resolveCoverUrl } from "../lib/backend";
import { isRecentlyPlayed } from "../lib/library";
import { platformLabel } from "../lib/platforms";
import { useBrokenCovers } from "../lib/useBrokenCovers";
import type { HomePluginPartData } from "../lib/useHomePlugins";
import type { BadgeEntry, CardInfoItem, DetectedGame, HomeConfig, HomeSection, ThemeHome } from "../types/game";
import { CardInfoPanel, PluginBadges, useCardFacts, type CardFact } from "./CardInfo";
import { Cover } from "./Cover";
import { PluginFailure } from "./GameExtras";
import { HomeAmbience } from "./HomeAmbience";
import { PluginBlocks } from "./PluginBlocks";
import { PluginPicks, resolvePicks, usePickReveal } from "./PluginPicks";

interface HomeViewProps {
  /** Every game that is not hidden, the emulated ones included. */
  games: DetectedGame[];
  home: HomeConfig;
  /** Which parts the active theme shows, and in which order. */
  theme: ThemeHome;
  /** Changes with the theme: the background reads its colors again. */
  themeKey: string;
  /** What moves behind the top of the page; `null` when nothing does. */
  ambience: Ambience | null;
  /** The place whose weather the background follows, when it does. */
  weatherPlace: string | null;
  /** Opens the site the weather comes from. */
  onOpenWeatherSource: () => void;
  onChangeHome: (next: HomeConfig) => void;
  onOpenGame: (game: DetectedGame) => void;
  onLaunch: (game: DetectedGame) => void;
  launchingId: string | null;
  onOpenSettings: () => void;
  /** The discounts of the stores, drawn by whoever owns them. */
  /** The best discounts of the moment, and the suggestions by genre: both from the Store. */
  deals: ReactNode;
  recommendations: ReactNode;
  /** What the plugins add to the page, in their order; drawn where the theme puts "plugins". */
  pluginParts: HomePluginPartData[];
  /** The list of plugins was read once: until then nobody knows whether one has a part for Home. */
  pluginsListed: boolean;
  /** Short labels plugins have for the games, by game id. */
  badges: Record<string, BadgeEntry[]>;
  /** What a cover tells under the pointer besides the title, as the user chose. */
  cardInfo: readonly CardInfoItem[];
}

/** The narrowest a cover on a shelf gets, in rem. */
const SHELF_CARD_REM = 9.5;
/** Most games on a shelf, however wide the window. */
const SHELF_MAX = 10;

/** How many covers fit side by side: a shelf is one row, never a cut second one. */
function useShelfColumns(): [React.RefObject<HTMLDivElement>, number] {
  const ref = useRef<HTMLDivElement>(null);
  const [columns, setColumns] = useState(6);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () => {
      const style = getComputedStyle(element);
      const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
      const gap = parseFloat(style.columnGap) || 0;
      setColumns(Math.max(2, Math.min(SHELF_MAX, Math.floor((element.clientWidth + gap) / (SHELF_CARD_REM * rem + gap)))));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return [ref, columns];
}

interface ShelfCardProps {
  game: DetectedGame;
  /** A line under the title: when it was played, for how long. */
  note: string;
  /** A number over the cover: a place in a ranking, or in a queue. */
  rank?: number;
  isLaunching: boolean;
  onOpen: (game: DetectedGame) => void;
  onLaunch: (game: DetectedGame) => void;
  /** Controls shown over the cover, top right (the queue's own). */
  controls?: ReactNode;
}

/** What the covers of the page's shelves tell under the pointer, as the user chose for the library. */
interface CardTelling {
  cardInfo: readonly CardInfoItem[];
  badges: Record<string, BadgeEntry[]>;
  factsOf: (game: DetectedGame) => CardFact[];
}

const CardTellingContext = createContext<CardTelling>({ cardInfo: [], badges: {}, factsOf: () => [] });

function ShelfCard({ game, note, rank, isLaunching, onOpen, onLaunch, controls }: ShelfCardProps) {
  const { t } = useI18n();
  const { cardInfo, badges, factsOf } = useContext(CardTellingContext);
  const { usableCover, markBroken } = useBrokenCovers();
  const coverUrl = usableCover(resolveCoverUrl(game.coverPath, game.updatedAt));
  const canStart = game.isInstalled && Boolean(game.launchUri || game.executablePath);

  return (
    <article className="game-card home-card group" data-home-game={game.id}>
      <div className="game-card-cover relative w-full overflow-hidden bg-oled-bg">
        {coverUrl ? (
          <div className={`h-full w-full ${game.isInstalled ? "" : "opacity-45 grayscale"}`}>
            <Cover url={coverUrl} zoomOnHover onError={() => markBroken(coverUrl)} />
          </div>
        ) : (
          <div className="game-card-blank flex h-full w-full flex-col justify-end bg-gradient-to-b from-oled-elevated/50 to-oled-bg p-3">
            <p className="line-clamp-4 text-xs font-semibold leading-snug text-zinc-300">{game.title}</p>
          </div>
        )}

        {rank !== undefined && (
          <span className="home-rank absolute left-2 top-2 flex h-6 min-w-6 items-center justify-center rounded-md border border-white/15 bg-black/75 px-1.5 text-xs font-semibold tabular-nums text-white">
            {rank}
          </span>
        )}

        {/* The whole cover opens the game's sheet */}
        <button
          type="button"
          onClick={() => onOpen(game)}
          aria-label={t("game.openDetails", { title: game.title })}
          className="absolute inset-0 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
        />

        {controls && (
          <div className="absolute right-1.5 top-1.5 flex items-center gap-0.5 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
            {controls}
          </div>
        )}

        {/* Under the pointer: the button that starts the game and, in themes whose cards tell
            about the game over the blurred cover, the same lines as in the library. A game
            that cannot be started has only those lines to show. */}
        <div
          data-info-only={canStart ? undefined : ""}
          className="game-card-hover pointer-events-none absolute inset-x-0 bottom-0 flex items-end bg-gradient-to-t from-black/85 to-transparent p-2 opacity-0 transition-opacity duration-200 focus-within:opacity-100 group-hover:opacity-100"
        >
          {/* The title stands under the cover already */}
          <CardInfoPanel game={game} cardInfo={cardInfo} facts={factsOf(game)} showTitle={false} />
          {canStart && (
            <button
              type="button"
              onClick={() => onLaunch(game)}
              disabled={isLaunching}
              aria-label={t("game.launchNamed", { title: game.title })}
              className="pointer-events-auto inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-accent py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:opacity-60"
            >
              <Play className="h-3.5 w-3.5 fill-current" />
              <span>{isLaunching ? t("common.launching") : t("common.play")}</span>
            </button>
          )}
        </div>
      </div>
      <div className="space-y-0.5 px-2.5 py-2">
        <h3 className="truncate text-xs font-medium text-zinc-100" title={game.title}>
          {game.title}
        </h3>
        <div className="flex items-center justify-between gap-2 text-2xs text-zinc-500">
          <p className="min-w-0 truncate">{note}</p>
          {/* Where covers tell nothing under the pointer, the plugins' labels stand here */}
          <span className="home-card-badges shrink-0">
            <PluginBadges entries={badges[game.id]} />
          </span>
        </div>
      </div>
    </article>
  );
}

function Shelf({ children }: { children: (columns: number) => ReactNode }) {
  const [ref, columns] = useShelfColumns();
  return (
    <div ref={ref} className="home-shelf" style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
      {children(columns)}
    </div>
  );
}

interface SectionHeaderProps {
  id: string;
  title: string;
  note?: string;
  /** Small controls of the section, after its title. */
  actions?: ReactNode;
}

function SectionHeader({ id, title, note, actions }: SectionHeaderProps) {
  return (
    <header className="flex items-baseline gap-2.5">
      <h2 id={id} className="home-heading text-sm font-semibold">
        {title}
      </h2>
      {note && <span className="truncate text-2xs text-zinc-500">{note}</span>}
      {actions && <span className="flex shrink-0 items-center gap-1 self-center">{actions}</span>}
    </header>
  );
}

/** Whether a plugin's part has something to draw: words, games of the library, or a failure to report. */
function partHasContent(part: HomePluginPartData, games: DetectedGame[]): boolean {
  const { answer } = part;
  if (!answer) return false;
  const view = answer.data;
  return (view?.sections.length ?? 0) > 0 || resolvePicks(view, games).length > 0 || Boolean(answer.error);
}

interface HomePluginPartProps {
  part: HomePluginPartData;
  games: DetectedGame[];
  onOpenGame: (game: DetectedGame) => void;
  onLaunch: (game: DetectedGame) => void;
}

/**
 * The part of Home a plugin adds: a heading like the other parts' (the plugin's own title, or
 * its name), its words drawn with Kanso's blocks, and the games it points at laid out as it
 * asked. A plugin that failed with nothing older to show gets one line, with "Try again".
 */
function HomePluginPart({ part, games, onOpenGame, onLaunch }: HomePluginPartProps) {
  const { t } = useI18n();
  const { plugin, answer, isLoading, refresh } = part;
  const view = answer?.data ?? null;
  const { deal, revealed, reveal } = usePickReveal(view);
  // Covers as wide as those of the shelves above and below
  const [picksRef, columns] = useShelfColumns();
  if (!answer) return null;

  const sections = view?.sections ?? [];
  const picks = resolvePicks(view, games);
  const hasView = sections.length > 0 || picks.length > 0;
  const covered = view?.gamesStyle === "mystery" ? picks.filter(({ game }) => !revealed.has(game.id)) : [];
  const title = view?.title ?? plugin.name;
  const headingId = `home-plugin-${plugin.id}`;
  const control = "rounded p-1 text-zinc-500 transition-colors hover:text-zinc-100";

  return (
    <section
      aria-labelledby={hasView ? headingId : undefined}
      aria-label={hasView ? undefined : plugin.name}
      data-home-section={`plugin:${plugin.id}`}
      className="space-y-2.5"
    >
      {hasView && (
        <SectionHeader
          id={headingId}
          title={title}
          // A part with a title of its own still says whose it is
          note={title === plugin.name ? undefined : plugin.name}
          actions={
            <>
              {covered.length > 0 && (
                <button
                  type="button"
                  data-reveal-all={plugin.id}
                  onClick={() => reveal(covered.map(({ game }) => game.id))}
                  title={t("plugins.revealAll")}
                  aria-label={t("plugins.revealAll")}
                  className={control}
                >
                  <Eye className="h-3 w-3" />
                </button>
              )}
              <button
                type="button"
                data-refresh-plugin={plugin.id}
                // Busy, not disabled: a disabled button would drop the keyboard focus it has
                onClick={isLoading ? undefined : refresh}
                aria-disabled={isLoading}
                title={t("plugins.refresh", { name: plugin.name })}
                aria-label={t("plugins.refresh", { name: plugin.name })}
                className={control}
              >
                <RefreshCw className={`h-3 w-3 ${isLoading ? "animate-spin" : ""}`} />
              </button>
            </>
          }
        />
      )}

      {sections.length > 0 && (
        <div className="home-plugin-words space-y-3.5">
          {sections.map((section, index) => (
            <div key={index} className="space-y-2">
              {section.title && <h3 className="micro-label text-zinc-500">{section.title}</h3>}
              <PluginBlocks blocks={section.blocks} />
            </div>
          ))}
        </div>
      )}

      <div ref={picksRef} className="home-picks empty:hidden">
        <PluginPicks
          picks={picks}
          style={view?.gamesStyle ?? "list"}
          idPrefix={`home-pick-${plugin.id}`}
          deal={deal}
          revealed={revealed}
          onReveal={reveal}
          columns={columns}
          onOpenGame={onOpenGame}
          onLaunch={onLaunch}
        />
      </div>

      {answer.error && (
        <div className={isLoading ? "opacity-60" : undefined}>
          <PluginFailure
            pluginId={plugin.id}
            name={plugin.name}
            error={answer.error}
            logs={answer.logs}
            staleSince={hasView ? answer.fetchedAt : null}
            onRetry={refresh}
          />
        </div>
      )}
    </section>
  );
}

/**
 * The Home page: the user's own games first. What is being played, to pick up again; the
 * games lined up to play next, which the user chooses (five at most); the ones played the
 * longest; the stores' discounts; and what the plugins the user switched on add. The active
 * theme says which parts there are and in which order; the user can leave any of them out in
 * Settings. Behind the top of the page
 * something of the season moves, unless the theme, the user or the animations say no.
 */
export function HomeView({
  games,
  home,
  theme,
  themeKey,
  ambience,
  weatherPlace,
  onOpenWeatherSource,
  onChangeHome,
  onOpenGame,
  onLaunch,
  launchingId,
  onOpenSettings,
  deals,
  recommendations,
  pluginParts,
  pluginsListed,
  badges,
  cardInfo,
}: HomeViewProps) {
  const { t, relativePast, duration } = useI18n();
  const factsOf = useCardFacts(cardInfo, badges);
  const telling: CardTelling = { cardInfo, badges, factsOf };
  const [picking, setPicking] = useState(false);
  const [query, setQuery] = useState("");
  const shown = theme.sections.filter((section) => !home.hidden.includes(section));

  const byId = useMemo(() => new Map(games.map((game) => [game.id, game])), [games]);
  // A game that left the library leaves the queue with it, quietly
  const queue = home.upNext.flatMap((id) => byId.get(id) ?? []);
  const setQueue = (next: DetectedGame[]) => onChangeHome({ ...home, upNext: next.map((game) => game.id) });

  const resume = useMemo(
    () =>
      games
        .filter((game) => game.status === "playing" || isRecentlyPlayed(game))
        // What is marked as being played first, then by the last session
        .sort(
          (a, b) =>
            Number(b.status === "playing") - Number(a.status === "playing") ||
            (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0)
        ),
    [games]
  );
  const mostPlayed = useMemo(
    () =>
      games
        .filter((game) => (game.playtimeMinutes ?? 0) > 0)
        .sort((a, b) => (b.playtimeMinutes ?? 0) - (a.playtimeMinutes ?? 0)),
    [games]
  );
  // What can be lined up: not already in the queue, the ones marked "to play" first, then
  // what was never started
  const candidates = useMemo(() => {
    const queued = new Set(home.upNext);
    const needle = query.trim().toLowerCase();
    return games
      .filter((game) => !queued.has(game.id) && game.status !== "finished" && game.status !== "abandoned")
      .filter((game) => !needle || game.title.toLowerCase().includes(needle))
      .sort(
        (a, b) =>
          Number(b.status === "backlog") - Number(a.status === "backlog") ||
          (a.playtimeMinutes ?? 0) - (b.playtimeMinutes ?? 0) ||
          a.title.localeCompare(b.title)
      );
  }, [games, home.upNext, query]);

  useEffect(() => {
    if (queue.length >= MAX_UP_NEXT) setPicking(false);
  }, [queue.length]);

  const playedNote = (game: DetectedGame) =>
    game.lastPlayedAt !== null ? relativePast(game.lastPlayedAt) : t("detail.neverPlayed");
  const timeNote = (game: DetectedGame) => duration(Math.max(1, game.playtimeMinutes ?? 0));
  const control =
    "rounded-md border border-white/15 bg-black/75 p-1 text-white/85 transition-colors hover:bg-black hover:text-white disabled:opacity-30";
  const drawnParts = pluginParts.filter((part) => partHasContent(part, games));

  const sections: Record<HomeSection, ReactNode> = {
    resume:
      resume.length > 0 ? (
        <section key="resume" aria-labelledby="home-resume" data-home-section="resume" className="space-y-2.5">
          <SectionHeader id="home-resume" title={t("home.resume")} note={t("home.resumeNote")} />
          <Shelf>
            {(columns) =>
              resume.slice(0, columns).map((game) => (
                <ShelfCard
                  key={game.id}
                  game={game}
                  note={playedNote(game)}
                  isLaunching={launchingId === game.id}
                  onOpen={onOpenGame}
                  onLaunch={onLaunch}
                />
              ))
            }
          </Shelf>
        </section>
      ) : null,

    upNext: (
      <section key="upNext" aria-labelledby="home-up-next" data-home-section="upNext" className="space-y-2.5">
        <SectionHeader
          id="home-up-next"
          title={t("home.upNext")}
          note={t("home.upNextNote", { count: queue.length, max: MAX_UP_NEXT })}
        />
        <Shelf>
          {() => (
            <>
              {queue.map((game, index) => (
                <ShelfCard
                  key={game.id}
                  game={game}
                  rank={index + 1}
                  note={game.playtimeMinutes ? timeNote(game) : platformLabel(game.platform, t)}
                  isLaunching={launchingId === game.id}
                  onOpen={onOpenGame}
                  onLaunch={onLaunch}
                  controls={
                    <>
                      <button
                        type="button"
                        onClick={() => setQueue([...queue.slice(0, index - 1), game, queue[index - 1], ...queue.slice(index + 1)])}
                        disabled={index === 0}
                        aria-label={t("home.moveEarlier", { title: game.title })}
                        title={t("home.moveEarlier", { title: game.title })}
                        className={control}
                      >
                        <ChevronLeft className="h-3 w-3" />
                      </button>
                      <button
                        type="button"
                        onClick={() => setQueue([...queue.slice(0, index), queue[index + 1], game, ...queue.slice(index + 2)])}
                        disabled={index === queue.length - 1}
                        aria-label={t("home.moveLater", { title: game.title })}
                        title={t("home.moveLater", { title: game.title })}
                        className={control}
                      >
                        <ChevronRight className="h-3 w-3" />
                      </button>
                      <button
                        type="button"
                        data-remove-next={game.id}
                        onClick={() => setQueue(queue.filter((entry) => entry.id !== game.id))}
                        aria-label={t("home.removeNext", { title: game.title })}
                        title={t("home.removeNext", { title: game.title })}
                        className={control}
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </>
                  }
                />
              ))}
              {queue.length < MAX_UP_NEXT && (
                <button
                  id="home-add-next"
                  type="button"
                  onClick={() => setPicking(!picking)}
                  aria-expanded={picking}
                  aria-controls="home-next-picker"
                  className="home-add-tile flex flex-col items-center justify-center gap-2 text-xs text-zinc-500 transition-colors hover:text-accent"
                >
                  <Plus className="h-5 w-5" />
                  <span>{t(queue.length === 0 ? "home.addFirst" : "home.addNext")}</span>
                </button>
              )}
            </>
          )}
        </Shelf>

        {picking && (
          <div id="home-next-picker" className="motion-list space-y-2 rounded-xl border border-oled-border bg-oled-surface/60 p-3">
            <input
              id="home-next-search"
              autoFocus
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.stopPropagation();
                  setPicking(false);
                  setQuery("");
                }
              }}
              placeholder={t("home.pickPlaceholder")}
              aria-label={t("home.pickPlaceholder")}
              className="w-full max-w-sm rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-accent/60 focus:outline-none"
            />
            {candidates.length === 0 ? (
              <p className="text-2xs text-zinc-500">{t("home.pickNone")}</p>
            ) : (
              <ul className="flex flex-wrap gap-1.5">
                {candidates.slice(0, 18).map((game) => (
                  <li key={game.id}>
                    <button
                      type="button"
                      data-pick-next={game.id}
                      onClick={() => {
                        setQueue([...queue, game]);
                        setQuery("");
                      }}
                      title={t("home.pickNamed", { title: game.title })}
                      className={`inline-flex max-w-[16rem] items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-colors hover:border-accent/60 hover:text-accent ${
                        game.status === "backlog"
                          ? "border-accent/40 bg-accent/10 text-zinc-100"
                          : "border-oled-border bg-oled-bg text-zinc-300"
                      }`}
                    >
                      <Plus className="h-3 w-3 shrink-0" />
                      <span className="truncate">{game.title}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <p className="text-2xs text-zinc-500">{t("home.pickHelp")}</p>
          </div>
        )}
      </section>
    ),

    mostPlayed:
      mostPlayed.length > 0 ? (
        <section key="mostPlayed" aria-labelledby="home-most-played" data-home-section="mostPlayed" className="space-y-2.5">
          <SectionHeader id="home-most-played" title={t("home.mostPlayed")} />
          <Shelf>
            {(columns) =>
              mostPlayed.slice(0, columns).map((game, index) => (
                <ShelfCard
                  key={game.id}
                  game={game}
                  rank={index + 1}
                  note={timeNote(game)}
                  isLaunching={launchingId === game.id}
                  onOpen={onOpenGame}
                  onLaunch={onLaunch}
                />
              ))
            }
          </Shelf>
        </section>
      ) : null,

    // Both draw themselves, or nothing when the stores have nothing to say
    recommendations: (
      <div key="recommendations" className="empty:hidden">
        {recommendations}
      </div>
    ),

    deals: (
      <div key="deals" className="empty:hidden">
        {deals}
      </div>
    ),

    // What the plugins add, one part each in the plugins' order. A plugin that has not
    // answered yet, or has nothing for today, takes no room.
    plugins: drawnParts.length > 0 ? (
      <Fragment key="plugins">
        {drawnParts.map((part) => (
          <HomePluginPart
            key={part.plugin.id}
            part={part}
            games={games}
            onOpenGame={onOpenGame}
            onLaunch={onLaunch}
          />
        ))}
      </Fragment>
    ) : null,
  };

  const drawn = shown.map((section) => sections[section]).filter((section) => section !== null);
  // A plugin that has not answered yet may still have something for the page, and so may one
  // not listed yet (at startup the plugins are read after the settings): until they have,
  // nobody can say there is nothing to show
  const awaitingPlugins =
    shown.includes("plugins") && (!pluginsListed || pluginParts.some((part) => part.answer === null));

  return (
    <CardTellingContext.Provider value={telling}>
    <div className="home-page relative space-y-7">
      {ambience && <HomeAmbience kind={ambience.kind} themeKey={themeKey} />}

      <header className="relative flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-baseline gap-3">
          <h1 id="home-title" className="text-lg font-semibold tracking-tight text-zinc-50">
            {t("sidebar.home")}
          </h1>
          {ambience && (
            <span id="home-ambience-label" data-reason={ambience.reason} className="text-xs text-zinc-400">
              {weatherPlace && ambience.reason.startsWith("weather")
                ? t("ambience.at", { what: t(`ambience.${ambience.reason}`), place: weatherPlace })
                : t(`ambience.${ambience.reason}`)}
            </span>
          )}
          {/* Where weather data is shown, its source is credited, as its licence asks */}
          {weatherPlace && (
            <button
              id="home-weather-source"
              type="button"
              onClick={onOpenWeatherSource}
              className="text-2xs text-zinc-500 underline-offset-2 transition-colors hover:text-zinc-200 hover:underline"
            >
              {t("ambience.source")}
            </button>
          )}
        </div>
        <button
          id="home-settings-button"
          type="button"
          onClick={onOpenSettings}
          className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg/70 px-3 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
        >
          <SlidersHorizontal className="h-3.5 w-3.5" />
          <span>{t("home.customize")}</span>
        </button>
      </header>

      <div className="relative space-y-7">
        {drawn.length > 0 ? (
          drawn
        ) : awaitingPlugins ? null : (
          <div id="home-empty" className="rounded-xl border border-oled-border bg-oled-surface/40 px-6 py-12 text-center">
            <p className="mx-auto max-w-xl text-sm leading-relaxed text-zinc-400">{t("home.empty")}</p>
          </div>
        )}
      </div>
    </div>
    </CardTellingContext.Provider>
  );
}
