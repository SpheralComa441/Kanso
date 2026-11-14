import { useCallback, useEffect, useMemo, useState } from "react";
import { ExternalLink, RefreshCw, WifiOff } from "lucide-react";
import { detectCountry, useI18n } from "../i18n";
import { fetchDeals, openStorePage } from "../lib/backend";
import type { Deal, DealsReport, DealStore } from "../types/game";

interface DealsViewProps {
  /** Steam appids of the games in the library, to mark deals the user already owns. */
  ownedSteamAppIds: ReadonlySet<string>;
  onError: (message: string) => void;
}

const STORE_NAMES: Record<DealStore, string> = {
  steam: "Steam",
  epic: "Epic Games",
  gog: "GOG",
};

const STORE_BADGE: Record<DealStore, string> = {
  steam: "border-sky-500/30 bg-sky-500/15 text-sky-300",
  epic: "border-zinc-500/30 bg-zinc-500/20 text-zinc-200",
  gog: "border-fuchsia-500/30 bg-fuchsia-500/15 text-fuchsia-300",
};

const SECTIONS: [DealStore, "deals.sectionEpic" | "deals.sectionSteam" | "deals.sectionGog"][] = [
  ["epic", "deals.sectionEpic"],
  ["steam", "deals.sectionSteam"],
  ["gog", "deals.sectionGog"],
];

