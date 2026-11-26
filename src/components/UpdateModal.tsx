import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  ArrowRight,
  Calendar,
  Download,
  ExternalLink,
  HardDriveDownload,
  RefreshCw,
  ShieldCheck,
  X,
} from "lucide-react";
import { useI18n } from "../i18n";
import {
  cancelAppUpdate,
  installAppUpdate,
  openReleasePage,
  watchUpdateProgress,
} from "../lib/backend";
import type { AppUpdateInfo, UpdateProgress } from "../types/game";
import { KansoMark } from "./KansoMark";

interface UpdateModalProps {
  update: AppUpdateInfo | null;
  isOpen: boolean;
  /** `false` while another dialog is stacked on top, so Escape closes only that one. */
  isTopmost: boolean;
  onClose: () => void;
  onSkipVersion: (version: string) => void;
}

type NoteBlock =
  | { kind: "heading"; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "paragraph"; text: string };

/**
 * Parses Kanso's CHANGELOG-style markdown into structured blocks without ever injecting HTML.
 * Continuation lines under a bullet (`- **Title.** line one\n  line two`) are folded into the
 * same item so wrapped changelog text reflows cleanly to the dialog width.
 */
function parseReleaseNotes(raw: string): NoteBlock[] {
  const blocks: NoteBlock[] = [];
  const lines = raw.replace(/\r\n/g, "\n").split("\n");
  let list: string[] = [];
  let paragraph: string[] = [];

  const flushList = () => {
    if (list.length > 0) {
      blocks.push({ kind: "list", items: list });
      list = [];
    }
  };
  const flushParagraph = () => {
    if (paragraph.length > 0) {
      blocks.push({ kind: "paragraph", text: paragraph.join(" ") });
      paragraph = [];
    }
  };

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) {
      flushList();
      flushParagraph();
      continue;
    }
    const heading = trimmed.match(/^#{1,4}\s+(.+)$/);
    if (heading) {
      flushList();
      flushParagraph();
      blocks.push({ kind: "heading", text: heading[1].trim() });
      continue;
    }
    const bullet = trimmed.match(/^[-*•]\s+(.+)$/);
    if (bullet) {
      flushParagraph();
      list.push(bullet[1].trim());
      continue;
    }
    // Indented continuation of the previous bullet item
    if (list.length > 0 && /^\s+/.test(line)) {
      list[list.length - 1] = `${list[list.length - 1]} ${trimmed}`;
      continue;
    }
    flushList();
    paragraph.push(trimmed);
  }
  flushList();
  flushParagraph();
  return blocks;
}

/** Renders inline `**bold**`, `*italic*`, `` `code` ``, and `[label](url)` spans as plain React nodes. */
function renderInline(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let key = 0;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    if (token.startsWith("**") && token.endsWith("**")) {
      nodes.push(
        <strong key={key++} className="font-semibold text-zinc-100">
          {token.slice(2, -2)}
        </strong>
      );
    } else if (token.startsWith("*") && token.endsWith("*")) {
      nodes.push(
        <em key={key++} className="italic text-zinc-200">
          {token.slice(1, -1)}
        </em>
      );
    } else if (token.startsWith("`") && token.endsWith("`")) {
      nodes.push(
        <code
          key={key++}
          className="rounded border border-oled-border bg-oled-elevated px-1 py-0.5 font-mono text-2xs text-zinc-200"
        >
          {token.slice(1, -1)}
        </code>
      );
    } else if (token.startsWith("[")) {
      const label = token.slice(1, token.indexOf("]("));
      nodes.push(
        <span key={key++} className="font-medium text-zinc-200">
          {label}
        </span>
      );
    }
    lastIndex = match.index + token.length;
  }

  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }
  return nodes;
}

/**
 * In-app update dialog: follows the active theme's surfaces, accents, typography, corner
 * radii, glass/cut panel styling, motion and accessibility preferences.
 */
