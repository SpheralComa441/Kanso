import { useEffect, useState } from "react";
import {
  Accessibility,
  Check,
  Download,
  Eye,
  EyeOff,
  FolderOpen,
  FolderPlus,
  KeyRound,
  Languages,
  Palette,
  Plus,
  Puzzle,
  SlidersHorizontal,
  Trash2,
  Upload,
  UserRound,
  X,
} from "lucide-react";
import {
  LANGUAGES,
  LANGUAGE_NAMES,
  isLanguageCode,
  useI18n,
  type LanguageCode,
} from "../i18n";
import {
  deleteTheme,
  exportThemeToFile,
  importThemeFromFile,
  isRunningInTauri,
  openThemesFolder,
  pickCustomFolder,
} from "../lib/backend";
import {
  BUILTIN_THEMES,
  DEFAULT_THEME,
  READABLE_FONTS,
  TEXT_SCALE_OPTIONS,
  isBuiltinTheme,
  resolveTheme,
  ANIMATION_CHOICES,
} from "../lib/themes";
import type {
  AccessibilityConfig,
  AnimationsChoice,
  AppConfig,
  PluginInfo,
  PluginState,
  Theme,
} from "../types/game";
import { CARD_INFO_CHOICES } from "../lib/cardInfo";
import { EmulatorsSettings } from "./EmulatorsSettings";
import { PluginsSettings } from "./PluginsSettings";
import { StoreAccountsSettings } from "./StoreAccountsSettings";

interface SettingsModalProps {
  isOpen: boolean;
  initialConfig: AppConfig;
  /** Language currently shown (the saved one, or the one detected on first run). */
  currentLanguage: LanguageCode;
  userThemes: Theme[];
  onClose: () => void;
  onSave: (updated: AppConfig) => Promise<void>;
  /** Re-reads the installed themes after an import or a delete. */
  onThemesChanged: () => Promise<Theme[]>;
  /** Shows a theme right away without saving it; `null` goes back to the saved one. */
  onPreviewTheme: (theme: Theme | null) => void;
  /** Same for accessibility choices: tried at once, kept only when saved. */
  onPreviewAccessibility: (accessibility: AccessibilityConfig | null) => void;
  /** Shows the app with this choice for the animations while the dialog is open. */
  onPreviewAnimations: (animations: AnimationsChoice | null) => void;
  onCustomizeTheme: (theme: Theme) => void;
  plugins: PluginInfo[];
  pluginWarnings: string[];
  /** A plugin was installed or removed: adopt the settings the backend saved. */
  onPluginsChanged: (saved: AppConfig) => Promise<void>;
  /** Someone signed in to a store: its games can be fetched. */
  onStoreSignedIn: () => void;
}

const SKIPPED_EXE_PATTERNS = "unins, crash, redist, dxsetup, vc_redist, launcher";

