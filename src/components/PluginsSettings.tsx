import { useState } from "react";
import { Eye, EyeOff, FolderOpen, Globe, Home, Plus, ShieldCheck, Trash2, WifiOff } from "lucide-react";
import { useI18n } from "../i18n";
import {
  installPlugin,
  isRunningInTauri,
  openPluginsFolder,
  pickPluginToInstall,
  removePlugin,
} from "../lib/backend";
import type {
  AppConfig,
  PluginInfo,
  PluginPreview,
  PluginSettingInfo,
  PluginState,
} from "../types/game";

interface PluginsSettingsProps {
  plugins: PluginInfo[];
  /** One entry per folder in the plugins folder that is not a valid plugin. */
  warnings: string[];
  /** The choices being edited in Settings; saved with everything else. */
  states: Record<string, PluginState>;
  onStatesChange: (states: Record<string, PluginState>) => void;
  /** A plugin was installed or removed: the saved settings changed and the list must be re-read. */
  onInstalledChanged: (saved: AppConfig) => Promise<void>;
  /** The plugin with this id was removed: what is being edited about it is to be forgotten. */
  onRemoved: (pluginId: string) => void;
}

/** The choices for a plugin as edited so far, falling back to what the backend reported. */
function stateOf(plugin: PluginInfo, states: Record<string, PluginState>): PluginState {
  return (
    states[plugin.id] ?? { enabled: plugin.enabled, showBadge: plugin.showBadge, settings: {} }
  );
}

interface SettingFieldProps {
  pluginId: string;
  setting: PluginSettingInfo;
  value: string;
  onChange: (value: string) => void;
}