export function UpdateModal({
  update,
  isOpen,
  isTopmost,
  onClose,
  onSkipVersion,
}: UpdateModalProps) {
  const { t, lang, number } = useI18n();
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [isUpdating, setIsUpdating] = useState(false);
  const [isCancelling, setIsCancelling] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const cancelledByUserRef = useRef(false);

  // Reset transient state only when the dialog opens or switches to another version
  useEffect(() => {
    if (isOpen) {
      setErrorMsg(null);
      setProgress(null);
      setIsCancelling(false);
      cancelledByUserRef.current = false;
    }
  }, [isOpen, update?.version]);

  useEffect(() => {
    if (!isOpen) return;
    let active = true;
    let stop: (() => void) | null = null;
    void watchUpdateProgress((next) => {
      if (active) setProgress(next);
    }).then((unlisten) => {
      if (active) stop = unlisten;
      else unlisten();
    });
    return () => {
      active = false;
      stop?.();
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen || !isTopmost) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !isUpdating) onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, isTopmost, isUpdating, onClose]);

  if (!isOpen || !update) return null;

  const formatMb = (bytes: number) =>
    t("update.sizeMb", { size: number(bytes / (1024 * 1024), 1) });

  const publishedLabel = (() => {
    if (!update.publishedAt) return null;
    const parsed = Date.parse(update.publishedAt);
    if (Number.isNaN(parsed)) return null;
    try {
      return new Intl.DateTimeFormat(lang, {
        day: "numeric",
        month: "short",
        year: "numeric",
      }).format(new Date(parsed));
    } catch {
      return null;
    }
  })();

  const handleInstall = async () => {
    cancelledByUserRef.current = false;
    setIsUpdating(true);
    setIsCancelling(false);
    setErrorMsg(null);
    setProgress({
      phase: "downloading",
      downloaded: 0,
      total: update.assetSize > 0 ? update.assetSize : null,
    });
    try {
      await installAppUpdate(update.version);
    } catch (err) {
      const raw = err instanceof Error ? err.message : typeof err === "string" ? err : "";
      if (cancelledByUserRef.current || raw === "CANCELLED") {
        setErrorMsg(null);
      } else if (raw === "PREVIEW_ONLY") {
        setErrorMsg(t("update.previewOnly"));
      } else {
        setErrorMsg(raw || t("update.failed"));
      }
      cancelledByUserRef.current = false;
      setIsUpdating(false);
      setIsCancelling(false);
      setProgress(null);
    }
  };

  const handleCancel = async () => {
    if (!isUpdating || progress?.phase === "installing") return;
    cancelledByUserRef.current = true;
    setIsCancelling(true);
    try {
      await cancelAppUpdate();
    } catch {
      // The download command reports cancellation when it stops
    }
  };

  const handleOpenGitHub = () => {
    void openReleasePage().catch(() => {});
  };

  const noteBlocks = parseReleaseNotes(update.notes);
  const phase = progress?.phase ?? null;
  const totalBytes = progress?.total ?? (update.assetSize > 0 ? update.assetSize : null);
  const downloadedBytes = progress?.downloaded ?? 0;
  const percent =
    totalBytes && totalBytes > 0
      ? Math.min(100, Math.round((downloadedBytes / totalBytes) * 100))
      : null;
  const indeterminate =
    isUpdating && (phase === "verifying" || phase === "installing" || percent === null);

  const kickerText =
    phase === "downloading"
      ? t("update.kickerDownloading")
      : phase === "verifying"
        ? t("update.kickerVerifying")
        : phase === "installing"
          ? t("update.kickerInstalling")
          : t("update.kicker");

  const progressLabel =
    phase === "verifying"
      ? t("update.verifying")
      : phase === "installing"
        ? t("update.installing")
        : t("update.downloading");

  return (
    <div
      className="below-header fixed inset-x-0 bottom-0 z-[55] flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm motion-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !isUpdating) onClose();
      }}
    >
      <div
        id="update-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-modal-title"
        className="update-sheet motion-sheet flex max-h-full w-full max-w-lg flex-col overflow-hidden rounded-xl border border-oled-border bg-oled-surface shadow-2xl"
      >
        {/* Themed Hero Header */}
        <div className="update-hero relative flex shrink-0 items-start justify-between gap-4 border-b border-oled-border px-6 py-5">
          <div className="flex min-w-0 items-center gap-3.5">
            <div className="update-emblem flex h-12 w-12 shrink-0 items-center justify-center rounded-lg border border-oled-border bg-oled-bg shadow-sm">
              <KansoMark decorative className="h-7 w-7 text-zinc-100" />
            </div>
            <div className="min-w-0">
              <span className="micro-label block text-accent">{kickerText}</span>
              <h2
                id="update-modal-title"
                className="mt-0.5 truncate text-base font-semibold text-zinc-100"
              >
                {update.name || t("update.title")}
              </h2>
              <p className="mt-0.5 text-2xs leading-relaxed text-zinc-400">
                {t("update.subtitle")}
              </p>
            </div>
          </div>

          {!isUpdating && (
            <button
              id="update-modal-close"
              type="button"
              onClick={onClose}
              aria-label={t("common.close")}
              className="rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-200"
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>

        {/* Scrollable Content */}
        <div className="scroll-area min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-5">
          {/* Version upgrade strip */}
          <div
            aria-label={t("update.versionFromTo")}
            className="update-card flex flex-wrap items-center justify-between gap-3 rounded-lg border border-oled-border bg-oled-bg px-4 py-3"
          >
            <div className="flex items-center gap-2.5">
              <div className="flex flex-col">
                <span className="text-3xs uppercase tracking-wider text-zinc-500">
                  {t("update.currentVersion")}
                </span>
                <span className="font-mono text-xs font-medium tabular-nums text-zinc-300">
                  v{update.currentVersion}
                </span>
              </div>
              <ArrowRight className="h-4 w-4 shrink-0 text-accent" aria-hidden="true" />
              <div className="flex flex-col">
                <span className="text-3xs uppercase tracking-wider text-zinc-500">
                  {t("update.newVersion")}
                </span>
                <span className="inline-flex items-center rounded-md border border-accent/40 bg-accent/15 px-2 py-0.5 font-mono text-xs font-semibold tabular-nums text-accent">
                  v{update.version}
                </span>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-3 text-2xs text-zinc-400">
              {publishedLabel && (
                <span className="inline-flex items-center gap-1.5">
                  <Calendar className="h-3 w-3 text-zinc-500" aria-hidden="true" />
                  <span>{publishedLabel}</span>
                </span>
              )}
              {update.assetSize > 0 && (
                <span className="inline-flex items-center gap-1.5 tabular-nums">
                  <HardDriveDownload className="h-3 w-3 text-zinc-500" aria-hidden="true" />
                  <span>{formatMb(update.assetSize)}</span>
                </span>
              )}
            </div>
          </div>

          {/* Release notes */}
          <section className="space-y-2">
            <div className="flex items-center justify-between gap-2">
              <h3 className="card-title">{t("update.whatsNew")}</h3>
              <button
                id="update-modal-github-link"
                type="button"
                onClick={handleOpenGitHub}
                className="inline-flex items-center gap-1 text-2xs text-zinc-400 transition-colors hover:text-zinc-100"
              >
                <span>{t("update.viewOnGitHub")}</span>
                <ExternalLink className="h-2.5 w-2.5" aria-hidden="true" />
              </button>
            </div>
            <div className="update-card scroll-area max-h-52 overflow-y-auto rounded-lg border border-oled-border bg-oled-bg px-4 py-3.5">
              {noteBlocks.length === 0 ? (
                <p className="text-xs leading-relaxed text-zinc-400">{t("update.noNotes")}</p>
              ) : (
                <div className="space-y-3 text-xs leading-relaxed text-zinc-300">
                  {noteBlocks.map((block, idx) => {
                    if (block.kind === "heading") {
                      return (
                        <h4
                          key={idx}
                          className="text-xs font-semibold tracking-tight text-zinc-100"
                        >
                          {renderInline(block.text)}
                        </h4>
                      );
                    }
                    if (block.kind === "list") {
                      return (
                        <ul key={idx} className="space-y-2">
                          {block.items.map((item, itemIdx) => (
                            <li key={itemIdx} className="flex items-start gap-2.5">
                              <span
                                aria-hidden="true"
                                className="mt-[0.42rem] h-1.5 w-1.5 shrink-0 rounded-full bg-accent"
                              />
                              <span className="min-w-0 flex-1">{renderInline(item)}</span>
                            </li>
                          ))}
                        </ul>
                      );
                    }
                    return <p key={idx}>{renderInline(block.text)}</p>;
                  })}
                </div>
              )}
            </div>
          </section>

          {/* Progress bar while downloading / verifying / installing */}
          {isUpdating && (
            <section
              id="update-progress-panel"
              role="status"
              aria-live="polite"
              className="update-card space-y-2.5 rounded-lg border border-accent/35 bg-oled-bg px-4 py-3"
            >
              <div className="flex items-center justify-between gap-3 text-xs">
                <span className="font-medium text-zinc-100">{progressLabel}</span>
                {phase === "downloading" && totalBytes && totalBytes > 0 && (
                  <span className="shrink-0 font-mono text-2xs tabular-nums text-zinc-400">
                    {formatMb(downloadedBytes)} / {formatMb(totalBytes)} · {percent}%
                  </span>
                )}
              </div>
              <div
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={indeterminate ? undefined : (percent ?? 0)}
                className="h-2 w-full overflow-hidden rounded-full bg-oled-elevated"
              >
                <div
                  className={`h-full rounded-full bg-accent ${
                    indeterminate
                      ? "update-progress-indeterminate w-2/5"
                      : "transition-[width] duration-150"
                  }`}
                  style={indeterminate ? undefined : { width: `${percent ?? 0}%` }}
                />
              </div>
              {phase === "installing" && (
                <p className="text-2xs leading-relaxed text-zinc-400">
                  {t("update.restartingHint")}
                </p>
              )}
            </section>
          )}

          {/* Error message */}
          {errorMsg && (
            <div
              id="update-error-banner"
              role="alert"
              className="select-text rounded-lg border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-xs leading-relaxed text-red-200"
            >
              {errorMsg}
            </div>
          )}

          {/* Subtle SHA-256 verification assurance */}
          {!isUpdating && (
            <p className="flex items-start gap-2 text-2xs leading-relaxed text-zinc-500">
              <ShieldCheck className="mt-0.5 h-3.5 w-3.5 shrink-0 text-accent" aria-hidden="true" />
              <span>{t("update.verifiedNote")}</span>
            </p>
          )}
        </div>

        {/* Footer actions */}
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-t border-oled-border px-6 py-4">
          {!isUpdating ? (
            <button
              id="update-skip-button"
              type="button"
              onClick={() => onSkipVersion(update.version)}
              className="text-2xs text-zinc-400 underline-offset-2 transition-colors hover:text-zinc-200 hover:underline"
            >
              {t("update.skipVersion")}
            </button>
          ) : (
            <span className="text-2xs text-zinc-500">{update.assetName}</span>
          )}

          <div className="ml-auto flex items-center gap-2.5">
            {isUpdating ? (
              <button
                id="update-cancel-button"
                type="button"
                disabled={isCancelling || phase === "verifying" || phase === "installing"}
                onClick={() => void handleCancel()}
                className="rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated disabled:cursor-not-allowed disabled:opacity-50"
              >
                {t("common.cancel")}
              </button>
            ) : (
              <>
                <button
                  id="update-later-button"
                  type="button"
                  onClick={onClose}
                  className="rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
                >
                  {t("update.later")}
                </button>
                <button
                  id="update-install-button"
                  type="button"
                  onClick={() => void handleInstall()}
                  className="inline-flex items-center gap-2 rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover"
                >
                  {errorMsg ? (
                    <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
                  ) : (
                    <Download className="h-3.5 w-3.5" aria-hidden="true" />
                  )}
                  <span>{errorMsg ? t("update.retry") : t("update.installNow")}</span>
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
