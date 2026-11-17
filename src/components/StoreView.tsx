import { useCallback, useEffect, useMemo, useState } from "react";
import { BellRing, ExternalLink, EyeOff, Heart, RefreshCw, SlidersHorizontal, WifiOff } from "lucide-react";
import { detectCountry, useI18n, type MessageKey } from "../i18n";
import { DEAL_STORES, fetchDeals, openStorePage } from "../lib/backend";
import { storePicks } from "../lib/storePicks";
import type {
  Deal,
  DealsReport,
  DealStore,
  DetectedGame,
  LikedDeal,
  StoreConfig,
  StoreTab,
  ThemeStore,
} from "../types/game";

const STORE_NAMES: Record<Exclude<DealStore, "others">, string> = {
  steam: "Steam",
  epic: "Epic Games",
  gog: "GOG",
};

/** A store's name; the "others" are a group, named in the user's language. */
export function storeLabel(store: DealStore, t: (key: MessageKey) => string): string {
  return store === "others" ? t("store.others") : STORE_NAMES[store];
}

const STORE_BADGE: Record<DealStore, string> = {
  steam: "border-sky-500/30 bg-sky-500/15 text-sky-300",
  epic: "border-zinc-500/30 bg-zinc-500/20 text-zinc-200",
  gog: "border-fuchsia-500/30 bg-fuchsia-500/15 text-fuchsia-300",
  others: "border-emerald-500/30 bg-emerald-500/15 text-emerald-300",
};

/** A liked deal is "about to end" from this long before its end. */
export const ENDING_SOON_MS = 48 * 60 * 60 * 1000;

/** The liked deals that end within two days and have not ended yet. */
export function likedEndingSoon(liked: LikedDeal[], now = Date.now()): LikedDeal[] {
  return liked.filter((deal) => deal.endsAt !== null && deal.endsAt > now && deal.endsAt - now <= ENDING_SOON_MS);
}

type Sort = "discount" | "price" | "ending" | "title";
const SORTS: [Sort, MessageKey][] = [
  ["discount", "store.sortDiscount"],
  ["price", "store.sortPrice"],
  ["ending", "store.sortEnding"],
  ["title", "store.sortTitle"],
];

interface DealTileProps {
  deal: Deal;
  owned: boolean;
  liked: boolean;
  onOpen: (deal: Deal) => void;
  onToggleLike: (deal: Deal) => void;
  onHide: (deal: Deal) => void;
}

/** What a tile and a row say of a deal in words: who sells it, and when it starts or ends. */
function useDealWords(deal: Deal) {
  const { t, shortDate, relativeFuture } = useI18n();
  // Among the other shops, the one that sells it is the name that matters
  const store = deal.seller ?? storeLabel(deal.store, t);
  const endsSoon = deal.endsAt !== null && deal.endsAt > Date.now() && deal.endsAt - Date.now() <= ENDING_SOON_MS;
  const timing = deal.upcoming
    ? deal.startsAt !== null
      ? t("deals.freeFrom", { date: shortDate(deal.startsAt) })
      : null
    : deal.endsAt !== null
      ? endsSoon
        ? t("store.endsIn", { when: relativeFuture(deal.endsAt) })
        : t("deals.until", { date: shortDate(deal.endsAt) })
      : null;
  return { store, endsSoon, timing };
}

