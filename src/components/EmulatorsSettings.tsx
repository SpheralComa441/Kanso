import { useEffect, useState } from "react";
import { FolderPlus, Joystick, Plus, Trash2, X } from "lucide-react";
import { useI18n } from "../i18n";
import {
  identifyEmulator,
  isRunningInTauri,
  listEmulators,
  pickCustomFolder,
  pickEmulatorExecutable,
} from "../lib/backend";
import type { EmulatorInfo, EmulatorSetting } from "../types/game";

interface EmulatorsSettingsProps {
  /** What Settings keeps: emulators the user added, folders added to the ones Kanso found. */
  settings: EmulatorSetting[];
  onChange: (next: EmulatorSetting[]) => void;
  onError: (message: string) => void;
}

const EMPTY: Omit<EmulatorSetting, "id"> = { name: "", exePath: "", folders: [], extensions: [], args: "" };

/** An id for an emulator Kanso does not know, from the name of its executable. */
function customId(exePath: string, taken: string[]): string {
  const stem = exePath.replace(/\\/g, "/").split("/").pop()?.replace(/\.exe$/i, "") ?? "";
  const slug = stem.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 28) || "emulator";
  let id = `custom-${slug}`;
  for (let n = 2; taken.includes(id); n += 1) id = `custom-${slug}-${n}`;
  return id;
}

/**
 * Emulators in Settings: the ones Kanso found, the ones the user adds, and the folders of
 * games of each. Everything here is saved with the other settings; the scan that follows
 * reads the games.
 */
