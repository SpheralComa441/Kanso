import { useEffect, useState } from "react";
import { LANGUAGE_NAMES, isLanguageCode, useI18n } from "../i18n";
import { Plus, Trash2 } from "lucide-react";
import { MAX_CUSTOM_FEEDS, isFeedUrl, newsSources } from "../lib/backend";
import type { CustomFeed, NewsSourceInfo, NewsTopic } from "../types/game";

/** The sources read for a language until the user chooses: its own, or the English ones. */
export function defaultNewsSources(all: NewsSourceInfo[], lang: string): string[] {
  const own = all.filter((source) => source.lang === lang);
  return (own.length > 0 ? own : all.filter((source) => source.lang === "en")).map((source) => source.id);
}

interface NewsSettingsProps {
  /** The panel is the one in view: the list of sources is asked for then. */
  active: boolean;
  /** The user's choice; `null` until they make one. */
  value: string[] | null;
  onChange: (next: string[] | null) => void;
  /** The language chosen in the dialog, saved or not: what "my language" means here. */
  language: string;
  /** Feeds the user added by hand. */
  feeds: CustomFeed[];
  onFeedsChange: (next: CustomFeed[]) => void;
}

/** Which sites the News page reads: every source Kanso knows, by language, each with a switch. */
export function NewsSettings({ active, value, onChange, language, feeds, onFeedsChange }: NewsSettingsProps) {
  const { t } = useI18n();
  const [all, setAll] = useState<NewsSourceInfo[] | null>(null);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [topic, setTopic] = useState<NewsTopic>("gaming");
  const address = url.trim();
  const canAdd = isFeedUrl(address) && feeds.length < MAX_CUSTOM_FEEDS && !feeds.some((feed) => feed.url === address);
  const addFeed = () => {
    if (!canAdd) return;
    onFeedsChange([...feeds, { name: name.trim(), url: address, topic }]);
    setName("");
    setUrl("");
  };
  /** Enter in either field adds the feed; it never saves the whole dialog from here. */
  const addOnEnter = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      addFeed();
    }
  };
  useEffect(() => {
    if (active && all === null) {
      newsSources()
        .then(setAll)
        .catch(() => setAll([]));
    }
  }, [active, all]);

  if (all === null) return <p className="text-2xs text-zinc-500">{t("news.loading")}</p>;
  const chosen = value ?? defaultNewsSources(all, language);
  // The user's own language first, then the others as Kanso lists them
  const languages = [...new Set(all.map((source) => source.lang))].sort((a, b) => Number(b === language) - Number(a === language));
  const toggle = (id: string, on: boolean) => onChange(all.map((source) => source.id).filter((entry) => (entry === id ? on : chosen.includes(entry))));

  return (
    <>
    <section id="settings-news-sources" className="space-y-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <span className="block text-xs font-medium text-zinc-200">{t("settings.newsSources")}</span>
          <p className="text-2xs leading-relaxed text-zinc-500">{t("settings.newsSourcesHelp")}</p>
        </div>
        <button
          id="news-defaults-button"
          type="button"
          disabled={value === null}
          onClick={() => onChange(null)}
          className="shrink-0 rounded-lg border border-oled-border bg-oled-surface px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-50"
        >
          {t("settings.newsDefaults")}
        </button>
      </div>
      {languages.map((code) => (
        <div key={code} data-news-language={code} className="space-y-1.5">
          <span className="micro-label">{isLanguageCode(code) ? LANGUAGE_NAMES[code] : code}</span>
          <div className="grid gap-1.5 sm:grid-cols-2">
            {all
              .filter((source) => source.lang === code)
              .map((source) => (
                <div key={source.id} className="flex items-center justify-between gap-4 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2">
                  <div className="min-w-0">
                    <label htmlFor={`news-source-${source.id}`} className="block truncate text-xs font-medium text-zinc-200">
                      {source.name}
                    </label>
                    <p className="truncate text-2xs text-zinc-500">
                      {t(`news.${source.topic}`)} · {source.siteUrl.replace(/^https:\/\/(www\.)?/, "").replace(/\/$/, "")}
                    </p>
                  </div>
                  <input
                    id={`news-source-${source.id}`}
                    type="checkbox"
                    checked={chosen.includes(source.id)}
                    onChange={(e) => toggle(source.id, e.target.checked)}
                    className="h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
                  />
                </div>
              ))}
          </div>
        </div>
      ))}
    </section>

    {/* A site Kanso does not list: its feed, added by hand */}
    <section id="settings-news-feeds" className="space-y-2">
      <div>
        <span className="block text-xs font-medium text-zinc-200">{t("settings.newsAdd")}</span>
        <p className="text-2xs leading-relaxed text-zinc-500">{t("settings.newsAddHelp")}</p>
      </div>
      {feeds.length > 0 && (
        <ul id="news-feeds" className="space-y-1.5">
          {feeds.map((feed) => (
            <li key={feed.url} className="flex items-center justify-between gap-4 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2">
              <div className="min-w-0">
                <span className="block truncate text-xs font-medium text-zinc-200">{feed.name || feed.url}</span>
                <p className="truncate text-2xs text-zinc-500">
                  {t(`news.${feed.topic}`)} · {feed.url}
                </p>
              </div>
              <button
                type="button"
                data-remove-feed={feed.url}
                onClick={() => onFeedsChange(feeds.filter((entry) => entry.url !== feed.url))}
                aria-label={t("settings.newsFeedRemove", { name: feed.name || feed.url })}
                title={t("settings.newsFeedRemove", { name: feed.name || feed.url })}
                className="shrink-0 rounded-lg border border-oled-border bg-oled-surface p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-100"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,2fr)_auto_auto]">
        <input
          id="news-feed-name"
          type="text"
          value={name}
          maxLength={40}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={addOnEnter}
          placeholder={t("settings.newsFeedName")}
          aria-label={t("settings.newsFeedName")}
          className="rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-accent/60 focus:outline-none"
        />
        <input
          id="news-feed-url"
          // Plain text: the dialog's own note says what is wrong with an address, and a
          // browser "invalid field" here would silently block saving the whole dialog
          type="text"
          inputMode="url"
          autoComplete="off"
          spellCheck={false}
          value={url}
          maxLength={300}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={addOnEnter}
          placeholder={t("settings.newsFeedUrl")}
          aria-label={t("settings.newsFeedUrl")}
          aria-invalid={address !== "" && !isFeedUrl(address)}
          className="rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-accent/60 focus:outline-none"
        />
        <select
          id="news-feed-topic"
          value={topic}
          onChange={(e) => setTopic(e.target.value as NewsTopic)}
          aria-label={t("news.topics")}
          className="rounded-lg border border-oled-border bg-oled-bg px-2.5 py-1.5 text-xs text-zinc-200 focus:border-accent/60 focus:outline-none"
        >
          <option value="gaming">{t("news.gaming")}</option>
          <option value="hardware">{t("news.hardware")}</option>
        </select>
        <button
          id="news-feed-add-button"
          type="button"
          disabled={!canAdd}
          onClick={addFeed}
          className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-oled-border bg-oled-surface px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Plus className="h-3.5 w-3.5" />
          <span>{t("common.add")}</span>
        </button>
      </div>
      {address !== "" && !isFeedUrl(address) && (
        <p id="news-feed-invalid" className="text-2xs text-amber-200">
          {t("settings.newsFeedInvalid")}
        </p>
      )}
    </section>
    </>
  );
}
