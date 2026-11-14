import { useEffect, useState } from "react";
import { Check, ChevronDown, X } from "lucide-react";
import { useI18n, type MessageKey } from "../i18n";
import {
  BUNDLED_FONTS,
  THEME_COLOR_TOKENS,
  fontStackNames,
  isBuiltinTheme,
  toColorInputValue,
} from "../lib/themes";
import type { Theme, ThemeColors } from "../types/game";

interface ThemeEditorProps {
  /** Theme to start from; `null` keeps the editor closed. */
  baseTheme: Theme | null;
  /** Called on every change so the app behind the panel shows the draft live. */
  onPreview: (draft: Theme) => void;
  /** Persists the draft. `replaceId` overwrites that installed theme, `null` creates one. */
  onSave: (draft: Theme, replaceId: string | null) => Promise<void>;
  onClose: () => void;
}

interface FontPickerProps {
  id: string;
  label: string;
  /** The theme's font stack; `null` is the default. */
  value: string | null;
  defaultLabel: string;
  /** Fonts the theme carries inside its file. */
  themeFonts: string[];
  onChange: (value: string | null) => void;
}

/**
 * Chooses a font among the ones bundled with Kanso (so the theme looks the same on every
 * PC), the ones embedded in the theme, or any font installed on this PC by name.
 */