export function EmulatorsSettings({ settings, onChange, onError }: EmulatorsSettingsProps) {
  const { t } = useI18n();
  const [found, setFound] = useState<EmulatorInfo[]>([]);
  // What Kanso recognised the executables picked in this dialog as
  const [picked, setPicked] = useState<Record<string, { name: string; systems: string; known: boolean }>>({});

  useEffect(() => {
    let current = true;
    listEmulators()
      .then((list) => current && setFound(list))
      .catch(() => {});
    return () => {
      current = false;
    };
  }, []);

  const settingOf = (id: string) => settings.find((setting) => setting.id === id);
  const update = (id: string, change: (setting: EmulatorSetting) => EmulatorSetting) => {
    const current = settingOf(id) ?? { id, ...EMPTY };
    const next = change(current);
    onChange(settingOf(id) ? settings.map((s) => (s.id === id ? next : s)) : [...settings, next]);
  };

  // One row per emulator: those the backend reports, then those added in this dialog
  const rows = [
    ...found.map((emulator) => ({
      id: emulator.id,
      name: emulator.name,
      systems: emulator.systems,
      exePath: settingOf(emulator.id)?.exePath || emulator.exePath,
      ownFolders: emulator.folders.filter((folder) => folder.fromEmulator).map((folder) => folder.path),
      // Removing what the user added leaves the copy Kanso found, if there is one
      addedByUser: Boolean(settingOf(emulator.id)?.exePath),
      known: !emulator.id.startsWith("custom-"),
      gone: emulator.addedByUser && !settingOf(emulator.id)?.exePath,
    })),
    ...settings
      .filter((setting) => setting.exePath && !found.some((emulator) => emulator.id === setting.id))
      .map((setting) => ({
        id: setting.id,
        name: picked[setting.id]?.name ?? (setting.name || setting.id),
        systems: picked[setting.id]?.systems ?? "",
        exePath: setting.exePath,
        ownFolders: [] as string[],
        addedByUser: true,
        known: picked[setting.id]?.known ?? !setting.id.startsWith("custom-"),
        gone: false,
      })),
  ].filter((row) => !row.gone);

  const addEmulator = async () => {
    try {
      const exePath = await pickEmulatorExecutable();
      if (!exePath) return;
      const identity = await identifyEmulator(exePath);
      const id = identity?.id ?? customId(exePath, settings.map((setting) => setting.id));
      const stem = exePath.replace(/\\/g, "/").split("/").pop()?.replace(/\.exe$/i, "") ?? id;
      setPicked((current) => ({
        ...current,
        [id]: { name: identity?.name ?? stem, systems: identity?.systems ?? "", known: identity !== null },
      }));
      update(id, (setting) => ({ ...setting, exePath, name: identity ? "" : setting.name || stem }));
    } catch {
      onError(t("settings.folderPickerError"));
    }
  };

  const addFolder = async (id: string) => {
    try {
      const folder = await pickCustomFolder();
      if (!folder) return;
      update(id, (setting) =>
        setting.folders.includes(folder) ? setting : { ...setting, folders: [...setting.folders, folder] }
      );
    } catch {
      onError(t("settings.folderPickerError"));
    }
  };

  const field =
    "w-full rounded-lg border border-oled-border bg-oled-bg px-2.5 py-1.5 text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-accent focus:outline-none";

  return (
    <section id="settings-emulators" className="space-y-3">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-1.5 text-xs font-medium text-zinc-200">
          <Joystick className="h-3.5 w-3.5 text-accent" />
          <span>{t("settings.emulators")}</span>
        </div>
        <button
          id="add-emulator-button"
          type="button"
          onClick={() => void addEmulator()}
          disabled={!isRunningInTauri()}
          className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated disabled:opacity-50"
        >
          <Plus className="h-3.5 w-3.5" />
          <span>{t("settings.emulatorAdd")}</span>
        </button>
      </div>

      {rows.length === 0 ? (
        <p className="rounded-lg border border-dashed border-oled-border px-3.5 py-3 text-center text-xs text-zinc-500">
          {t("settings.emulatorsNone")}
        </p>
      ) : (
        <ul className="space-y-2">
          {rows.map((row) => {
            const setting = settingOf(row.id);
            const folders = setting?.folders ?? [];
            return (
              <li
                key={row.id}
                data-emulator-setting={row.id}
                className="space-y-2.5 rounded-lg border border-oled-border bg-oled-bg/50 p-3"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 space-y-0.5">
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                      <span className="text-xs font-medium text-zinc-100">{row.name}</span>
                      {row.systems && <span className="text-2xs text-zinc-400">{row.systems}</span>}
                      <span className="text-3xs text-zinc-500">
                        {t(row.addedByUser ? "settings.emulatorAdded" : "settings.emulatorFound")}
                      </span>
                    </div>
                    <p className="select-text truncate font-mono text-3xs text-zinc-500" title={row.exePath}>
                      {row.exePath}
                    </p>
                  </div>
                  {row.addedByUser && (
                    <button
                      type="button"
                      onClick={() => onChange(settings.filter((s) => s.id !== row.id))}
                      aria-label={t("settings.emulatorRemove", { name: row.name })}
                      title={t("settings.emulatorRemove", { name: row.name })}
                      className="shrink-0 rounded-lg p-1.5 text-zinc-500 transition-colors hover:bg-red-500/10 hover:text-red-300"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  )}
                </div>

                {/* An emulator Kanso does not know: its name and which files are its games */}
                {!row.known && (
                  <div className="space-y-2">
                    <p className="text-2xs leading-relaxed text-zinc-500">{t("settings.emulatorUnknown")}</p>
                    <div className="grid gap-2 sm:grid-cols-2">
                      <label className="space-y-1 text-2xs text-zinc-400">
                        <span>{t("settings.emulatorName")}</span>
                        <input
                          value={setting?.name ?? ""}
                          maxLength={60}
                          onChange={(e) => update(row.id, (s) => ({ ...s, name: e.target.value }))}
                          className={field}
                        />
                      </label>
                      <label className="space-y-1 text-2xs text-zinc-400">
                        <span>{t("settings.emulatorExtensions")}</span>
                        <input
                          data-emulator-extensions={row.id}
                          defaultValue={(setting?.extensions ?? []).join(", ")}
                          placeholder={t("settings.emulatorExtensionsHint")}
                          onChange={(e) =>
                            update(row.id, (s) => ({
                              ...s,
                              extensions: e.target.value
                                .split(/[\s,;]+/)
                                .map((extension) => extension.replace(/^\./, "").toLowerCase())
                                .filter((extension) => /^[a-z0-9]{1,10}$/.test(extension)),
                            }))
                          }
                          className={field}
                        />
                      </label>
                    </div>
                  </div>
                )}

                <div className="space-y-1.5">
                  {row.ownFolders.map((folder) => (
                    <p key={folder} className="flex items-center gap-1.5 text-2xs text-zinc-500">
                      <span className="select-text truncate font-mono" title={folder}>
                        {folder}
                      </span>
                      <span className="shrink-0 text-zinc-600">· {t("emulators.fromEmulator")}</span>
                    </p>
                  ))}
                  {folders.map((folder) => (
                    <p key={folder} data-emulator-folder={folder} className="flex items-center gap-1.5 text-2xs text-zinc-300">
                      <span className="select-text min-w-0 flex-1 truncate font-mono" title={folder}>
                        {folder}
                      </span>
                      <button
                        type="button"
                        onClick={() =>
                          update(row.id, (s) => ({ ...s, folders: s.folders.filter((f) => f !== folder) }))
                        }
                        aria-label={t("settings.removeFolder", { folder })}
                        className="shrink-0 rounded p-0.5 text-zinc-500 transition-colors hover:text-red-300"
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </p>
                  ))}
                  <button
                    type="button"
                    data-add-emulator-folder={row.id}
                    onClick={() => void addFolder(row.id)}
                    disabled={!isRunningInTauri()}
                    className="inline-flex items-center gap-1.5 text-2xs font-medium text-zinc-400 transition-colors hover:text-zinc-100 disabled:opacity-50"
                  >
                    <FolderPlus className="h-3 w-3" />
                    <span>{t("settings.emulatorAddFolder")}</span>
                  </button>
                </div>

                <label className="block space-y-1 text-2xs text-zinc-400">
                  <span>{t("settings.emulatorArgs")}</span>
                  <input
                    data-emulator-args={row.id}
                    value={setting?.args ?? ""}
                    maxLength={400}
                    placeholder={'"{rom}"'}
                    onChange={(e) => update(row.id, (s) => ({ ...s, args: e.target.value }))}
                    className={`${field} font-mono`}
                  />
                  <span className="block text-3xs leading-relaxed text-zinc-500">
                    {t("settings.emulatorArgsHelp")}
                  </span>
                </label>
              </li>
            );
          })}
        </ul>
      )}

      <p className="text-2xs leading-relaxed text-zinc-500">
        {t("settings.emulatorsHelp")} {t("settings.emulatorsRescan")}
        {!isRunningInTauri() && ` ${t("settings.emulatorsDesktopOnly")}`}
      </p>
    </section>
  );
}