function DealTile({ deal, owned, liked, onOpen, onToggleLike, onHide }: DealTileProps) {
  const { t, money } = useI18n();
  const [imageFailed, setImageFailed] = useState(false);
  const { store, endsSoon, timing } = useDealWords(deal);
  const isFree = deal.finalPrice === 0;
  const chip = "rounded-md border border-white/15 bg-black/75 p-1.5 text-white/85 transition-colors hover:bg-black hover:text-white";

  return (
    <li className="game-card group relative flex h-full flex-col" data-deal={deal.id} data-store={deal.store}>
      <div className="relative aspect-[460/215] w-full overflow-hidden bg-oled-bg">
        {deal.imageUrl && !imageFailed ? (
          <img
            src={deal.imageUrl}
            alt=""
            loading="lazy"
            draggable={false}
            onError={() => setImageFailed(true)}
            className="game-card-art h-full w-full object-cover"
          />
        ) : (
          <div className="flex h-full w-full items-end bg-gradient-to-b from-oled-elevated/50 to-oled-bg p-3">
            <span className="line-clamp-2 text-sm font-semibold text-zinc-300">{deal.title}</span>
          </div>
        )}
        {/* The whole picture opens the store's page */}
        <button
          type="button"
          onClick={() => onOpen(deal)}
          aria-label={t("deals.openStore", { title: deal.title, store })}
          className="absolute inset-0 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
        />
        {!deal.upcoming && (
          <span className="pointer-events-none absolute left-2 top-2 rounded-md bg-accent px-1.5 py-0.5 text-2xs font-bold tabular-nums text-zinc-950 shadow">
            {isFree ? t("deals.free") : `-${deal.discountPercent}%`}
          </span>
        )}
        <div className="absolute right-2 top-2 flex items-center gap-1">
          <button
            type="button"
            data-hide-deal={deal.id}
            onClick={() => onHide(deal)}
            aria-label={t("store.hideDeal", { title: deal.title })}
            title={t("store.hideDeal", { title: deal.title })}
            className={`${chip} opacity-0 focus-visible:opacity-100 group-hover:opacity-100`}
          >
            <EyeOff className="h-3 w-3" />
          </button>
          <button
            type="button"
            data-like-deal={deal.id}
            onClick={() => onToggleLike(deal)}
            aria-pressed={liked}
            aria-label={t(liked ? "store.unlike" : "store.like", { title: deal.title })}
            title={t(liked ? "store.unlike" : "store.like", { title: deal.title })}
            className={`${chip} ${liked ? "opacity-100" : "opacity-0 focus-visible:opacity-100 group-hover:opacity-100"}`}
          >
            <Heart className={`h-3 w-3 ${liked ? "favorite-on-cover" : ""}`} />
          </button>
        </div>
        {owned && (
          <span className="pointer-events-none absolute bottom-2 left-2 rounded-md border border-white/15 bg-black/75 px-1.5 py-0.5 text-3xs font-medium text-white">
            {t("deals.owned")}
          </span>
        )}
      </div>

      <div className="flex flex-1 flex-col gap-2 px-3 py-2.5">
        <div className="flex items-start justify-between gap-2">
          <h3 className="line-clamp-2 text-xs font-medium leading-snug text-zinc-100" title={deal.title}>
            {deal.title}
          </h3>
          <ExternalLink className="mt-0.5 h-3 w-3 shrink-0 text-zinc-500 opacity-0 transition-opacity group-hover:opacity-100" />
        </div>
        {deal.genres.length > 0 && (
          <p className="truncate text-3xs text-zinc-500">{deal.genres.slice(0, 3).join(" · ")}</p>
        )}
        <div className="mt-auto flex items-end justify-between gap-2">
          <div className="min-w-0">
            <span className={`inline-block rounded border px-1.5 py-px text-3xs font-medium ${STORE_BADGE[deal.store]}`}>
              {store}
            </span>
            {timing && (
              <p className={`mt-1 truncate text-3xs ${endsSoon ? "font-medium text-amber-300" : "text-zinc-500"}`}>{timing}</p>
            )}
          </div>
          <div className="shrink-0 text-right tabular-nums">
            {!deal.upcoming && deal.originalPrice !== null && deal.originalPrice > deal.finalPrice && (
              <p className="text-3xs text-zinc-500 line-through">{money(deal.originalPrice, deal.currency)}</p>
            )}
            <p className="text-sm font-semibold text-zinc-100">
              {deal.upcoming
                ? deal.originalPrice !== null
                  ? money(deal.originalPrice, deal.currency)
                  : ""
                : isFree
                  ? t("deals.free")
                  : money(deal.finalPrice, deal.currency)}
            </p>
          </div>
        </div>
      </div>
    </li>
  );
}