export function SettingsModal({
  isOpen,
  initialConfig,
  currentLanguage,
  userThemes,
  onClose,
  onSave,
  onThemesChanged,
  onPreviewTheme,
  onPreviewAccessibility,
  onPreviewAnimations,
  onCustomizeTheme,
  plugins,
  pluginWarnings,
  onPluginsChanged,
  onStoreSignedIn,
}: SettingsModalProps) {
  const { t } = useI18n();
  const [apiKey, setApiKey] = useState(initialConfig.steamgriddbApiKey);
  const [showKey, setShowKey] = useState(false);
  const [folders, setFolders] = useState<string[]>(initialConfig.customFolders);
  const [manualFolderInput, setManualFolderInput] = useState("");
  const [minimizeOnLaunch, setMinimizeOnLaunch] = useState(
    initialConfig.minimizeToTrayOnLaunch
  );
  const [language, setLanguage] = useState<LanguageCode>(currentLanguage);
  const [themeId, setThemeId] = useState<string | null>(initialConfig.themeId);
  const [accessibility, setAccessibility] = useState<AccessibilityConfig>(
    initialConfig.accessibility
  );
  const [trackPlaytime, setTrackPlaytime] = useState(initialConfig.trackPlaytime);
  const [animations, setAnimations] = useState(initialConfig.animations);
  const [cardInfo, setCardInfo] = useState(initialConfig.cardInfo);
  const [emulators, setEmulators] = useState(initialConfig.emulators);
  const [hasGamePass, setHasGamePass] = useState(initialConfig.hasGamePass);
  const [gamePassCatalog, setGamePassCatalog] = useState(initialConfig.gamePassCatalog);
  const [pluginStates, setPluginStates] = useState<Record<string, PluginState>>(
    initialConfig.plugins
  );
  const [themeNotice, setThemeNotice] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      setApiKey(initialConfig.steamgriddbApiKey);
      setFolders(initialConfig.customFolders);
      setMinimizeOnLaunch(initialConfig.minimizeToTrayOnLaunch);
      setLanguage(currentLanguage);
      setThemeId(initialConfig.themeId);
      setAccessibility(initialConfig.accessibility);
      setTrackPlaytime(initialConfig.trackPlaytime);
      setAnimations(initialConfig.animations);
      setCardInfo(initialConfig.cardInfo);
      setEmulators(initialConfig.emulators);
      setHasGamePass(initialConfig.hasGamePass);
      setGamePassCatalog(initialConfig.gamePassCatalog);
      setPluginStates(initialConfig.plugins);
      setManualFolderInput("");
      setShowKey(false);
      setErrorMsg(null);
      setThemeNotice(null);
    }
    // Only when the dialog opens: edits in progress must survive background updates
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  const closeWithoutSaving = () => {
    onPreviewTheme(null);
    onPreviewAccessibility(null);
    onPreviewAnimations(null);
    onClose();
  };

  const changeAccessibility = (change: Partial<AccessibilityConfig>) => {
    const next = { ...accessibility, ...change };
    setAccessibility(next);
    onPreviewAccessibility(next);
  };

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeWithoutSaving();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  if (!isOpen) return null;

  const allThemes = [...BUILTIN_THEMES, ...userThemes];
  const selectedTheme = resolveTheme(themeId, userThemes);

  const selectTheme = (theme: Theme) => {
    setThemeId(theme.id);
    onPreviewTheme(theme);
  };

  const reportThemeError = (err: unknown) =>
    setThemeNotice({
      type: "error",
      text: typeof err === "string" ? err : t("theme.error"),
    });

  const handleImportTheme = async () => {
    setThemeNotice(null);
    if (!isRunningInTauri()) {
      setThemeNotice({ type: "error", text: t("theme.previewOnly") });
      return;
    }
    try {
      const imported = await importThemeFromFile();
      if (!imported) return;
      await onThemesChanged();
      selectTheme(imported);
      setThemeNotice({
        type: "success",
        text: t("theme.imported", { name: imported.name }),
      });
    } catch (err) {
      reportThemeError(err);
    }
  };

  const handleExportTheme = async () => {
    setThemeNotice(null);
    if (!isRunningInTauri()) {
      setThemeNotice({ type: "error", text: t("theme.previewOnly") });
      return;
    }
    try {
      if (await exportThemeToFile(selectedTheme)) {
        setThemeNotice({ type: "success", text: t("theme.exported") });
      }
    } catch (err) {
      reportThemeError(err);
    }
  };

  const handleDeleteTheme = async (theme: Theme) => {
    setThemeNotice(null);
    try {
      await deleteTheme(theme.id);
      await onThemesChanged();
      if (themeId === theme.id) selectTheme(DEFAULT_THEME);
      setThemeNotice({
        type: "success",
        text: t("theme.deleted", { name: theme.name }),
      });
    } catch (err) {
      reportThemeError(err);
    }
  };

  const addFolderPath = (rawPath: string) => {
    const trimmed = rawPath.trim();
    if (!trimmed) return;
    const norm = (f: string) =>
      f.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
    if (!folders.some((f) => norm(f) === norm(trimmed))) {
      setFolders((prev) => [...prev, trimmed]);
    }
  };

  const handleBrowseFolder = async () => {
    try {
      const picked = await pickCustomFolder();
      if (picked) addFolderPath(picked);
    } catch (err) {
      setErrorMsg(
        typeof err === "string" ? err : t("settings.folderPickerError")
      );
    }
  };

  const handleAddManualFolder = () => {
    if (!manualFolderInput.trim()) return;
    addFolderPath(manualFolderInput);
    setManualFolderInput("");
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSaving(true);
    setErrorMsg(null);
    try {
      await onSave({
        steamgriddbApiKey: apiKey.trim(),
        customFolders: folders,
        minimizeToTrayOnLaunch: minimizeOnLaunch,
        language,
        themeId: selectedTheme.id,
        accessibility,
        trackPlaytime,
        plugins: pluginStates,
        animations,
        // Kept in Kanso's order, whatever order they were ticked in
        cardInfo: CARD_INFO_CHOICES.filter((item) => cardInfo.includes(item)),
        // An entry that says nothing (no executable, folder or argument) is not kept
        emulators: emulators.filter(
          (emulator) => emulator.exePath.trim() || emulator.folders.length > 0 || emulator.args.trim()
        ),
        hasGamePass,
        gamePassCatalog,
      });
      onPreviewTheme(null);
      onPreviewAccessibility(null);
      onPreviewAnimations(null);
      onClose();
    } catch (err) {
      setErrorMsg(typeof err === "string" ? err : t("settings.saveError"));
    } finally {
      setIsSaving(false);
    }
  };

  const keyConfigured = apiKey.trim().length > 0;
  const smallButton =
    "inline-flex items-center gap-1.5 rounded-md border border-oled-border bg-oled-elevated px-2.5 py-1 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-600";

  return (
    <div
      className="below-header fixed inset-x-0 bottom-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm motion-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeWithoutSaving();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-modal-title"
        className="motion-sheet flex max-h-full w-full max-w-xl flex-col overflow-hidden rounded-xl border border-oled-border bg-oled-surface shadow-2xl"
      >
        {/* Header */}
        <div className="flex shrink-0 items-center justify-between border-b border-oled-border px-6 py-4">
          <div>
            <h2
              id="settings-modal-title"
              className="text-base font-semibold text-zinc-100"
            >
              {t("settings.title")}
            </h2>
            <p className="mt-0.5 text-xs text-zinc-400">{t("settings.subtitle")}</p>
          </div>
          <button
            id="close-settings-button"
            type="button"
            onClick={closeWithoutSaving}
            aria-label={t("settings.close")}
            className="rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-200"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <form
          id="settings-form"
          onSubmit={handleSubmit}
          className="min-h-0 flex-1 space-y-6 scroll-area overflow-y-auto px-6 py-5"
        >
          {errorMsg && (
            <div className="select-text rounded-lg border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-xs text-red-200">
              {errorMsg}
            </div>
          )}

          {/* Language */}
          <section className="space-y-2">
            <label
              htmlFor="language-select"
              className="flex items-center gap-1.5 text-xs font-medium text-zinc-200"
            >
              <Languages className="h-3.5 w-3.5 text-accent" />
              <span>{t("settings.language")}</span>
            </label>
            <select
              id="language-select"
              value={language}
              onChange={(e) => {
                if (isLanguageCode(e.target.value)) setLanguage(e.target.value);
              }}
              className="w-full rounded-lg border border-oled-border bg-oled-bg px-3 py-2 text-xs text-zinc-100 focus:border-accent/60 focus:outline-none"
            >
              {LANGUAGES.map((code) => (
                <option key={code} value={code}>
                  {LANGUAGE_NAMES[code]}
                </option>
              ))}
            </select>
            <p className="text-2xs text-zinc-500">{t("settings.languageHelp")}</p>
          </section>

          {/* Theme */}
          <section className="space-y-2.5">
            <div className="flex items-center gap-1.5 text-xs font-medium text-zinc-200">
              <Palette className="h-3.5 w-3.5 text-accent" />
              <span>{t("theme.section")}</span>
            </div>

            <ul className="grid grid-cols-2 gap-2">
              {allThemes.map((theme) => {
                const selected = theme.id === selectedTheme.id;
                const builtIn = isBuiltinTheme(theme);
                return (
                  <li key={theme.id} className="relative">
                    <button
                      id={`theme-option-${theme.id}`}
                      type="button"
                      onClick={() => selectTheme(theme)}
                      aria-pressed={selected}
                      className={`flex w-full items-center gap-2.5 rounded-lg border px-2.5 py-2 text-left transition-colors ${
                        selected
                          ? "border-accent/70 bg-oled-elevated"
                          : "border-oled-border bg-oled-bg hover:border-zinc-600"
                      }`}
                    >
                      {/* The theme's own colors, whatever theme is active */}
                      <span
                        aria-hidden="true"
                        className="flex h-8 w-8 shrink-0 items-end justify-end overflow-hidden rounded-md border"
                        style={{
                          background: theme.colors.background,
                          borderColor: theme.colors.border,
                        }}
                      >
                        <span
                          className="h-4 w-4 rounded-tl-md"
                          style={{ background: theme.colors.accent }}
                        />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-xs font-medium text-zinc-100">
                          {theme.name}
                        </span>
                        <span className="block truncate text-3xs text-zinc-500">
                          {builtIn
                            ? t("theme.builtin")
                            : theme.author
                              ? t("theme.by", { author: theme.author })
                              : theme.description}
                        </span>
                      </span>
                    </button>
                    {!builtIn && (
                      <button
                        type="button"
                        onClick={() => void handleDeleteTheme(theme)}
                        aria-label={t("theme.deleteNamed", { name: theme.name })}
                        title={t("common.delete")}
                        className="absolute right-1.5 top-1.5 rounded p-1 text-zinc-500 transition-colors hover:bg-red-500/10 hover:text-red-300"
                      >
                        <Trash2 className="h-3 w-3" />
                      </button>
                    )}
                  </li>
                );
              })}
            </ul>

            <div className="flex flex-wrap gap-2">
              <button
                id="customize-theme-button"
                type="button"
                onClick={() => onCustomizeTheme(selectedTheme)}
                className={smallButton}
              >
                <SlidersHorizontal className="h-3.5 w-3.5 text-accent" />
                <span>{t("theme.customize")}</span>
              </button>
              <button
                id="import-theme-button"
                type="button"
                onClick={() => void handleImportTheme()}
                className={smallButton}
              >
                <Download className="h-3.5 w-3.5 text-accent" />
                <span>{t("theme.import")}</span>
              </button>
              <button
                id="export-theme-button"
                type="button"
                onClick={() => void handleExportTheme()}
                className={smallButton}
              >
                <Upload className="h-3.5 w-3.5 text-accent" />
                <span>{t("theme.export")}</span>
              </button>
              {isRunningInTauri() && (
                <button
                  id="open-themes-folder-button"
                  type="button"
                  onClick={() => void openThemesFolder().catch(reportThemeError)}
                  className={smallButton}
                >
                  <FolderOpen className="h-3.5 w-3.5 text-accent" />
                  <span>{t("theme.openFolder")}</span>
                </button>
              )}
            </div>

            {themeNotice && (
              <p
                role="status"
                className={`select-text text-2xs ${
                  themeNotice.type === "success"
                    ? "text-emerald-300"
                    : "text-red-300"
                }`}
              >
                {themeNotice.text}
              </p>
            )}
            <p className="text-2xs leading-relaxed text-zinc-500">
              {t("theme.help")}
            </p>
          </section>

          {/* Animations: the theme says how things move, the user how much */}
          <section id="settings-animations" className="space-y-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span id="animations-label" className="text-xs font-medium text-zinc-200">
                {t("motion.title")}
              </span>
              <div
                role="group"
                aria-labelledby="animations-label"
                className="flex items-center gap-0.5 rounded-lg border border-oled-border bg-oled-bg p-0.5"
              >
                {ANIMATION_CHOICES.map((choice) => (
                  <button
                    key={choice}
                    id={`animations-${choice}`}
                    type="button"
                    onClick={() => {
                      setAnimations(choice);
                      onPreviewAnimations(choice);
                    }}
                    aria-pressed={animations === choice}
                    className={`rounded-md px-2.5 py-1 text-xs font-medium transition-colors ${
                      animations === choice
                        ? "bg-oled-elevated text-zinc-100"
                        : "text-zinc-400 hover:text-zinc-200"
                    }`}
                  >
                    {t(`motion.${choice}`)}
                  </button>
                ))}
              </div>
            </div>
            <p className="text-2xs leading-relaxed text-zinc-500">{t("motion.help")}</p>
          </section>

          {/* What a cover tells under the pointer, besides the title */}
          <section id="settings-card-info" className="space-y-2">
            <span id="card-info-label" className="block text-xs font-medium text-zinc-200">
              {t("cardInfo.title")}
            </span>
            <div role="group" aria-labelledby="card-info-label" className="flex flex-wrap gap-1.5">
              {CARD_INFO_CHOICES.map((item) => {
                const on = cardInfo.includes(item);
                return (
                  <button
                    key={item}
                    id={`card-info-${item}`}
                    type="button"
                    aria-pressed={on}
                    onClick={() =>
                      setCardInfo((current) =>
                        on ? current.filter((entry) => entry !== item) : [...current, item]
                      )
                    }
                    className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs font-medium transition-colors ${
                      on
                        ? "border-accent/60 bg-accent/10 text-zinc-100"
                        : "border-oled-border bg-oled-bg text-zinc-400 hover:text-zinc-200"
                    }`}
                  >
                    <Check className={`h-3 w-3 ${on ? "text-accent" : "opacity-0"}`} aria-hidden="true" />
                    <span>{t(`cardInfo.${item}`)}</span>
                  </button>
                );
              })}
            </div>
            <p className="text-2xs leading-relaxed text-zinc-500">{t("cardInfo.help")}</p>
          </section>

          {/* Accessibility: applies on top of any theme */}
          <section className="space-y-2.5">
            <div className="flex items-center gap-1.5 text-xs font-medium text-zinc-200">
              <Accessibility className="h-3.5 w-3.5 text-accent" />
              <span>{t("a11y.section")}</span>
            </div>

            <div className="flex flex-wrap items-center justify-between gap-2">
              <span id="a11y-text-scale-label" className="text-xs text-zinc-300">
                {t("a11y.textSize")}
              </span>
              <div
                role="group"
                aria-labelledby="a11y-text-scale-label"
                className="flex rounded-lg border border-oled-border bg-oled-bg p-0.5"
              >
                {TEXT_SCALE_OPTIONS.map((percent) => (
                  <button
                    key={percent}
                    id={`a11y-text-scale-${percent}`}
                    type="button"
                    onClick={() => changeAccessibility({ textScale: percent })}
                    aria-pressed={accessibility.textScale === percent}
                    className={`rounded-md px-2.5 py-1 text-xs font-medium tabular-nums transition-colors ${
                      accessibility.textScale === percent
                        ? "bg-oled-elevated text-zinc-100"
                        : "text-zinc-400 hover:text-zinc-200"
                    }`}
                  >
                    {percent}%
                  </button>
                ))}
              </div>
            </div>

            <div className="flex flex-wrap items-center justify-between gap-2">
              <label htmlFor="a11y-font-select" className="text-xs text-zinc-300">
                {t("a11y.readableFont")}
              </label>
              <select
                id="a11y-font-select"
                value={accessibility.readableFont ?? ""}
                onChange={(e) =>
                  changeAccessibility({
                    readableFont:
                      e.target.value === "atkinson" || e.target.value === "opendyslexic"
                        ? e.target.value
                        : null,
                  })
                }
                className="rounded-lg border border-oled-border bg-oled-bg px-2.5 py-1.5 text-xs text-zinc-100 focus:border-accent/60 focus:outline-none"
              >
                <option value="">{t("a11y.fontTheme")}</option>
                <option value="atkinson">
                  {READABLE_FONTS.atkinson} — {t("a11y.fontLowVision")}
                </option>
                <option value="opendyslexic">
                  {READABLE_FONTS.opendyslexic} — {t("a11y.fontDyslexia")}
                </option>
              </select>
            </div>

            <ul className="space-y-1.5">
              {(
                [
                  ["highContrast", "a11y.highContrast", "a11y.highContrastHelp"],
                  ["reduceMotion", "a11y.reduceMotion", "a11y.reduceMotionHelp"],
                  ["alwaysShowScrollbars", "a11y.scrollbars", "a11y.scrollbarsHelp"],
                  ["strongFocus", "a11y.strongFocus", "a11y.strongFocusHelp"],
                ] as const
              ).map(([key, label, help]) => (
                <li
                  key={key}
                  className="flex items-center justify-between gap-4 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2"
                >
                  <div>
                    <label
                      htmlFor={`a11y-${key}`}
                      className="block text-xs font-medium text-zinc-200"
                    >
                      {t(label)}
                    </label>
                    <p className="text-2xs text-zinc-500">{t(help)}</p>
                  </div>
                  <input
                    id={`a11y-${key}`}
                    type="checkbox"
                    checked={accessibility[key]}
                    onChange={(e) => changeAccessibility({ [key]: e.target.checked })}
                    className="h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
                  />
                </li>
              ))}
            </ul>
          </section>

          {/* Plugins */}
          <section id="settings-plugins" className="space-y-2.5">
            <div className="flex items-center gap-1.5 text-xs font-medium text-zinc-200">
              <Puzzle className="h-3.5 w-3.5 text-accent" />
              <span>{t("settings.plugins")}</span>
            </div>
            <p className="text-2xs leading-relaxed text-zinc-500">{t("settings.pluginsHelp")}</p>
            <PluginsSettings
              plugins={plugins}
              warnings={pluginWarnings}
              states={pluginStates}
              onStatesChange={setPluginStates}
              onInstalledChanged={onPluginsChanged}
            />
          </section>

          {/* Store accounts */}
          <section id="settings-accounts" className="space-y-2.5">
            <div className="flex items-center gap-1.5 text-xs font-medium text-zinc-200">
              <UserRound className="h-3.5 w-3.5 text-accent" />
              <span>{t("accounts.title")}</span>
            </div>
            <p className="text-2xs leading-relaxed text-zinc-500">{t("accounts.help")}</p>
            <StoreAccountsSettings isOpen={isOpen} onSignedIn={onStoreSignedIn} />
            {/* Nothing tells Kanso about a Game Pass subscription: the user does */}
            {(
              [
                ["game-pass-catalog-checkbox", "accounts.gamePassCatalog", gamePassCatalog, setGamePassCatalog],
                ["game-pass-checkbox", "accounts.gamePass", hasGamePass, setHasGamePass],
              ] as const
            ).map(([id, key, checked, setChecked]) => (
              <div
                key={id}
                className="flex items-center justify-between gap-4 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2.5"
              >
                <div>
                  <label htmlFor={id} className="block text-xs font-medium text-zinc-200">
                    {t(`${key}.title`)}
                  </label>
                  <p className="text-2xs leading-relaxed text-zinc-500">{t(`${key}.help`)}</p>
                </div>
                <input
                  id={id}
                  type="checkbox"
                  checked={checked}
                  onChange={(e) => setChecked(e.target.checked)}
                  className="h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
                />
              </div>
            ))}
          </section>

          {/* SteamGridDB API Key */}
          <section className="space-y-2">
            <div className="flex items-center justify-between">
              <label
                htmlFor="steamgriddb-api-key-input"
                className="flex items-center gap-1.5 text-xs font-medium text-zinc-200"
              >
                <KeyRound className="h-3.5 w-3.5 text-accent" />
                <span>SteamGridDB API Key</span>
              </label>
              <span
                className={`rounded-full px-2 py-0.5 text-2xs font-medium ${
                  keyConfigured
                    ? "bg-emerald-500/15 text-emerald-300"
                    : "bg-zinc-800 text-zinc-400"
                }`}
              >
                {keyConfigured
                  ? t("settings.keyConfigured")
                  : t("settings.keyMissing")}
              </span>
            </div>

            <div className="relative">
              <input
                id="steamgriddb-api-key-input"
                type={showKey ? "text" : "password"}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={t("settings.keyPlaceholder")}
                autoComplete="off"
                spellCheck={false}
                className="w-full rounded-lg border border-oled-border bg-oled-bg py-2 pl-3 pr-10 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
              />
              <button
                id="toggle-apikey-visibility"
                type="button"
                onClick={() => setShowKey((v) => !v)}
                aria-label={showKey ? t("settings.hideKey") : t("settings.showKey")}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded p-1 text-zinc-400 hover:text-zinc-200"
              >
                {showKey ? (
                  <EyeOff className="h-3.5 w-3.5" />
                ) : (
                  <Eye className="h-3.5 w-3.5" />
                )}
              </button>
            </div>
            <p className="select-text text-2xs leading-relaxed text-zinc-500">
              {t("settings.keyHelp", {
                url: "steamgriddb.com/profile/preferences/api",
              })}
            </p>
          </section>

          {/* Custom / Standalone Folders */}
          <section className="space-y-2.5">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-zinc-200">
                {t("settings.folders")}
              </span>
              {isRunningInTauri() && (
                <button
                  id="browse-custom-folder-button"
                  type="button"
                  onClick={() => void handleBrowseFolder()}
                  className={smallButton}
                >
                  <FolderPlus className="h-3.5 w-3.5 text-accent" />
                  <span>{t("common.browse")}...</span>
                </button>
              )}
            </div>

            <div className="flex gap-2">
              <input
                id="manual-folder-input"
                type="text"
                value={manualFolderInput}
                onChange={(e) => setManualFolderInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    handleAddManualFolder();
                  }
                }}
                placeholder={t("settings.folderPlaceholder")}
                className="flex-1 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
              />
              <button
                id="add-manual-folder-button"
                type="button"
                onClick={handleAddManualFolder}
                className="inline-flex items-center gap-1 rounded-lg border border-oled-border bg-oled-elevated px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-600"
              >
                <Plus className="h-3.5 w-3.5" />
                <span>{t("common.add")}</span>
              </button>
            </div>

            {folders.length === 0 ? (
              <div className="rounded-lg border border-dashed border-oled-border bg-oled-bg/50 px-3 py-3 text-center text-xs text-zinc-500">
                {t("settings.noFolders")}
              </div>
            ) : (
              <ul className="max-h-36 space-y-1.5 scroll-area overflow-y-auto pr-1">
                {folders.map((folder, index) => (
                  <li
                    key={`${folder}-${index}`}
                    className="flex items-center justify-between gap-2 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs"
                  >
                    <span className="truncate font-mono text-zinc-300" title={folder}>
                      {folder}
                    </span>
                    <button
                      id={`remove-folder-${index}`}
                      type="button"
                      onClick={() =>
                        setFolders((prev) => prev.filter((_, i) => i !== index))
                      }
                      aria-label={t("settings.removeFolder", { folder })}
                      className="rounded p-1 text-zinc-500 transition-colors hover:bg-red-500/10 hover:text-red-300"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </li>
                ))}
              </ul>
            )}

            <p className="text-2xs text-zinc-500">
              {t("settings.foldersHelp", { list: SKIPPED_EXE_PATTERNS })}
            </p>
          </section>

          {/* Emulators and the folders of their games */}
          <EmulatorsSettings settings={emulators} onChange={setEmulators} onError={setErrorMsg} />

          {/* Launch behavior */}
          <section className="flex items-center justify-between gap-4 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2.5">
            <div>
              <label
                htmlFor="minimize-to-tray-checkbox"
                className="block text-xs font-medium text-zinc-200"
              >
                {t("settings.trayTitle")}
              </label>
              <p className="text-2xs text-zinc-500">{t("settings.trayHelp")}</p>
            </div>
            <input
              id="minimize-to-tray-checkbox"
              type="checkbox"
              checked={minimizeOnLaunch}
              onChange={(e) => setMinimizeOnLaunch(e.target.checked)}
              className="h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
            />
          </section>

          {/* Play time */}
          <section className="flex items-center justify-between gap-4 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2.5">
            <div>
              <label
                htmlFor="track-playtime-checkbox"
                className="block text-xs font-medium text-zinc-200"
              >
                {t("settings.trackTitle")}
              </label>
              <p className="text-2xs leading-relaxed text-zinc-500">{t("settings.trackHelp")}</p>
            </div>
            <input
              id="track-playtime-checkbox"
              type="checkbox"
              checked={trackPlaytime}
              onChange={(e) => setTrackPlaytime(e.target.checked)}
              className="h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
            />
          </section>
        </form>

        {/* Footer */}
        <div className="flex shrink-0 items-center justify-end gap-2.5 border-t border-oled-border px-6 py-4">
          <button
            id="cancel-settings-button"
            type="button"
            onClick={closeWithoutSaving}
            className="rounded-lg border border-oled-border bg-oled-bg px-3.5 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
          >
            {t("common.cancel")}
          </button>
          <button
            id="save-settings-button"
            type="submit"
            form="settings-form"
            disabled={isSaving}
            className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:opacity-60"
          >
            <Check className="h-3.5 w-3.5" />
            <span>{isSaving ? t("settings.saving") : t("settings.save")}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
