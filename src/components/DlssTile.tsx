import { useEffect, useState } from "react";
import { ArrowUp, ChevronDown, Download, FileUp, Gamepad2, RotateCcw } from "lucide-react";
import { useI18n } from "../i18n";
import { dlssRestore, dlssUpdate, gameDlss, pickDlssLibrary } from "../lib/backend";
import { DLSS_PARTS, donorsFor, isOlder, loadDlssStatus, noteDlssChange, outdated, useDlssStatus } from "../lib/dlss";
import type { DetectedGame, DlssLibrary, DlssSource } from "../types/game";

/** Other games offered as a source at once: the ones with the newest libraries. */
const DONORS_SHOWN = 3;

/**
 * The versions of DLSS the game carries: Super Resolution, Frame Generation, Ray
 * Reconstruction, each as its library in the game's folder declares it. Read when the sheet
 * opens; a game without DLSS shows nothing. With `tile`, it is a tile of its own.
 *
 * From here a library can be brought up to date: from another of the user's games, from a
 * file they chose or, with the setting on, from NVIDIA's repository. The game's original is
 * kept and can be put back.
 */
export function DlssTile({ game, tile = false }: { game: DetectedGame; tile?: boolean }) {
  const { t } = useI18n();
  const status = useDlssStatus();
  const [libraries, setLibraries] = useState<DlssLibrary[]>([]);
  const [choosing, setChoosing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; failed: boolean } | null>(null);

  useEffect(() => {
    let current = true;
    setLibraries([]);
    setChoosing(false);
    setMessage(null);
    if (game.isInstalled && game.platform !== "emulator") {
      gameDlss(game)
        .then((found) => current && setLibraries(found))
        // Not being able to read them is the same as not having them
        .catch(() => {});
    }
    return () => {
      current = false;
    };
    // The folder is the game's: another game, or the same one installed since, reads again
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game.id, game.isInstalled, game.installDir]);

  if (libraries.length === 0) return null;
  const rows = DLSS_PARTS.map((part) => ({ part, found: libraries.filter((library) => library.part === part) })).filter(
    (row) => row.found.length > 0
  );
  const latest = status?.latest ?? null;
  const behind = outdated(libraries, latest);
  const donors = donorsFor(status, game.id, libraries).slice(0, DONORS_SHOWN);
  const replaced = libraries.some((library) => library.replaced);

  const openChoices = () => {
    setMessage(null);
    setChoosing((open) => !open);
    // Which other games could lend a newer library is only known once the library was looked through
    if (!status) void loadDlssStatus().catch(() => {});
  };
  const apply = async (work: () => Promise<{ changed: number; libraries: DlssLibrary[] }>, done: "dlss.updated" | "dlss.restored") => {
    setBusy(true);
    setMessage(null);
    try {
      const change = await work();
      setLibraries(change.libraries);
      noteDlssChange(game, change.libraries);
      setChoosing(false);
      setMessage({ text: t(done, { count: change.changed }), failed: false });
    } catch (err) {
      setMessage({ text: typeof err === "string" ? err : t("dlss.failed"), failed: true });
    } finally {
      setBusy(false);
    }
  };
  const update = (source: DlssSource) => apply(() => dlssUpdate(game, source), "dlss.updated");
  const updateFromFile = async () => {
    const path = await pickDlssLibrary().catch(() => null);
    if (path) await update({ kind: "file", path });
  };
  const choice =
    "flex w-full items-center gap-2 rounded-lg border border-oled-border bg-oled-bg/60 px-2.5 py-1.5 text-left text-2xs text-zinc-200 transition-colors hover:border-zinc-600 hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-50";

  const list = (
    <div id="detail-dlss" className="space-y-2.5">
      <div className="flex items-center justify-between gap-2">
        <span className="micro-label" title={t("dlss.help")}>
          {t("dlss.title")}
        </span>
        {behind.length > 0 && (
          <span id="dlss-outdated" className="rounded-full bg-amber-400 px-1.5 py-px text-3xs font-semibold text-zinc-950" title={t("dlss.outdatedHelp", { version: latest ?? "" })}>
            {t("dlss.outdated")}
          </span>
        )}
      </div>
      <dl className="space-y-1">
        {rows.map(({ part, found }) => (
          <div key={part} data-dlss={part} className="flex items-baseline justify-between gap-3 text-xs">
            <dt className="min-w-0 truncate text-zinc-400">{t(`dlss.${part}`)}</dt>
            <dd
              className="flex shrink-0 select-text items-baseline gap-1.5 font-medium tabular-nums text-zinc-100"
              title={
                found.length > 1
                  ? t("dlss.mixed")
                  : found[0].copies > 1
                    ? t("dlss.copies", { count: found[0].copies })
                    : undefined
              }
            >
              <span>{found.map((library) => library.version).join(" · ")}</span>
              {found.some((library) => isOlder(library, latest)) && (
                <span data-dlss-newer className="inline-flex items-center gap-0.5 text-3xs font-normal text-amber-300">
                  <ArrowUp className="h-2.5 w-2.5" />
                  {latest}
                </span>
              )}
            </dd>
          </div>
        ))}
      </dl>

      <div className="flex flex-wrap items-center gap-1.5">
        <button
          id="dlss-update-button"
          type="button"
          onClick={openChoices}
          aria-expanded={choosing}
          disabled={busy}
          className="inline-flex items-center gap-1 rounded-full border border-oled-border bg-oled-bg/60 px-2.5 py-1 text-2xs font-medium text-zinc-300 transition-colors hover:border-zinc-600 hover:text-zinc-100 disabled:opacity-50"
        >
          <span>{t("dlss.update")}</span>
          <ChevronDown className={`h-3 w-3 transition-transform ${choosing ? "rotate-180" : ""}`} />
        </button>
        {replaced && (
          <button
            id="dlss-restore-button"
            type="button"
            onClick={() => void apply(() => dlssRestore(game), "dlss.restored")}
            disabled={busy}
            title={t("dlss.restoreHelp")}
            className="inline-flex items-center gap-1 rounded-full border border-oled-border bg-oled-bg/60 px-2.5 py-1 text-2xs font-medium text-zinc-300 transition-colors hover:border-zinc-600 hover:text-zinc-100 disabled:opacity-50"
          >
            <RotateCcw className="h-3 w-3" />
            <span>{t("dlss.restore")}</span>
          </button>
        )}
      </div>

      {choosing && (
        <div id="dlss-update-panel" className="space-y-1.5">
          {status?.online && latest && (
            <button type="button" data-dlss-source="nvidia" onClick={() => void update({ kind: "nvidia" })} disabled={busy || behind.length === 0} className={choice}>
              <Download className="h-3 w-3 shrink-0 text-accent" />
              <span className="min-w-0 flex-1 truncate">{t("dlss.fromNvidia")}</span>
              <span className="shrink-0 tabular-nums text-zinc-400">{latest}</span>
            </button>
          )}
          {donors.map((donor) => (
            <button
              key={donor.gameId}
              type="button"
              data-dlss-source="game"
              data-donor={donor.gameId}
              onClick={() => void update({ kind: "game", gameId: donor.gameId })}
              disabled={busy}
              className={choice}
              title={t("dlss.fromGameHelp", { title: donor.title })}
            >
              <Gamepad2 className="h-3 w-3 shrink-0" style={{ color: "var(--accent-secondary)" }} />
              <span className="min-w-0 flex-1 truncate">{t("dlss.fromGame", { title: donor.title })}</span>
              <span className="shrink-0 tabular-nums text-zinc-400">{donor.version}</span>
            </button>
          ))}
          <button type="button" data-dlss-source="file" onClick={() => void updateFromFile()} disabled={busy} className={choice}>
            <FileUp className="h-3 w-3 shrink-0 text-zinc-400" />
            <span className="min-w-0 flex-1 truncate">{t("dlss.fromFile")}</span>
          </button>
          {status && !status.online && <p className="text-3xs leading-relaxed text-zinc-500">{t("dlss.nvidiaOff")}</p>}
          {/* The choices first, within reach; what to know about them right under */}
          <p className="text-3xs leading-relaxed text-zinc-500">{t("dlss.warning")}</p>
        </div>
      )}

      {message && (
        <p id="dlss-message" role="status" className={`text-3xs leading-relaxed ${message.failed ? "text-amber-300" : "text-zinc-400"}`}>
          {message.text}
        </p>
      )}
    </div>
  );
  return tile ? <div className="tile shrink-0 p-4">{list}</div> : list;
}