/**
 * A deal as a row of a list: a small picture, then title and genres, the store, when it
 * ends, the discount and the price. The whole row opens the store's page; liking and
 * hiding are the same two actions a tile has.
 */
function DealRow({ deal, owned, liked, index, onOpen, onToggleLike, onHide }: DealTileProps & { index: number }) {
  const { t, money } = useI18n();
  const [imageFailed, setImageFailed] = useState(false);
  const { store, endsSoon, timing } = useDealWords(deal);
  const isFree = deal.finalPrice === 0;
  const action = "rounded-lg p-1.5 transition-colors hover:bg-oled-elevated";
  // One color at a time: two color classes on one button would leave the choice to their
  // order in the stylesheet
  const quiet = "text-zinc-500 hover:text-zinc-200";

  return (
    // The row wraps where it is too narrow for its words (a small window, larger text):
    // price and actions go under the title, which keeps a width it can be read in
    <li
      className="motion-row group relative flex flex-wrap items-center gap-x-3.5 gap-y-2 px-3.5 py-2.5 transition-colors hover:bg-oled-elevated/60"
      style={{ "--i": index } as React.CSSProperties}
      data-deal={deal.id}
      data-store={deal.store}
    >
      {/* The whole row opens the store's page; what can be pressed on it sits above */}
      <button
        type="button"
        onClick={() => onOpen(deal)}
        aria-label={t("deals.openStore", { title: deal.title, store })}
        title={deal.title}
        className="absolute inset-0 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
      />
      <div className="h-12 w-[6.5rem] shrink-0 overflow-hidden rounded-md border border-oled-border bg-oled-bg">
        {deal.imageUrl && !imageFailed && (
          <img
            src={deal.imageUrl}
            alt=""
            loading="lazy"
            draggable={false}
            onError={() => setImageFailed(true)}
            className="h-full w-full object-cover"
          />
        )}
      </div>

      <div className="min-w-[9rem] flex-1 space-y-1">
        <div className="flex items-center gap-2">
          <h3 className="truncate text-sm font-medium text-zinc-100" title={deal.title}>
            {deal.title}
          </h3>
          <span className={`shrink-0 rounded border px-1.5 py-px text-3xs font-medium ${STORE_BADGE[deal.store]}`}>{store}</span>
          {owned && (
            <span className="shrink-0 rounded border border-oled-border px-1.5 py-px text-3xs font-medium text-zinc-400">
              {t("deals.owned")}
            </span>
          )}
        </div>
        {(deal.genres.length > 0 || timing) && (
          <p className="truncate text-2xs text-zinc-500">
            {deal.genres.slice(0, 3).join(" · ")}
            {deal.genres.length > 0 && timing && " · "}
            {timing && <span className={endsSoon ? "font-medium text-amber-300" : undefined}>{timing}</span>}
          </p>
        )}
      </div>

      {/* Price and actions stay together, at the end of the row or of the line under it */}
      <div className="ml-auto flex shrink-0 items-center gap-3.5">
        <div className="flex shrink-0 items-center gap-3 tabular-nums">
          {!deal.upcoming && (
            <span className="rounded-md bg-accent px-1.5 py-0.5 text-2xs font-bold text-zinc-950">
              {isFree ? t("deals.free") : `-${deal.discountPercent}%`}
            </span>
          )}
          <div className="min-w-[4.5rem] text-right">
            {!deal.upcoming && deal.originalPrice !== null && deal.originalPrice > deal.finalPrice && (
              <p className="text-3xs leading-tight text-zinc-500 line-through">{money(deal.originalPrice, deal.currency)}</p>
            )}
            <p className="text-sm font-semibold leading-tight text-zinc-100">
              {deal.upcoming
                ? deal.originalPrice !== null
                  ? money(deal.originalPrice, deal.currency)
                  : ""
                : isFree
                  ? t("deals.free")
                  : money(deal.finalPrice, deal.currency)}
            </p>
          </div>
        </div>

        <div className="relative flex shrink-0 items-center gap-0.5">
          <button
            type="button"
            data-hide-deal={deal.id}
            onClick={() => onHide(deal)}
            aria-label={t("store.hideDeal", { title: deal.title })}
            title={t("store.hideDeal", { title: deal.title })}
            className={`${action} ${quiet}`}
          >
            <EyeOff className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            data-like-deal={deal.id}
            onClick={() => onToggleLike(deal)}
            aria-pressed={liked}
            aria-label={t(liked ? "store.unlike" : "store.like", { title: deal.title })}
            title={t(liked ? "store.unlike" : "store.like", { title: deal.title })}
            className={`${action} ${liked ? "text-accent" : quiet}`}
          >
            <Heart className={`h-3.5 w-3.5 ${liked ? "fill-current" : ""}`} />
          </button>
        </div>
      </div>
    </li>
  );
}