function FontPicker({ id, label, value, defaultLabel, themeFonts, onChange }: FontPickerProps) {
  const { t } = useI18n();
  const [isOpen, setIsOpen] = useState(false);

  const current = fontStackNames(value)[0] ?? null;
  const bundled = BUNDLED_FONTS.map((font) => font.family);
  const listed = [...new Set([...themeFonts, ...bundled])];
  const isListed = current !== null && listed.some((name) => name === current);
  const customValue = current !== null && !isListed ? (value ?? "") : "";

  const optionClass = (selected: boolean) =>
    `flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors ${
      selected
        ? "bg-oled-elevated text-zinc-100"
        : "text-zinc-300 hover:bg-oled-elevated/60"
    }`;

  return (
    <div className="space-y-1">
      <span id={`${id}-label`} className="block text-2xs font-medium text-zinc-300">
        {label}
      </span>
      <button
        id={id}
        type="button"
        aria-expanded={isOpen}
        aria-labelledby={`${id}-label ${id}`}
        onClick={() => setIsOpen((open) => !open)}
        className="flex w-full items-center justify-between gap-2 rounded-lg border border-oled-border bg-oled-bg px-2.5 py-1.5 text-left text-sm text-zinc-100 transition-colors hover:border-zinc-600"
      >
        <span
          className="min-w-0 truncate"
          style={current ? { fontFamily: `"${current}"` } : undefined}
        >
          {current ?? defaultLabel}
        </span>
        <ChevronDown
          className={`h-3.5 w-3.5 shrink-0 text-zinc-500 transition-transform ${
            isOpen ? "rotate-180" : ""
          }`}
        />
      </button>

      {isOpen && (
        <div className="space-y-1.5 rounded-lg border border-oled-border bg-oled-bg p-1.5">
          <ul className="scroll-area max-h-56 space-y-0.5 overflow-y-auto">
            <li>
              <button
                id={`${id}-option-default`}
                type="button"
                aria-pressed={current === null}
                onClick={() => onChange(null)}
                className={optionClass(current === null)}
              >
                <span>{defaultLabel}</span>
                {current === null && <Check className="h-3.5 w-3.5 text-accent" />}
              </button>
            </li>
            {listed.map((family) => {
              const selected = current === family;
              return (
                <li key={family}>
                  <button
                    id={`${id}-option-${family.toLowerCase().replace(/\s+/g, "-")}`}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => onChange(family)}
                    className={optionClass(selected)}
                  >
                    {/* Each name in its own typeface */}
                    <span className="min-w-0 truncate" style={{ fontFamily: `"${family}"` }}>
                      {family}
                    </span>
                    <span className="flex shrink-0 items-center gap-1.5">
                      {themeFonts.includes(family) && (
                        <span className="rounded-full border border-oled-border px-1.5 text-3xs text-zinc-500">
                          {t("theme.fontFromTheme")}
                        </span>
                      )}
                      {selected && <Check className="h-3.5 w-3.5 text-accent" />}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
          <label className="block space-y-1 px-1 pb-1">
            <span className="block text-2xs text-zinc-500">{t("theme.fontCustom")}</span>
            <input
              id={`${id}-custom`}
              type="text"
              maxLength={200}
              value={customValue}
              placeholder={t("theme.fontCustomPlaceholder")}
              onChange={(e) =>
                // Same character set the backend accepts: plain font names, no CSS
                onChange(e.target.value.replace(/[^\p{L}\p{N} ,'"_-]/gu, "") || null)
              }
              className="w-full rounded-md border border-oled-border bg-oled-surface px-2 py-1 text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
            />
          </label>
        </div>
      )}
    </div>
  );
}

/**
 * Side panel for recoloring a theme and choosing its fonts without touching JSON: every
 * change is applied to the running app immediately, and nothing is stored until one of the
 * save buttons is pressed.
 *
 * Shapes, sizes and effects are deliberately not editable here: they belong to theme files
 * (`docs/THEMES.md`). Whatever the base theme defines is kept as it is.
 */
export function ThemeEditor({ baseTheme, onPreview, onSave, onClose }: ThemeEditorProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<Theme | null>(baseTheme);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const baseId = baseTheme?.id ?? null;
  useEffect(() => {
    if (!baseTheme) {
      setDraft(null);
      return;
    }
    // A built-in theme cannot be overwritten: start a copy under a new name
    setDraft(
      isBuiltinTheme(baseTheme)
        ? { ...baseTheme, name: t("theme.copyOf", { name: baseTheme.name }), author: "" }
        : baseTheme
    );
    setError(null);
    // Re-initialise only when another theme is opened, not on every re-render
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [baseId]);

  useEffect(() => {
    if (!baseId) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [baseId, onClose]);

  if (!baseTheme || !draft) return null;

  const update = (next: Theme) => {
    setDraft(next);
    onPreview(next);
  };
  const setColor = (key: keyof ThemeColors, value: string) =>
    update({ ...draft, colors: { ...draft.colors, [key]: value } });

  const canOverwrite = !isBuiltinTheme(baseTheme);
  const themeFonts = [...new Set(draft.fontFiles.map((file) => file.family))];

  const save = async (replaceId: string | null) => {
    if (!draft.name.trim()) {
      setError(t("theme.nameRequired"));
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      await onSave({ ...draft, name: draft.name.trim() }, replaceId);
    } catch (err) {
      setError(typeof err === "string" ? err : t("theme.error"));
    } finally {
      setIsSaving(false);
    }
  };

  const fieldClass =
    "w-full rounded-lg border border-oled-border bg-oled-bg px-2.5 py-1.5 text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none";
  const labelClass = "block text-2xs font-medium text-zinc-300";
  const legendClass =
    "pb-1 text-3xs font-semibold uppercase tracking-[0.12em] text-zinc-500";

  return (
    <aside
      role="dialog"
      aria-labelledby="theme-editor-title"
      className="below-header fixed bottom-0 right-0 z-50 flex w-[21rem] max-w-full flex-col border-l border-oled-border bg-oled-surface shadow-2xl"
    >
      <div className="flex shrink-0 items-start justify-between gap-3 border-b border-oled-border px-5 py-4">
        <div>
          <h2 id="theme-editor-title" className="text-sm font-semibold text-zinc-100">
            {t("theme.editorTitle")}
          </h2>
          <p className="mt-0.5 text-2xs leading-snug text-zinc-400">
            {t("theme.editorHint")}
          </p>
        </div>
        <button
          id="close-theme-editor-button"
          type="button"
          onClick={onClose}
          aria-label={t("common.close")}
          className="rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-100"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="scroll-area min-h-0 flex-1 space-y-5 overflow-y-auto px-5 py-4">
        {error && (
          <div
            role="alert"
            className="select-text rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-200"
          >
            {error}
          </div>
        )}

        <div className="space-y-3">
          <div className="space-y-1">
            <label htmlFor="theme-name-input" className={labelClass}>
              {t("theme.name")}
            </label>
            <input
              id="theme-name-input"
              type="text"
              maxLength={60}
              value={draft.name}
              onChange={(e) => update({ ...draft, name: e.target.value })}
              className={fieldClass}
            />
          </div>
          <div className="space-y-1">
            <label htmlFor="theme-author-input" className={labelClass}>
              {t("theme.author")}
            </label>
            <input
              id="theme-author-input"
              type="text"
              maxLength={60}
              value={draft.author}
              onChange={(e) => update({ ...draft, author: e.target.value })}
              className={fieldClass}
            />
          </div>
          <div className="space-y-1">
            <label htmlFor="theme-description-input" className={labelClass}>
              {t("theme.description")}
            </label>
            <input
              id="theme-description-input"
              type="text"
              maxLength={240}
              value={draft.description}
              onChange={(e) => update({ ...draft, description: e.target.value })}
              className={fieldClass}
            />
          </div>

          {/* Tells Kanso which built-in tints (badges, alerts) suit the theme's surfaces */}
          <div className="space-y-1">
            <span className={labelClass}>{t("theme.scheme")}</span>
            <div
              role="group"
              aria-label={t("theme.scheme")}
              className="flex rounded-lg border border-oled-border bg-oled-bg p-0.5"
            >
              {(["dark", "light"] as const).map((scheme) => (
                <button
                  key={scheme}
                  id={`theme-scheme-${scheme}`}
                  type="button"
                  onClick={() => update({ ...draft, colorScheme: scheme })}
                  aria-pressed={draft.colorScheme === scheme}
                  className={`flex-1 rounded-md px-2 py-1 text-xs font-medium transition-colors ${
                    draft.colorScheme === scheme
                      ? "bg-oled-elevated text-zinc-100"
                      : "text-zinc-400 hover:text-zinc-200"
                  }`}
                >
                  {t(scheme === "dark" ? "theme.schemeDark" : "theme.schemeLight")}
                </button>
              ))}
            </div>
          </div>
        </div>

        <fieldset className="space-y-2">
          <legend className={legendClass}>{t("theme.colors")}</legend>
          {THEME_COLOR_TOKENS.map(([key]) => (
            <label
              key={key}
              className="flex items-center justify-between gap-3 text-xs text-zinc-300"
            >
              <span className="min-w-0 truncate">
                {t(`theme.color.${key}` as MessageKey)}
              </span>
              <span className="flex shrink-0 items-center gap-2">
                <span className="font-mono text-3xs text-zinc-500">
                  {toColorInputValue(draft.colors[key])}
                </span>
                <input
                  id={`theme-color-${key}`}
                  type="color"
                  value={toColorInputValue(draft.colors[key])}
                  onChange={(e) => setColor(key, e.target.value)}
                  className="h-6 w-9 cursor-pointer rounded border border-oled-border bg-transparent p-0"
                />
              </span>
            </label>
          ))}
        </fieldset>

        <fieldset className="space-y-3">
          <legend className={legendClass}>{t("theme.fonts")}</legend>
          <FontPicker
            id="theme-font-body"
            label={t("theme.fontBody")}
            value={draft.fontFamily}
            defaultLabel={t("theme.fontDefault")}
            themeFonts={themeFonts}
            onChange={(fontFamily) => update({ ...draft, fontFamily })}
          />
          <FontPicker
            id="theme-font-heading"
            label={t("theme.fontHeading")}
            value={draft.typography.headingFont}
            defaultLabel={t("theme.fontSameAsBody")}
            themeFonts={themeFonts}
            onChange={(headingFont) =>
              update({ ...draft, typography: { ...draft.typography, headingFont } })
            }
          />
        </fieldset>

        <p className="text-2xs leading-relaxed text-zinc-500">{t("theme.editorScope")}</p>
      </div>

      <div className="flex shrink-0 flex-col gap-2 border-t border-oled-border px-5 py-4">
        {canOverwrite && (
          <button
            id="theme-save-changes-button"
            type="button"
            disabled={isSaving}
            onClick={() => void save(baseTheme.id)}
            className="inline-flex items-center justify-center gap-1.5 rounded-lg bg-accent px-4 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:opacity-60"
          >
            <Check className="h-3.5 w-3.5" />
            <span>{t("theme.saveChanges")}</span>
          </button>
        )}
        <button
          id="theme-save-new-button"
          type="button"
          disabled={isSaving}
          onClick={() => void save(null)}
          className={
            canOverwrite
              ? "rounded-lg border border-oled-border bg-oled-bg px-4 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-600 disabled:opacity-60"
              : "inline-flex items-center justify-center gap-1.5 rounded-lg bg-accent px-4 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:opacity-60"
          }
        >
          {t("theme.saveAsNew")}
        </button>
      </div>
    </aside>
  );
}