function DealCard({
  deal,
  owned,
  onOpen,
}: {
  deal: Deal;
  owned: boolean;
  onOpen: (deal: Deal) => void;
}) {
  const { t, money, shortDate } = useI18n();
  const [imageFailed, setImageFailed] = useState(false);
  const store = STORE_NAMES[deal.store];
  const isFree = deal.finalPrice === 0;

  const timing = deal.upcoming
    ? deal.startsAt !== null
      ? t("deals.freeFrom", { date: shortDate(deal.startsAt) })
      : null
    : deal.endsAt !== null
      ? t("deals.until", { date: shortDate(deal.endsAt) })
      : null;

  return (
    <li>
      <button
        id={`deal-${deal.id}`}
        type="button"
        onClick={() => onOpen(deal)}
        aria-label={t("deals.openStore", { title: deal.title, store })}
        className="group flex h-full w-full flex-col overflow-hidden rounded-card border border-oled-border bg-oled-surface text-left transition-all duration-200 hover:-translate-y-0.5 hover:border-zinc-600/80 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
      >
        <div className="relative aspect-[460/215] w-full overflow-hidden bg-oled-bg">
          {deal.imageUrl && !imageFailed ? (
            <img
              src={deal.imageUrl}
              alt=""
              loading="lazy"
              draggable={false}
              onError={() => setImageFailed(true)}
              className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
            />
          ) : (
            <div className="flex h-full w-full items-end bg-gradient-to-b from-oled-elevated/50 to-oled-bg p-3">
              <span className="line-clamp-2 text-sm font-semibold text-zinc-300">
                {deal.title}
              </span>
            </div>
          )}

          {!deal.upcoming && (
            <span className="absolute left-2 top-2 rounded-md bg-accent px-1.5 py-0.5 text-[11px] font-bold tabular-nums text-zinc-950 shadow">
              {isFree ? t("deals.free") : `-${deal.discountPercent}%`}
            </span>
          )}
          {owned && (
            <span className="absolute right-2 top-2 rounded-md border border-white/15 bg-black/65 px-1.5 py-0.5 text-[10px] font-medium text-white backdrop-blur-md">
              {t("deals.owned")}
            </span>
          )}
        </div>

        <div className="flex flex-1 flex-col gap-2 px-3 py-2.5">
          <div className="flex items-start justify-between gap-2">
            <h3
              className="line-clamp-2 text-xs font-medium leading-snug text-zinc-100"
              title={deal.title}
            >
              {deal.title}
            </h3>
            <ExternalLink className="mt-0.5 h-3 w-3 shrink-0 text-zinc-500 opacity-0 transition-opacity group-hover:opacity-100" />
          </div>

          <div className="mt-auto flex items-end justify-between gap-2">
            <div className="min-w-0">
              <span
                className={`inline-block rounded border px-1.5 py-px text-[10px] font-medium ${STORE_BADGE[deal.store]}`}
              >
                {store}
              </span>
              {timing && (
                <p className="mt-1 truncate text-[10px] text-zinc-500">{timing}</p>
              )}
            </div>
            <div className="shrink-0 text-right tabular-nums">
              {/* An upcoming giveaway still costs its regular price today */}
              {!deal.upcoming &&
                deal.originalPrice !== null &&
                deal.originalPrice > deal.finalPrice && (
                  <p className="text-[10px] text-zinc-500 line-through">
                    {money(deal.originalPrice, deal.currency)}
                  </p>
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
      </button>
    </li>
  );
}

/** Home: free games and discounts currently running on Steam, Epic Games and GOG. */
export function DealsView({ ownedSteamAppIds, onError }: DealsViewProps) {
  const { t, lang, relativePast } = useI18n();
  const [report, setReport] = useState<DealsReport | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [failed, setFailed] = useState(false);

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
    [lang]
  );

  useEffect(() => {
    void load(false);
  }, [load]);

  const bySection = useMemo(
    () =>
      SECTIONS.map(([store, titleKey]) => ({
        store,
        titleKey,
        deals: (report?.deals ?? []).filter((deal) => deal.store === store),
      })).filter((section) => section.deals.length > 0),
    [report]
  );

  const handleOpen = async (deal: Deal) => {
    try {
      await openStorePage(deal.url);
    } catch (err) {
      onError(typeof err === "string" ? err : t("deals.openError"));
    }
  };

  const nothingToShow = !isLoading && bySection.length === 0;
  const offline =
    failed || (report !== null && report.deals.length === 0 && report.failedStores.length > 0);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold tracking-tight text-zinc-50">
            {t("deals.title")}
          </h2>
          <p className="mt-0.5 text-xs text-zinc-400">{t("deals.subtitle")}</p>
        </div>
        <div className="flex items-center gap-3">
          {report && !isLoading && report.deals.length > 0 && (
            <span className="text-[11px] text-zinc-500">
              {t("deals.updated", { time: relativePast(report.fetchedAt) })}
            </span>
          )}
          <button
            id="refresh-deals-button"
            type="button"
            onClick={() => void load(true)}
            disabled={isLoading}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-surface px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-700 hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-60"
          >
            <RefreshCw
              className={`h-3.5 w-3.5 ${
                isLoading ? "animate-spin text-accent" : "text-zinc-400"
              }`}
            />
            <span>{t("deals.refresh")}</span>
          </button>
        </div>
      </div>

      {report && report.failedStores.length > 0 && report.deals.length > 0 && (
        <p role="status" className="text-[11px] text-amber-200">
          {t("deals.failedStores", {
            stores: report.failedStores.map((store) => STORE_NAMES[store]).join(", "),
          })}
        </p>
      )}

      {isLoading && bySection.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-24 text-center">
          <div className="mb-3 h-6 w-6 animate-spin rounded-full border-2 border-oled-border border-t-accent" />
          <p className="text-sm text-zinc-300">{t("deals.loading")}</p>
        </div>
      ) : nothingToShow ? (
        <div className="rounded-xl border border-oled-border bg-oled-surface/60 px-6 py-14 text-center">
          {offline && <WifiOff className="mx-auto mb-3 h-7 w-7 text-zinc-500" />}
          <p className="text-sm text-zinc-300">
            {offline ? t("deals.offline") : t("deals.empty")}
          </p>
        </div>
      ) : (
        bySection.map((section) => (
          <section key={section.store} className="space-y-3">
            <h3 className="text-[10px] font-semibold uppercase tracking-[0.12em] text-zinc-500">
              {t(section.titleKey)}
            </h3>
            <ul
              className="grid gap-4"
              style={{ gridTemplateColumns: "repeat(auto-fill, minmax(230px, 1fr))" }}
            >
              {section.deals.map((deal) => (
                <DealCard
                  key={deal.id}
                  deal={deal}
                  owned={
                    deal.steamAppid !== null && ownedSteamAppIds.has(deal.steamAppid)
                  }
                  onOpen={(d) => void handleOpen(d)}
                />
              ))}
            </ul>
          </section>
        ))
      )}
    </div>
  );
}