function SettingField({ pluginId, setting, value, onChange }: SettingFieldProps) {
  const [revealed, setRevealed] = useState(false);
  const id = `plugin-${pluginId}-${setting.key}`;
  const inputClass =
    "w-full rounded-md border border-oled-border bg-oled-bg px-2.5 py-1.5 text-xs text-zinc-100 placeholder-zinc-600 focus:border-accent/50 focus:outline-none";

  if (setting.kind === "toggle") {
    return (
      <label htmlFor={id} className="flex items-center justify-between gap-4 text-xs text-zinc-300">
        <span>{setting.label}</span>
        <input
          id={id}
          type="checkbox"
          checked={value === "true"}
          onChange={(e) => onChange(String(e.target.checked))}
          className="h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
        />
      </label>
    );
  }

  return (
    <div className="space-y-1">
      <label htmlFor={id} className="block text-xs text-zinc-300">
        {setting.label}
      </label>
      {setting.kind === "select" ? (
        <select id={id} value={value} onChange={(e) => onChange(e.target.value)} className={inputClass}>
          {setting.options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      ) : (
        <div className="relative">
          <input
            id={id}
            type={setting.kind === "secret" && !revealed ? "password" : "text"}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            className={`${inputClass} ${setting.kind === "secret" ? "pr-9 font-mono" : ""}`}
          />
          {setting.kind === "secret" && (
            <button
              type="button"
              onClick={() => setRevealed((shown) => !shown)}
              aria-label={setting.label}
              aria-pressed={revealed}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-1 text-zinc-500 hover:text-zinc-200"
            >
              {revealed ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
            </button>
          )}
        </div>
      )}
      {setting.help && <p className="text-2xs leading-relaxed text-zinc-500">{setting.help}</p>}
    </div>
  );
}

/**
 * Settings → Plugins: what each plugin is, which sites it may contact, on/off, its own
 * settings, and installing or removing the user's plugins.
 */
export function PluginsSettings({
  plugins,
  warnings,
  states,
  onStatesChange,
  onInstalledChanged,
  onRemoved,
}: PluginsSettingsProps) {
  const { t } = useI18n();
  const [pending, setPending] = useState<{ path: string; preview: PluginPreview } | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  const [notice, setNotice] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const canInstall = isRunningInTauri();

  const change = (plugin: PluginInfo, patch: Partial<PluginState>) => {
    onStatesChange({ ...states, [plugin.id]: { ...stateOf(plugin, states), ...patch } });
  };

  const fail = (error: unknown) =>
    setNotice({ type: "error", text: typeof error === "string" ? error : t("plugins.actionError") });

  const chooseFile = async () => {
    setNotice(null);
    try {
      setPending(await pickPluginToInstall());
    } catch (error) {
      fail(error);
    }
  };

  const confirmInstall = async () => {
    if (!pending) return;
    setIsBusy(true);
    try {
      const saved = await installPlugin(pending.path);
      const { id, name } = pending.preview.plugin;
      // Keep whatever else is being edited; adopt the new plugin's saved state
      if (saved.plugins[id]) onStatesChange({ ...states, [id]: saved.plugins[id] });
      await onInstalledChanged(saved);
      setNotice({ type: "success", text: t("plugins.installedNotice", { name }) });
      setPending(null);
    } catch (error) {
      fail(error);
      setPending(null);
    } finally {
      setIsBusy(false);
    }
  };

  const remove = async (plugin: PluginInfo) => {
    setIsBusy(true);
    setNotice(null);
    try {
      const saved = await removePlugin(plugin.id);
      const next = { ...states };
      delete next[plugin.id];
      onStatesChange(next);
      onRemoved(plugin.id);
      await onInstalledChanged(saved);
      setNotice({ type: "success", text: t("plugins.removedNotice", { name: plugin.name }) });
    } catch (error) {
      fail(error);
    } finally {
      setIsBusy(false);
    }
  };

  const smallButton =
    "inline-flex items-center gap-1.5 rounded-md border border-oled-border bg-oled-elevated px-2.5 py-1 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-600 disabled:opacity-60";

  return (
    <div className="space-y-2.5">
      <ul className="space-y-2">
        {plugins.map((plugin) => {
          const state = stateOf(plugin, states);
          return (
            <li
              key={plugin.id}
              data-plugin={plugin.id}
              className="rounded-lg border border-oled-border bg-oled-bg px-3.5 py-3"
            >
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs font-medium text-zinc-100">
                    <span>{plugin.name}</span>
                    <span className="rounded-full border border-oled-border px-1.5 py-px text-3xs font-normal text-zinc-400">
                      {plugin.bundled ? t("plugins.official") : t("plugins.userInstalled")}
                    </span>
                  </p>
                  <p className="mt-1 text-2xs leading-relaxed text-zinc-400">{plugin.description}</p>
                  <p className="mt-1.5 flex items-start gap-1.5 text-2xs text-zinc-500">
                    {plugin.hosts.length > 0 ? (
                      <>
                        <Globe className="mt-px h-3 w-3 shrink-0" />
                        <span className="select-text break-all">
                          {t("plugins.contacts", { hosts: plugin.hosts.join(", ") })}
                        </span>
                      </>
                    ) : (
                      <>
                        <WifiOff className="mt-px h-3 w-3 shrink-0" />
                        <span>{t("plugins.noNetwork")}</span>
                      </>
                    )}
                  </p>
                  {/* Where it shows besides the game sheets and the menu: said like the sites it contacts */}
                  {plugin.home && (
                    <p data-plugin-home={plugin.id} className="mt-1 flex items-start gap-1.5 text-2xs text-zinc-500">
                      <Home className="mt-px h-3 w-3 shrink-0" />
                      <span>{t("plugins.addsHome")}</span>
                    </p>
                  )}
                  <p className="mt-0.5 text-3xs text-zinc-600">
                    {t("plugins.byAuthor", {
                      version: plugin.version,
                      author: plugin.author ?? "—",
                    })}
                  </p>
                </div>
                <input
                  id={`plugin-enabled-${plugin.id}`}
                  type="checkbox"
                  checked={state.enabled}
                  onChange={(e) => change(plugin, { enabled: e.target.checked })}
                  aria-label={t("plugins.enable", { name: plugin.name })}
                  className="mt-0.5 h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
                />
              </div>

              {state.enabled && (plugin.badge || plugin.settings.length > 0) && (
                <div className="mt-3 space-y-2.5 border-t border-oled-border/70 pt-3">
                  {plugin.badge && (
                    <label
                      htmlFor={`plugin-badge-${plugin.id}`}
                      className="flex items-center justify-between gap-4 text-xs text-zinc-300"
                    >
                      <span>{t("plugins.showBadge")}</span>
                      <input
                        id={`plugin-badge-${plugin.id}`}
                        type="checkbox"
                        checked={state.showBadge}
                        onChange={(e) => change(plugin, { showBadge: e.target.checked })}
                        className="h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
                      />
                    </label>
                  )}
                  {plugin.settings.map((setting) => (
                    <SettingField
                      key={setting.key}
                      pluginId={plugin.id}
                      setting={setting}
                      value={state.settings[setting.key] ?? setting.default}
                      onChange={(value) =>
                        change(plugin, { settings: { ...state.settings, [setting.key]: value } })
                      }
                    />
                  ))}
                </div>
              )}

              {!plugin.bundled && (
                <div className="mt-3 flex justify-end">
                  <button
                    type="button"
                    onClick={() => void remove(plugin)}
                    disabled={isBusy}
                    aria-label={t("plugins.removeNamed", { name: plugin.name })}
                    className={smallButton}
                  >
                    <Trash2 className="h-3 w-3" />
                    <span>{t("plugins.remove")}</span>
                  </button>
                </div>
              )}
            </li>
          );
        })}
      </ul>

      {warnings.length > 0 && (
        <ul className="select-text space-y-1 rounded-lg border border-amber-500/25 bg-amber-500/10 px-3.5 py-2.5 text-2xs text-amber-200">
          {warnings.map((warning, index) => (
            <li key={index} className="break-words">
              {warning}
            </li>
          ))}
        </ul>
      )}

      {notice && (
        <p
          role="status"
          className={`select-text rounded-lg border px-3.5 py-2 text-xs ${
            notice.type === "success"
              ? "border-emerald-500/25 bg-emerald-500/10 text-emerald-200"
              : "border-red-500/30 bg-red-500/10 text-red-200"
          }`}
        >
          {notice.text}
        </p>
      )}

      {pending ? (
        <div
          id="plugin-install-confirm"
          role="group"
          aria-label={t("plugins.confirmTitle", { name: pending.preview.plugin.name })}
          className="space-y-2.5 rounded-lg border border-accent/40 bg-accent/5 px-3.5 py-3"
        >
          <p className="text-xs font-semibold text-zinc-100">
            {t("plugins.confirmTitle", { name: pending.preview.plugin.name })}
          </p>
          <p className="text-2xs text-zinc-500">
            {t("plugins.byAuthor", {
              version: pending.preview.plugin.version,
              author: pending.preview.plugin.author ?? "—",
            })}
          </p>
          {pending.preview.plugin.description && (
            <p className="select-text text-2xs leading-relaxed text-zinc-300">
              {pending.preview.plugin.description}
            </p>
          )}
          {pending.preview.plugin.hosts.length > 0 ? (
            <div className="text-2xs text-zinc-300">
              <p className="flex items-center gap-1.5 font-medium">
                <Globe className="h-3 w-3" />
                {t("plugins.confirmHosts")}
              </p>
              <ul className="mt-1 select-text space-y-0.5 pl-5 font-mono text-zinc-200">
                {pending.preview.plugin.hosts.map((host) => (
                  <li key={host}>{host}</li>
                ))}
              </ul>
            </div>
          ) : (
            <p className="flex items-center gap-1.5 text-2xs text-zinc-300">
              <WifiOff className="h-3 w-3" />
              {t("plugins.confirmNoHosts")}
            </p>
          )}
          {pending.preview.plugin.home && (
            <p className="flex items-center gap-1.5 text-2xs text-zinc-300">
              <Home className="h-3 w-3" />
              {t("plugins.addsHome")}
            </p>
          )}
          <p className="flex items-start gap-1.5 text-2xs leading-relaxed text-zinc-400">
            <ShieldCheck className="mt-px h-3 w-3 shrink-0" />
            <span>{t("plugins.confirmSees")}</span>
          </p>
          {pending.preview.replacesVersion && (
            <p className="text-2xs text-amber-200">
              {t("plugins.confirmReplace", { version: pending.preview.replacesVersion })}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <button type="button" onClick={() => setPending(null)} disabled={isBusy} className={smallButton}>
              {t("common.cancel")}
            </button>
            <button
              id="confirm-plugin-install-button"
              type="button"
              onClick={() => void confirmInstall()}
              disabled={isBusy}
              className="inline-flex items-center gap-1.5 rounded-md bg-accent px-3 py-1 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:opacity-60"
            >
              {t("plugins.confirmInstall")}
            </button>
          </div>
        </div>
      ) : (
        <div className="flex flex-wrap items-center gap-2">
          <button
            id="install-plugin-button"
            type="button"
            onClick={() => void chooseFile()}
            disabled={!canInstall || isBusy}
            title={canInstall ? undefined : t("plugins.desktopOnly")}
            className={smallButton}
          >
            <Plus className="h-3.5 w-3.5" />
            <span>{t("plugins.install")}</span>
          </button>
          {canInstall && (
            <button
              id="open-plugins-folder-button"
              type="button"
              onClick={() => void openPluginsFolder().catch(fail)}
              className={smallButton}
            >
              <FolderOpen className="h-3.5 w-3.5" />
              <span>{t("plugins.openFolder")}</span>
            </button>
          )}
        </div>
      )}
      <p className="text-2xs leading-relaxed text-zinc-500">{t("plugins.installHint")}</p>
    </div>
  );
}
