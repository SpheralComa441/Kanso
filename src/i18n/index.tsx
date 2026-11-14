import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  type ReactNode,
} from "react";
import { en } from "./en";
import { es } from "./es";
import { it, type Dictionary, type MessageKey } from "./it";
import { ru } from "./ru";

export type { MessageKey } from "./it";

export const LANGUAGES = ["it", "en", "es", "ru"] as const;
export type LanguageCode = (typeof LANGUAGES)[number];

/** Each language's own name, shown untranslated in the language picker. */
export const LANGUAGE_NAMES: Record<LanguageCode, string> = {
  it: "Italiano",
  en: "English",
  es: "Español",
  ru: "Русский",
};

const DICTIONARIES: Record<LanguageCode, Dictionary> = { it, en, es, ru };

/** Plural families: `games` -> `games.one` / `.few` / `.many` / `.other`. */
type PluralFamily = {
  [K in MessageKey]: K extends `${infer F}.one` ? F : never;
}[MessageKey];

export function isLanguageCode(value: unknown): value is LanguageCode {
  return LANGUAGES.includes(value as LanguageCode);
}

/**
 * Language to start with when none is saved: the first OS/browser language Kanso speaks,
 * English otherwise.
 */
export function detectLanguage(): LanguageCode {
  const preferred =
    typeof navigator === "undefined"
      ? []
      : [...(navigator.languages ?? []), navigator.language];
  for (const tag of preferred) {
    const primary = tag?.toLowerCase().split(/[-_]/)[0];
    if (isLanguageCode(primary)) return primary;
  }
  return "en";
}

/**
 * Two-letter country (for regional store prices) from the OS/browser locale, e.g. `it-IT` ->
 * `IT`. Falls back to a country that matches the interface language.
 */
export function detectCountry(lang: LanguageCode): string {
  const preferred =
    typeof navigator === "undefined"
      ? []
      : [...(navigator.languages ?? []), navigator.language];
  for (const tag of preferred) {
    const region = tag?.split(/[-_]/)[1];
    if (region && /^[a-zA-Z]{2}$/.test(region)) return region.toUpperCase();
  }
  return { it: "IT", en: "US", es: "ES", ru: "RU" }[lang];
}

export interface I18n {
  lang: LanguageCode;
  /** Translated text for `key`, with `{name}` placeholders replaced from `params`. */
  t: (key: MessageKey, params?: Record<string, string | number>) => string;
  /** Correctly pluralised count, e.g. `plural("games", 3)`. */
  plural: (family: PluralFamily, count: number) => string;
  /** "3 hours ago", "ieri", "2 месяца назад"... for a past timestamp in milliseconds. */
  relativePast: (timestampMs: number) => string;
  /** Short day and month, e.g. "19 nov". */
  shortDate: (timestampMs: number) => string;
  /** Localised price, e.g. "26,65 €". */
  money: (amount: number, currency: string) => string;
}

const RELATIVE_STEPS: [number, Intl.RelativeTimeFormatUnit, number][] = [
  [3600, "minute", 60],
  [86400, "hour", 3600],
  [86400 * 30, "day", 86400],
  [86400 * 365, "month", 86400 * 30],
  [Infinity, "year", 86400 * 365],
];

export function createI18n(lang: LanguageCode): I18n {
  const dict = DICTIONARIES[lang];
  const pluralRules = new Intl.PluralRules(lang);
  const relative = new Intl.RelativeTimeFormat(lang, { numeric: "auto" });
  const dayMonth = new Intl.DateTimeFormat(lang, { day: "numeric", month: "short" });

  const t: I18n["t"] = (key, params) => {
    const template = dict[key] ?? it[key] ?? key;
    if (!params) return template;
    return template.replace(/\{(\w+)\}/g, (match, name: string) =>
      name in params ? String(params[name]) : match
    );
  };

  return {
    lang,
    t,
    plural: (family, count) => {
      const category = pluralRules.select(count);
      const key = `${family}.${category}` as MessageKey;
      return t(key in dict ? key : (`${family}.other` as MessageKey), { count });
    },
    relativePast: (timestampMs) => {
      const elapsed = Math.max(0, Math.round((Date.now() - timestampMs) / 1000));
      if (elapsed < 60) return t("time.justNow");
      for (const [limit, unit, size] of RELATIVE_STEPS) {
        if (elapsed < limit) return relative.format(-Math.floor(elapsed / size), unit);
      }
      return "";
    },
    shortDate: (timestampMs) => dayMonth.format(new Date(timestampMs)),
    money: (amount, currency) => {
      try {
        return new Intl.NumberFormat(lang, { style: "currency", currency }).format(amount);
      } catch {
        // Unknown currency code from a store API
        return `${amount.toFixed(2)} ${currency}`;
      }
    },
  };
}

const I18nContext = createContext<I18n>(createI18n("it"));

export function I18nProvider({
  lang,
  children,
}: {
  lang: LanguageCode;
  children: ReactNode;
}) {
  const value = useMemo(() => createI18n(lang), [lang]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  return useContext(I18nContext);
}

/** Stable `t` for callbacks that should not re-run when unrelated state changes. */
export function useT(): I18n["t"] {
  const { t } = useI18n();
  return useCallback(t, [t]);
}