/** The deals of a tab: tiles in a grid or rows in a list, as the theme says. */
function Deals({ layout, children }: { layout: ThemeStore["layout"]; children: React.ReactNode }) {
  if (layout === "list") {
    return (
      <div className="overflow-hidden rounded-xl border border-oled-border bg-oled-surface/50">
        <ul className="divide-y divide-oled-border" data-store-layout="list">
          {children}
        </ul>
      </div>
    );
  }
  return (
    <ul
      className="grid"
      data-store-layout="grid"
      style={{ gap: "var(--grid-gap)", gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))" }}
    >
      {children}
    </ul>
  );
}

function Figure({ id, label, value, note }: { id: string; label: string; value: string; note?: string }) {
  return (
    <div className="tile flex min-w-0 flex-col justify-between gap-2 p-4" data-rank="key">
      <span className="micro-label">{label}</span>
      <div className="min-w-0">
        <p id={id} className="truncate text-2xl font-semibold leading-tight text-zinc-50">
          {value}
        </p>
        {/* Always a line, so that the four figures sit at the same height */}
        <p className="mt-0.5 truncate text-2xs text-zinc-500" title={note}>
          {note ?? "\u00a0"}
        </p>
      </div>
    </div>
  );
}

interface StoreViewProps {
  /** Every game of the library, for what the user plays and already owns. */
  games: DetectedGame[];
  ownedSteamAppIds: ReadonlySet<string>;
  store: StoreConfig;
  onChangeStore: (next: StoreConfig) => void;
  /** How the active theme arranges the page: its tabs, tiles or rows, the figures. */
  theme: ThemeStore;
  onOpenSettings: () => void;
  onError: (message: string) => void;
  /**
   * What the Home page shows of the Store: a few suggestions by genre ("picks") or the best
   * discounts of the moment ("top").
   */
  compact?: "picks" | "top";
  /** Opens the whole Store page (from the compact form). */
  onOpenStore?: () => void;
}

/**
 * The Store page: the discounts running on the stores Kanso can ask without an account, one
 * tab per store. A deal can be liked (Kanso then says when it is about to end) or hidden;
 * a store can be left out altogether. The active theme says which tabs there are and in
 * which order (the page opens on the first), whether deals are tiles or rows, and whether
 * the figures are shown; what it shows on the Home page is always a few tiles. "For you" picks, among the deals, the ones of the
 * genres the user plays the most and, for a change, some of genres they never play.
 *
 * Nothing about the user leaves the PC for this: the stores are asked for their public list
 * of discounts, and what suits the user is worked out here from the library.
 */
