import { useEffect, useState } from "react";
import { Download, ExternalLink, RefreshCw, RotateCcw } from "lucide-react";
import { useI18n } from "../i18n";
import { dlssDownload, dlssRestore, dlssUpdate, openDlssSource } from "../lib/backend";
import { DLSS_PARTS, loadDlssStatus, noteDlssChange, outdated, setDlssStatus, useDlssStatus } from "../lib/dlss";
import type { DlssLibrary, GameDlss } from "../types/game";

interface DlssSettingsProps {
  /** The panel is the one in view: the library is looked through then, not before. */
  active: boolean;
  /** The switch as it stands in the dialog, saved or not. */
  enabled: boolean;
  onEnabledChange: (enabled: boolean) => void;
  /** The switch as it was last saved: what Kanso actually does. */
  saved: boolean;
}

/**
 * DLSS in the settings: the switch that lets Kanso ask NVIDIA's repository (off until the
 * user turns it on), NVIDIA's newest version, the download of its libraries to this PC, and
 * the games that carry DLSS, each of which can be brought up to date or put back as it was.
 */
export function DlssSettings({ active, enabled, onEnabledChange, saved }: DlssSettingsProps) {
  const { t, relativePast } = useI18n();
  const status = useDlssStatus();
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; failed: boolean } | null>(null);

  // The games' folders are looked through when the panel is first shown, and the answer
  // follows the switch once it is saved
  useEffect(() => {
    if (active && (!status || status.online !== saved)) {
      void loadDlssStatus().catch(() => {});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, saved]);

  const run = async (name: string, work: () => Promise<string | null>) => {
    setBusy(name);
    setMessage(null);
    try {
      const done = await work();
      if (done) setMessage({ text: done, failed: false });
    } catch (err) {
      setMessage({ text: typeof err === "string" ? err : t("dlss.failed"), failed: true });
    } finally {
      setBusy(null);
    }
  };
  const change = (game: GameDlss, work: () => Promise<{ changed: number; libraries: DlssLibrary[] }>, done: "dlss.updated" | "dlss.restored") =>
    run(game.gameId, async () => {
      const result = await work();
      noteDlssChange({ id: game.gameId, title: game.title }, result.libraries);
      return `${game.title}: ${t(done, { count: result.changed })}`;
    });

  const latest = status?.latest ?? null;
  const online = Boolean(status?.online) && saved;
  const allDownloaded = status !== null && DLSS_PARTS.every((part) => status.downloaded.includes(part));
  const small =
    "inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-oled-border bg-oled-surface px-2.5 py-1.5 text-2xs font-medium text-zinc-200 transition-colors hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-50";

  return (
    <>
      {/* The switch: nothing about DLSS goes on the network without it */}
      <section id="settings-dlss-online" className="space-y-2">
        <div className="flex items-center justify-between gap-4 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2.5">
          <div>
            <label htmlFor="dlss-updates-checkbox" className="block text-xs font-medium text-zinc-200">
              {t("settings.dlssOnline")}
            </label>
            <p className="text-2xs leading-relaxed text-zinc-500">{t("settings.dlssOnlineHelp")}</p>
          </div>
          <input
            id="dlss-updates-checkbox"
            type="checkbox"
            checked={enabled}
            onChange={(e) => onEnabledChange(e.target.checked)}
            className="h-4 w-4 shrink-0 rounded border-zinc-600 bg-oled-surface accent-accent"
          />
        </div>
        <p className="text-2xs leading-relaxed text-zinc-500">
          {t("settings.dlssLicence")}{" "}
          <button
            type="button"
            id="dlss-source-link"
            onClick={() => void openDlssSource().catch(() => {})}
            className="inline-flex items-center gap-1 text-zinc-300 underline underline-offset-2 hover:text-zinc-100"
          >
            github.com/NVIDIA/DLSS
            <ExternalLink className="h-2.5 w-2.5" />
          </button>
        </p>
        {enabled !== saved && (
          <p id="dlss-save-first" className="text-2xs text-amber-200">
            {t("settings.dlssSaveFirst")}
          </p>
        )}
      </section>

      {/* NVIDIA's newest version, and its libraries on this PC */}
      {online && (
        <section id="settings-dlss-latest" className="space-y-2">
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2.5">
            <div className="min-w-0">
              <span className="block text-xs font-medium text-zinc-200">
                {latest ? t("settings.dlssLatest", { version: latest }) : t("settings.dlssLatestUnknown")}
              </span>
              <p className="text-2xs leading-relaxed text-zinc-500">
                {status?.error
                  ? t("settings.dlssCheckFailed", { error: status.error })
                  : status?.checkedAt
                    ? t("settings.dlssChecked", { time: relativePast(status.checkedAt) })
                    : ""}
                {allDownloaded ? ` ${t("settings.dlssDownloaded")}` : ""}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <button
                id="dlss-check-button"
                type="button"
                disabled={busy !== null}
                onClick={() => void run("check", async () => (await loadDlssStatus(true), null))}
                className={small}
              >
                <RefreshCw className={`h-3 w-3 ${busy === "check" ? "animate-spin text-accent" : "text-zinc-400"}`} />
                <span>{t("settings.dlssCheck")}</span>
              </button>
              <button
                id="dlss-download-button"
                type="button"
                disabled={busy !== null || !latest || allDownloaded}
                onClick={() =>
                  void run("download", async () => {
                    setDlssStatus(await dlssDownload());
                    return t("settings.dlssDownloadDone");
                  })
                }
                className={small}
              >
                <Download className={`h-3 w-3 ${busy === "download" ? "animate-pulse text-accent" : "text-zinc-400"}`} />
                <span>{t(busy === "download" ? "settings.dlssDownloading" : "settings.dlssDownload")}</span>
              </button>
            </div>
          </div>
        </section>
      )}

      {/* The games that carry DLSS */}
      <section id="settings-dlss-games" className="space-y-2">
        <div className="flex items-end justify-between gap-3">
          <div>
            <span className="block text-xs font-medium text-zinc-200">{t("settings.dlssGames")}</span>
            <p className="text-2xs leading-relaxed text-zinc-500">{t("settings.dlssGamesHelp")}</p>
          </div>
          <button
            id="dlss-rescan-button"
            type="button"
            disabled={busy !== null}
            onClick={() => void run("rescan", async () => (await loadDlssStatus(true), null))}
            className={small}
          >
            <RefreshCw className={`h-3 w-3 ${busy === "rescan" ? "animate-spin text-accent" : "text-zinc-400"}`} />
            <span>{t("settings.dlssRescan")}</span>
          </button>
        </div>
        {status === null ? (
          <p className="rounded-lg border border-oled-border bg-oled-bg px-3.5 py-3 text-2xs text-zinc-500">{t("settings.dlssLooking")}</p>
        ) : status.games.length === 0 ? (
          <p id="dlss-no-games" className="rounded-lg border border-oled-border bg-oled-bg px-3.5 py-3 text-2xs text-zinc-500">
            {t("settings.dlssNoGames")}
          </p>
        ) : (
          <ul id="dlss-games" className="space-y-1.5">
            {status.games.map((game) => {
              const behind = outdated(game.libraries, online ? latest : null).length > 0;
              const replaced = game.libraries.some((library) => library.replaced);
              return (
                <li
                  key={game.gameId}
                  data-dlss-game={game.gameId}
                  data-outdated={behind || undefined}
                  className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2"
                >
                  <div className="min-w-0">
                    <span className="block truncate text-xs font-medium text-zinc-200">{game.title}</span>
                    <p className="truncate text-2xs tabular-nums text-zinc-500">
                      {DLSS_PARTS.flatMap((part) => {
                        const versions = game.libraries.filter((library) => library.part === part).map((library) => library.version);
                        return versions.length > 0 ? [`${t(`dlss.short.${part}`)} ${versions.join(" / ")}`] : [];
                      }).join(" · ")}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    {behind && (
                      <button
                        type="button"
                        data-dlss-update={game.gameId}
                        disabled={busy !== null}
                        onClick={() => void change(game, () => dlssUpdate({ id: game.gameId }, { kind: "nvidia" }), "dlss.updated")}
                        className={small}
                        title={t("dlss.warning")}
                      >
                        <Download className={`h-3 w-3 ${busy === game.gameId ? "animate-pulse text-accent" : "text-accent"}`} />
                        <span>{t("settings.dlssUpdateTo", { version: latest ?? "" })}</span>
                      </button>
                    )}
                    {replaced && (
                      <button
                        type="button"
                        data-dlss-restore={game.gameId}
                        disabled={busy !== null}
                        onClick={() => void change(game, () => dlssRestore({ id: game.gameId }), "dlss.restored")}
                        className={small}
                        title={t("dlss.restoreHelp")}
                      >
                        <RotateCcw className="h-3 w-3 text-zinc-400" />
                        <span>{t("dlss.restore")}</span>
                      </button>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
        {message && (
          <p id="dlss-settings-message" role="status" className={`text-2xs leading-relaxed ${message.failed ? "text-amber-200" : "text-zinc-400"}`}>
            {message.text}
          </p>
        )}
        <p className="text-2xs leading-relaxed text-zinc-500">{t("dlss.warning")}</p>
      </section>
    </>
  );
}
