import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ExternalLink, RefreshCw, SlidersHorizontal, WifiOff } from "lucide-react";
import { useI18n } from "../i18n";
import { fetchNews, openNewsLink } from "../lib/backend";
import type { NewsItem, NewsReport, NewsTopic } from "../types/game";

type Tab = "all" | NewsTopic;
const TABS: [Tab, "news.all" | "news.gaming" | "news.hardware"][] = [
  ["all", "news.all"],
  ["gaming", "news.gaming"],
  ["hardware", "news.hardware"],
];

const TOPIC_BADGE: Record<NewsTopic, string> = {
  gaming: "border-sky-500/30 bg-sky-500/15 text-sky-300",
  hardware: "border-emerald-500/30 bg-emerald-500/15 text-emerald-300",
};

interface NewsCardProps {
  item: NewsItem;
  /** The first piece of news, drawn wide with its picture beside it. */
  lead?: boolean;
  onOpen: (item: NewsItem) => void;
}

function NewsCard({ item, lead = false, onOpen }: NewsCardProps) {
  const { t, relativePast } = useI18n();
  const [imageFailed, setImageFailed] = useState(false);
  const hasPicture = Boolean(item.imageUrl) && !imageFailed;
  return (
    <li
      className="game-card group relative flex h-full flex-col overflow-hidden"
      data-news={item.id}
      data-topic={item.topic}
      data-lead={lead || undefined}
    >
      {hasPicture && (
        <div className="news-picture relative aspect-[16/9] w-full shrink-0 overflow-hidden bg-oled-bg">
          <img
            src={item.imageUrl ?? undefined}
            alt=""
            loading="lazy"
            draggable={false}
            referrerPolicy="no-referrer"
            onError={() => setImageFailed(true)}
            className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105"
          />
        </div>
      )}
      <div className={`flex min-w-0 flex-1 flex-col gap-2 ${lead ? "p-5" : "px-3.5 py-3"}`}>
        <div className="flex items-center gap-2 text-3xs">
          <span className={`inline-block rounded border px-1.5 py-px font-medium ${TOPIC_BADGE[item.topic]}`}>{t(`news.${item.topic}`)}</span>
          <span className="truncate font-medium text-zinc-300">{item.sourceName}</span>
          {item.publishedAt !== null && <span className="shrink-0 text-zinc-500">· {relativePast(item.publishedAt)}</span>}
        </div>
        <h3 className={`font-semibold leading-snug text-zinc-100 ${lead ? "text-lg" : "line-clamp-3 text-sm"}`}>{item.title}</h3>
        {item.summary && <p className={`text-xs leading-relaxed text-zinc-400 ${lead ? "line-clamp-5" : "line-clamp-3"}`}>{item.summary}</p>}
        <span className="mt-auto inline-flex items-center gap-1 pt-1 text-3xs text-zinc-500 opacity-0 transition-opacity group-hover:opacity-100">
          <ExternalLink className="h-3 w-3" />
          {item.sourceName}
        </span>
      </div>
      {/* The whole card opens the article on the site that published it */}
      <button
        type="button"
        onClick={() => onOpen(item)}
        aria-label={t("news.open", { title: item.title, source: item.sourceName })}
        className="absolute inset-0 focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent"
      />
    </li>
  );
}

interface NewsViewProps {
  /** Changes when the user's choice of sources does: the page reads again. */
  sourcesKey: string;
  onOpenSettings: () => void;
  onError: (message: string) => void;
}

/**
 * The News page: the latest PC gaming and hardware news from the sites the user keeps, read
 * from their public feeds. Each piece opens on the site that published it.
 */