export function StoreView({
  games,
  ownedSteamAppIds,
  store,
  onChangeStore,
  theme,
  onOpenSettings,
  onError,
  compact,
  onOpenStore,
}: StoreViewProps) {
  const { t, lang, relativePast, relativeFuture, number } = useI18n();
  const [report, setReport] = useState<DealsReport | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  // The tab the user picked; until then, and whenever it is no longer offered, the first one
  const [pickedTab, setPickedTab] = useState<StoreTab | null>(null);
  const [sort, setSort] = useState<Sort>("discount");

  const load = useCallback(
    async (forceRefresh: boolean) => {
      setIsLoading(true);
      setFailed(false);
      try {
        setReport(await fetchDeals(detectCountry(lang), forceRefresh));
      } catch {
        setFailed(true);
      } finally {
        setIsLoading(false);
      }
    },
    // A store shown again is asked afresh: the backend does not ask the ones left out
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [lang, store.hiddenStores.join("|")]
  );
  useEffect(() => {
    void load(false);
  }, [load]);

  const shownStores = DEAL_STORES.filter((name) => !store.hiddenStores.includes(name));
  const likedIds = useMemo(() => new Set(store.liked.map((deal) => deal.id)), [store.liked]);
  // What the page works on: the stores the user keeps, less the deals they hid
  const deals = useMemo(() => {
    const hidden = new Set(store.hiddenDeals);
    return (report?.deals ?? []).filter((deal) => shownStores.includes(deal.store) && !hidden.has(deal.id));
    // `shownStores` is derived from `store.hiddenStores`
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [report, store.hiddenDeals, store.hiddenStores]);
  const picks = useMemo(() => storePicks(deals, games, ownedSteamAppIds), [deals, games, ownedSteamAppIds]);

  const openDeal = async (deal: { url: string }) => {
    try {
      await openStorePage(deal.url);
    } catch (err) {
      onError(typeof err === "string" ? err : t("deals.openError"));
    }
  };
  const toggleLike = (deal: Deal) =>
    onChangeStore({
      ...store,
      liked: likedIds.has(deal.id)
        ? store.liked.filter((entry) => entry.id !== deal.id)
        : [...store.liked, { id: deal.id, store: deal.store, title: deal.title, url: deal.url, endsAt: deal.endsAt }],
    });
  const hideDeal = (deal: Deal) =>
    onChangeStore({
      ...store,
      hiddenDeals: [...store.hiddenDeals.filter((id) => id !== deal.id), deal.id],
      // A deal the user no longer wants to see is not one they like
      liked: store.liked.filter((entry) => entry.id !== deal.id),
    });
  // On Home a deal is always a tile; on the page it is what the theme says
  const layout = compact ? "grid" : theme.layout;
  const tile = (deal: Deal, index: number) => {
    const entry: DealTileProps = {
      deal,
      owned: deal.steamAppid !== null && ownedSteamAppIds.has(deal.steamAppid),
      liked: likedIds.has(deal.id),
      onOpen: (d) => void openDeal(d),
      onToggleLike: toggleLike,
      onHide: hideDeal,
    };
    return layout === "list" ? (
      <DealRow key={deal.id} index={index} {...entry} />
    ) : (
      <DealTile key={deal.id} {...entry} />
    );
  };

  // On the Home page: a few suggestions, or the best discounts, and the way to the rest
  if (compact) {
    const shown =
      compact === "picks"
        ? [...picks.alike.slice(0, 3), ...picks.different.slice(0, 2)]
        : [...deals]
            .filter((deal) => !deal.upcoming)
            .sort((a, b) => b.discountPercent - a.discountPercent || a.title.localeCompare(b.title))
            .slice(0, 5);
    if (shown.length === 0) return null;
    const heading = compact === "picks" ? "home-picks" : "home-deals";
    return (
      <section aria-labelledby={heading} data-home-section={compact === "picks" ? "recommendations" : "deals"} className="space-y-3">
        <header className="flex flex-wrap items-baseline justify-between gap-3">
          <div className="flex min-w-0 items-baseline gap-2.5">
            <h2 id={heading} className="home-heading text-sm font-semibold">
              {t(compact === "picks" ? "store.picks" : "deals.title")}
            </h2>
            <span className="truncate text-2xs text-zinc-500">
              {compact === "picks" ? t("store.picksNote", { genres: picks.favourites.slice(0, 3).join(", ") }) : t("store.topNote")}
            </span>
          </div>
          {onOpenStore && (
            <button
              type="button"
              data-open-store
              onClick={onOpenStore}
              className="shrink-0 text-2xs text-zinc-400 underline-offset-2 hover:text-zinc-100 hover:underline"
            >
              {t("store.seeAll")}
            </button>
          )}
        </header>
        <Deals layout="grid">{shown.map(tile)}</Deals>
      </section>
    );
  }

  const now = Date.now();
  const endingSoon = likedEndingSoon(store.liked, now);
  const running = deals.filter((deal) => !deal.upcoming);
  const best = running.reduce<Deal | null>((top, deal) => (top === null || deal.discountPercent > top.discountPercent ? deal : top), null);
  const sorted = (list: Deal[]) =>
    [...list].sort((a, b) => {
      // What has not started yet comes after what can be had now, whatever the order
      if (a.upcoming !== b.upcoming) return a.upcoming ? 1 : -1;
      switch (sort) {
        case "price":
          return a.finalPrice - b.finalPrice || a.title.localeCompare(b.title);
        case "ending":
          return (a.endsAt ?? Number.MAX_SAFE_INTEGER) - (b.endsAt ?? Number.MAX_SAFE_INTEGER) || a.title.localeCompare(b.title);
        case "title":
          return a.title.localeCompare(b.title);
        default:
          return b.discountPercent - a.discountPercent || a.title.localeCompare(b.title);
      }
    });
  // The tabs the theme lists, in its order. A store the user hid has no tab, whatever the
  // theme says
  const themed: [StoreTab, string, number][] = theme.tabs.flatMap((name): [StoreTab, string, number][] => {
    switch (name) {
      case "all":
        return [[name, t("store.all"), deals.length]];
      case "picks":
        return [[name, t("store.picks"), picks.alike.length + picks.different.length]];
      case "liked":
        return [[name, t("store.liked"), store.liked.length]];
      default:
        return shownStores.includes(name)
          ? [[name, storeLabel(name, t), deals.filter((deal) => deal.store === name).length]]
          : [];
    }
  });
  // The deals the user likes are theirs: while there are any, their tab is there, last,
  // also under a theme that leaves it out. It is the one place that lists them, where one
  // can be taken off the list, and what the alert about deals ending leads to
  const tabs: [StoreTab, string, number][] =
    theme.tabs.includes("liked") || store.liked.length === 0
      ? themed
      : [...themed, ["liked", t("store.liked"), store.liked.length]];
  // `null` when the theme lists only stores and the user hid every one of them
  const tab: StoreTab | null = tabs.some(([name]) => name === pickedTab) ? pickedTab : (tabs[0]?.[0] ?? null);
  // The liked deals as the stores list them now; the ones no longer listed, as remembered
  const likedNow = store.liked.map((entry) => ({ entry, deal: (report?.deals ?? []).find((deal) => deal.id === entry.id) ?? null }));
  const offline = failed || (report !== null && report.deals.length === 0 && report.failedStores.length > 0);
  const listed = tab === "all" ? deals : deals.filter((deal) => deal.store === tab);

  return (
    <div className="store-page space-y-5" data-store-layout={layout}>
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 id="store-title" className="text-lg font-semibold tracking-tight text-zinc-50">
            {t("sidebar.store")}
          </h1>
          <p className="mt-0.5 text-xs text-zinc-400">{t("store.subtitle")}</p>
        </div>
        <div className="flex items-center gap-2.5">
          {report && !isLoading && report.deals.length > 0 && (
            <span className="text-2xs text-zinc-500">{t("deals.updated", { time: relativePast(report.fetchedAt) })}</span>
          )}
          <button
            id="store-settings-button"
            type="button"
            onClick={onOpenSettings}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
          >
            <SlidersHorizontal className="h-3.5 w-3.5" />
            <span>{t("home.customize")}</span>
          </button>
          <button
            id="refresh-store-button"
            type="button"
            onClick={() => void load(true)}
            disabled={isLoading}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-surface px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-700 hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-60"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isLoading ? "animate-spin text-accent" : "text-zinc-400"}`} />
            <span>{t("deals.refresh")}</span>
          </button>
        </div>
      </header>

      {/* The stores, as tabs: what the page lists */}
      {tabs.length > 0 && (
        <div id="store-tabs" role="tablist" aria-label={t("store.tabs")} className="flex flex-wrap gap-1.5">
          {tabs.map(([name, label, count]) => (
            <button
              key={name}
              id={`store-tab-${name}`}
              type="button"
              role="tab"
              aria-selected={tab === name}
              onClick={() => setPickedTab(name)}
              className={`inline-flex items-center gap-2 rounded-lg border px-3.5 py-1.5 text-xs font-medium transition-colors ${
                tab === name
                  ? "border-accent bg-accent text-zinc-950"
                  : "border-oled-border bg-oled-surface/60 text-zinc-300 hover:bg-oled-elevated"
              }`}
            >
              {name === "liked" && <Heart className={`h-3 w-3 ${tab === name ? "fill-current" : ""}`} />}
              <span>{label}</span>
              <span className={`tabular-nums ${tab === name ? "opacity-75" : "text-zinc-500"}`}>{count}</span>
            </button>
          ))}
        </div>
      )}

      {/* A liked deal about to end is said first, wherever on the page one is */}
      {endingSoon.length > 0 && (
        <div id="store-ending-soon" role="status" className="flex items-start gap-2.5 rounded-xl border border-amber-400/40 bg-amber-400/10 px-4 py-3 text-xs text-amber-100">
          <BellRing className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-300" />
          <p className="min-w-0 leading-relaxed">
            <span className="font-medium">{t("store.endingSoon")}</span>{" "}
            {endingSoon.map((deal, index) => (
              <span key={deal.id}>
                {index > 0 && ", "}
                <button type="button" onClick={() => void openDeal(deal)} className="underline underline-offset-2 hover:text-white">
                  {deal.title}
                </button>
                {deal.endsAt !== null && <span className="opacity-80"> ({relativeFuture(deal.endsAt)})</span>}
              </span>
            ))}
          </p>
        </div>
      )}

      {report && report.failedStores.length > 0 && report.deals.length > 0 && (
        <p role="status" className="text-2xs text-amber-200">
          {t("deals.failedStores", { stores: report.failedStores.map((name) => storeLabel(name, t)).join(", ") })}
        </p>
      )}

      {isLoading && deals.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-24 text-center">
          <div className="mb-3 h-6 w-6 animate-spin rounded-full border-2 border-oled-border border-t-accent" />
          <p className="text-sm text-zinc-300">{t("deals.loading")}</p>
        </div>
      ) : tab === "liked" ? (
        likedNow.length === 0 ? (
          <div id="store-liked-empty" className="rounded-xl border border-oled-border bg-oled-surface/60 px-6 py-14 text-center">
            <p className="mx-auto max-w-xl text-sm leading-relaxed text-zinc-300">{t("store.likedEmpty")}</p>
          </div>
        ) : (
          <div className="space-y-4">
            {/* Nothing is drawn for no deal: a list would leave its empty frame */}
            {likedNow.some(({ deal }) => deal !== null) && (
              <Deals layout={layout}>{likedNow.flatMap(({ deal }) => (deal ? [deal] : [])).map(tile)}</Deals>
            )}
            {likedNow.some(({ deal }) => deal === null) && (
              <section id="store-liked-gone" className="space-y-2">
                <h2 className="micro-label">{t("store.likedGone")}</h2>
                <ul className="space-y-1">
                  {likedNow
                    .filter(({ deal }) => deal === null)
                    .map(({ entry }) => (
                      <li key={entry.id} className="flex items-center justify-between gap-3 rounded-lg border border-oled-border bg-oled-surface/50 px-3.5 py-2 text-xs">
                        <button type="button" onClick={() => void openDeal(entry)} className="min-w-0 truncate text-left text-zinc-200 underline-offset-2 hover:underline">
                          {entry.title} <span className="text-zinc-500">· {storeLabel(entry.store, t)}</span>
                        </button>
                        <button
                          type="button"
                          onClick={() => onChangeStore({ ...store, liked: store.liked.filter((liked) => liked.id !== entry.id) })}
                          className="shrink-0 text-2xs text-zinc-400 underline-offset-2 hover:text-zinc-100 hover:underline"
                        >
                          {t("store.forget")}
                        </button>
                      </li>
                    ))}
                </ul>
              </section>
            )}
          </div>
        )
      ) : tab === "picks" ? (
        picks.alike.length === 0 && picks.different.length === 0 ? (
          <div id="store-picks-empty" className="rounded-xl border border-oled-border bg-oled-surface/60 px-6 py-14 text-center">
            <p className="mx-auto max-w-xl text-sm leading-relaxed text-zinc-300">{t("store.picksEmpty")}</p>
          </div>
        ) : (
          <div className="space-y-6">
            {picks.alike.length > 0 && (
              <section id="store-picks-alike" className="space-y-2.5">
                <header className="flex flex-wrap items-baseline gap-2.5">
                  <h2 className="home-heading text-sm font-semibold">{t("store.picksAlike")}</h2>
                  <span className="text-2xs text-zinc-500">{picks.favourites.join(" · ")}</span>
                </header>
                <Deals layout={layout}>{picks.alike.map(tile)}</Deals>
              </section>
            )}
            {picks.different.length > 0 && (
              <section id="store-picks-different" className="space-y-2.5">
                <header className="flex flex-wrap items-baseline gap-2.5">
                  <h2 className="home-heading text-sm font-semibold">{t("store.picksDifferent")}</h2>
                  <span className="text-2xs text-zinc-500">{t("store.picksDifferentNote")}</span>
                </header>
                <Deals layout={layout}>{picks.different.map(tile)}</Deals>
              </section>
            )}
            <p className="text-2xs leading-relaxed text-zinc-500">{t("store.picksHow")}</p>
          </div>
        )
      ) : listed.length === 0 ? (
        <div id="store-empty" className="rounded-xl border border-oled-border bg-oled-surface/60 px-6 py-14 text-center">
          {offline && <WifiOff className="mx-auto mb-3 h-7 w-7 text-zinc-500" />}
          <p className="text-sm text-zinc-300">{t(offline ? "deals.offline" : shownStores.length === 0 ? "store.noStores" : tab === null ? "store.noTabs" : "deals.empty")}</p>
        </div>
      ) : (
        <>
          {tab === "all" && theme.figures && (
            <div id="store-figures" className="grid grid-cols-2 gap-3 lg:grid-cols-4">
              <Figure id="store-count" label={t("store.figureDeals")} value={number(running.length)} note={shownStores.map((name) => storeLabel(name, t)).join(" · ")} />
              <Figure id="store-free" label={t("store.figureFree")} value={number(running.filter((deal) => deal.finalPrice === 0).length)} />
              <Figure id="store-best" label={t("store.figureBest")} value={best ? `-${best.discountPercent}%` : "—"} note={best?.title} />
              <Figure
                id="store-ending"
                label={t("store.figureEnding")}
                value={number(running.filter((deal) => deal.endsAt !== null && deal.endsAt > now && deal.endsAt - now <= ENDING_SOON_MS).length)}
                note={t("store.figureEndingNote")}
              />
            </div>
          )}
          {tab === "others" && (
            <p id="store-others-note" className="text-2xs leading-relaxed text-zinc-500">
              {t("store.othersNote")}
            </p>
          )}
          <div className="flex items-center justify-end gap-2">
            <label htmlFor="store-sort" className="text-2xs text-zinc-500">
              {t("store.sort")}
            </label>
            <select
              id="store-sort"
              value={sort}
              onChange={(event) => setSort(event.target.value as Sort)}
              className="rounded-lg border border-oled-border bg-oled-bg px-2.5 py-1 text-xs text-zinc-200 focus:border-accent/60 focus:outline-none"
            >
              {SORTS.map(([name, label]) => (
                <option key={name} value={name}>
                  {t(label)}
                </option>
              ))}
            </select>
          </div>
          <Deals layout={layout}>{sorted(listed).map(tile)}</Deals>
        </>
      )}

      <p className="text-2xs leading-relaxed text-zinc-500">{t("store.about")}</p>
    </div>
  );
}
