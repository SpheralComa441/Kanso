import { useEffect, useState } from "react";
import { LANGUAGE_NAMES, isLanguageCode, useI18n } from "../i18n";
import { newsSources } from "../lib/backend";
import type { NewsSourceInfo } from "../types/game";

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
}

/** Which sites the News page reads: every source Kanso knows, by language, each with a switch. */
export function NewsSettings({ active, value, onChange, language }: NewsSettingsProps) {
  const { t } = useI18n();
  const [all, setAll] = useState<NewsSourceInfo[] | null>(null);
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
  );
}