export function NewsView({ sourcesKey, onOpenSettings, onError }: NewsViewProps) {
  const { t, relativePast } = useI18n();
  const [report, setReport] = useState<NewsReport | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [tab, setTab] = useState<Tab>("all");
  const [source, setSource] = useState<string | null>(null);

  // Only the answer to the latest question counts: an older, slower one (asked before the
  // sources were changed) must not put back what it found
  const latest = useRef(0);
  const load = useCallback(
    async (forceRefresh: boolean) => {
      const mine = (latest.current += 1);
      setIsLoading(true);
      setFailed(false);
      try {
        const answer = await fetchNews(forceRefresh);
        if (mine === latest.current) setReport(answer);
      } catch {
        if (mine === latest.current) setFailed(true);
      } finally {
        if (mine === latest.current) setIsLoading(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sourcesKey]
  );
  useEffect(() => {
    void load(false);
  }, [load]);

  const items = useMemo(() => report?.items ?? [], [report]);
  // The sites that have news right now, in the order their news come
  const sources = useMemo(() => {
    const names = new Map<string, string>();
    for (const item of items) if (tab === "all" || item.topic === tab) names.set(item.source, item.sourceName);
    return [...names];
  }, [items, tab]);
  const shownSource = source !== null && sources.some(([id]) => id === source) ? source : null;
  const listed = items.filter((item) => (tab === "all" || item.topic === tab) && (shownSource === null || item.source === shownSource));

  const open = async (item: NewsItem) => {
    try {
      await openNewsLink(item.url);
    } catch (err) {
      onError(typeof err === "string" ? err : t("news.openError"));
    }
  };
  const offline = failed || (report !== null && report.items.length === 0 && report.failedSources.length > 0);
  const pill = (active: boolean) =>
    `inline-flex items-center gap-2 rounded-lg border px-3.5 py-1.5 text-xs font-medium transition-colors ${
      active ? "border-accent bg-accent text-zinc-950" : "border-oled-border bg-oled-surface/60 text-zinc-300 hover:bg-oled-elevated"
    }`;

  return (
    <div className="news-page space-y-5">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 id="news-title" className="text-lg font-semibold tracking-tight text-zinc-50">
            {t("sidebar.news")}
          </h1>
          <p className="mt-0.5 text-xs text-zinc-400">{t("news.subtitle")}</p>
        </div>
        <div className="flex items-center gap-2.5">
          {report && !isLoading && report.items.length > 0 && (
            <span className="text-2xs text-zinc-500">{t("deals.updated", { time: relativePast(report.fetchedAt) })}</span>
          )}
          <button
            id="news-settings-button"
            type="button"
            onClick={onOpenSettings}
            className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
          >
            <SlidersHorizontal className="h-3.5 w-3.5" />
            <span>{t("home.customize")}</span>
          </button>
          <button
            id="refresh-news-button"
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

      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div id="news-tabs" role="tablist" aria-label={t("news.topics")} className="flex flex-wrap gap-1.5">
          {TABS.map(([name, label]) => (
            <button key={name} id={`news-tab-${name}`} type="button" role="tab" aria-selected={tab === name} onClick={() => setTab(name)} className={pill(tab === name)}>
              <span>{t(label)}</span>
              <span className={`tabular-nums ${tab === name ? "opacity-75" : "text-zinc-500"}`}>
                {name === "all" ? items.length : items.filter((item) => item.topic === name).length}
              </span>
            </button>
          ))}
        </div>
        {/* One site at a time, for who follows one more than the others */}
        {sources.length > 1 && (
          <div id="news-sources" className="flex flex-wrap gap-1.5">
            {sources.map(([id, name]) => (
              <button
                key={id}
                type="button"
                data-news-source={id}
                aria-pressed={shownSource === id}
                onClick={() => setSource(shownSource === id ? null : id)}
                className={`rounded-full border px-2.5 py-1 text-2xs font-medium transition-colors ${
                  shownSource === id ? "border-accent/60 bg-accent/10 text-accent" : "border-oled-border bg-oled-bg/60 text-zinc-400 hover:border-zinc-600 hover:text-zinc-200"
                }`}
              >
                {name}
              </button>
            ))}
          </div>
        )}
      </div>

      {report && report.failedSources.length > 0 && report.items.length > 0 && (
        <p id="news-failed" role="status" className="text-2xs text-amber-200">
          {t("news.failedSources", { sources: report.failedSources.join(", ") })}
        </p>
      )}

      {isLoading && items.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-24 text-center">
          <div className="mb-3 h-6 w-6 animate-spin rounded-full border-2 border-oled-border border-t-accent" />
          <p className="text-sm text-zinc-300">{t("news.loading")}</p>
        </div>
      ) : listed.length === 0 ? (
        <div id="news-empty" className="rounded-xl border border-oled-border bg-oled-surface/60 px-6 py-14 text-center">
          {offline && <WifiOff className="mx-auto mb-3 h-7 w-7 text-zinc-500" />}
          <p className="text-sm text-zinc-300">{t(offline ? "news.offline" : "news.empty")}</p>
        </div>
      ) : (
        <ul className="news-list grid" style={{ gap: "var(--grid-gap)", gridTemplateColumns: "repeat(auto-fill, minmax(270px, 1fr))" }}>
          {listed.map((item, index) => (
            <NewsCard key={item.id} item={item} lead={index === 0 && Boolean(item.imageUrl)} onOpen={(piece) => void open(piece)} />
          ))}
        </ul>
      )}

      <p className="text-2xs leading-relaxed text-zinc-500">{t("news.about")}</p>
    </div>
  );
}
