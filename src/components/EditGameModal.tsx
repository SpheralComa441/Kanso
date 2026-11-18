import { useEffect, useState } from "react";
import {
  Check,
  CheckCircle2,
  FolderOpen,
  ImagePlus,
  Loader2,
  Search,
  Sparkles,
  X,
} from "lucide-react";
import {
  applySteamGridDbMatch,
  isRunningInTauri,
  pickCoverImageFile,
  pickExecutableFile,
  resolveCoverUrl,
  searchSteamGridDbGames,
  setGameLocalCover,
  updateGameDetails,
} from "../lib/backend";
import { useI18n } from "../i18n";
import { platformLabel, sourceOf } from "../lib/platforms";
import type { DetectedGame, SgdbGameMatch } from "../types/game";

interface EditGameModalProps {
  game: DetectedGame | null;
  hasApiKey: boolean;
  onClose: () => void;
  onGameUpdated: (updated: DetectedGame) => void;
  onOpenSettings: () => void;
}

export function EditGameModal({
  game,
  hasApiKey,
  onClose,
  onGameUpdated,
  onOpenSettings,
}: EditGameModalProps) {
  const { t } = useI18n();
  const [title, setTitle] = useState("");
  const [exePath, setExePath] = useState("");
  const [genresText, setGenresText] = useState("");
  const [description, setDescription] = useState("");

  // "Cerca su SteamGridDB" state
  const [sgdbQuery, setSgdbQuery] = useState("");
  const [sgdbResults, setSgdbResults] = useState<SgdbGameMatch[]>([]);
  const [isSearchingSgdb, setIsSearchingSgdb] = useState(false);
  const [applyingSgdbId, setApplyingSgdbId] = useState<number | null>(null);
  const [hasSearchedOnce, setHasSearchedOnce] = useState(false);

  const [isSaving, setIsSaving] = useState(false);
  const [feedbackMsg, setFeedbackMsg] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);

  useEffect(() => {
    if (game) {
      setTitle(game.title);
      setExePath(game.executablePath ?? "");
      setGenresText(game.genres.join(", "));
      setDescription(game.shortDescription ?? "");
      setSgdbQuery(game.cleanTitle || game.title);
      setSgdbResults([]);
      setHasSearchedOnce(false);
      setFeedbackMsg(null);
    }
  }, [game?.id]);

  useEffect(() => {
    if (!game) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [game, onClose]);

  if (!game) return null;

  const coverUrl = resolveCoverUrl(game.coverPath, game.updatedAt);

  const handleSearchSgdb = async () => {
    const q = sgdbQuery.trim();
    if (!q) return;
    setIsSearchingSgdb(true);
    setFeedbackMsg(null);
    setHasSearchedOnce(true);
    try {
      const matches = await searchSteamGridDbGames(q);
      setSgdbResults(matches);
    } catch (err) {
      setFeedbackMsg({
        type: "error",
        text:
          typeof err === "string" ? err : t("edit.sgdbSearchError"),
      });
    } finally {
      setIsSearchingSgdb(false);
    }
  };

  const handleSelectSgdbMatch = async (match: SgdbGameMatch) => {
    setApplyingSgdbId(match.id);
    setFeedbackMsg(null);
    try {
      const updated = await applySteamGridDbMatch(
        game.id,
        match.id,
        match.name
      );
      setTitle(updated.title);
      setGenresText(updated.genres.join(", "));
      setDescription(updated.shortDescription ?? "");
      onGameUpdated(updated);
      setFeedbackMsg({
        type: "success",
        text: t("edit.sgdbApplied", { name: match.name }),
      });
    } catch (err) {
      setFeedbackMsg({
        type: "error",
        text:
          typeof err === "string" ? err : t("edit.sgdbApplyError"),
      });
    } finally {
      setApplyingSgdbId(null);
    }
  };

  const handlePickLocalCover = async () => {
    setFeedbackMsg(null);
    try {
      const picked = await pickCoverImageFile();
      if (!picked) return;
      const updated = await setGameLocalCover(game.id, picked);
      onGameUpdated(updated);
      setFeedbackMsg({
        type: "success",
        text: t("edit.localCoverOk"),
      });
    } catch (err) {
      setFeedbackMsg({
        type: "error",
        text:
          typeof err === "string" ? err : t("edit.localCoverError"),
      });
    }
  };

  const handleBrowseExe = async () => {
    try {
      const picked = await pickExecutableFile();
      if (picked) {
        setExePath(picked);
      }
    } catch (err) {
      setFeedbackMsg({
        type: "error",
        text:
          typeof err === "string" ? err : t("edit.exePickError"),
      });
    }
  };

  const handleSaveManual = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSaving(true);
    setFeedbackMsg(null);
    try {
      const parsedGenres = genresText
        .split(",")
        .map((g) => g.trim())
        .filter(Boolean);

      const updated = await updateGameDetails({
        id: game.id,
        title: title.trim() || game.title,
        executablePath: exePath.trim() || null,
        shortDescription: description.trim() || null,
        genres: parsedGenres,
      });
      onGameUpdated(updated);
      onClose();
    } catch (err) {
      setFeedbackMsg({
        type: "error",
        text:
          typeof err === "string" ? err : t("edit.saveError"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div
      className="below-header fixed inset-x-0 bottom-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm motion-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="edit-game-modal-title"
        className="motion-sheet flex max-h-full w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-oled-border bg-oled-surface shadow-2xl"
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-oled-border px-6 py-4">
          <div className="min-w-0">
            <h2
              id="edit-game-modal-title"
              className="truncate text-base font-semibold text-zinc-100"
            >
              {t("edit.title", { title: game.title })}
            </h2>
            <p className="mt-0.5 text-xs text-zinc-400">
              {t("edit.platform")}:{" "}
              <strong className="text-zinc-300">
                {platformLabel(sourceOf(game), t)}
              </strong>
              {game.platformAppId ? ` · ID: ${game.platformAppId}` : ""}
            </p>
          </div>
          <button
            id="close-edit-game-button"
            type="button"
            onClick={onClose}
            aria-label={t("edit.close")}
            className="rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-200"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="flex-1 scroll-area overflow-y-auto px-6 py-5 space-y-6">
          {feedbackMsg && (
            <div
              className={`rounded-lg border px-3.5 py-2.5 text-xs ${
                feedbackMsg.type === "success"
                  ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-200"
                  : "border-red-500/30 bg-red-500/10 text-red-200"
              }`}
            >
              {feedbackMsg.text}
            </div>
          )}

          {/* Top layout: 2:3 Cover Preview + "Cerca su SteamGridDB" */}
          <div className="flex flex-col gap-5 sm:flex-row">
            {/* Cover preview */}
            <div className="flex shrink-0 flex-col items-center gap-2 sm:w-36">
              <div className="relative aspect-[2/3] w-32 sm:w-36 overflow-hidden rounded-card border border-oled-border bg-oled-bg">
                {coverUrl ? (
                  <img
                    src={coverUrl}
                    alt={game.title}
                    className="h-full w-full object-cover"
                  />
                ) : (
                  <div className="flex h-full w-full flex-col items-center justify-center p-3 text-center">
                    <span className="text-xs font-medium text-zinc-400 line-clamp-3">
                      {game.title}
                    </span>
                    <span className="mt-1 text-3xs text-zinc-500">
                      {t("detail.noCover")}
                    </span>
                  </div>
                )}
                {applyingSgdbId !== null && (
                  <div className="absolute inset-0 flex items-center justify-center bg-black/70">
                    <Loader2 className="h-5 w-5 animate-spin text-accent" />
                  </div>
                )}
              </div>

              {isRunningInTauri() && (
                <button
                  id="pick-local-cover-button"
                  type="button"
                  onClick={handlePickLocalCover}
                  className="inline-flex w-full items-center justify-center gap-1.5 rounded-lg border border-oled-border bg-oled-elevated px-2.5 py-1.5 text-2xs font-medium text-zinc-200 transition-colors hover:border-zinc-600"
                >
                  <ImagePlus className="h-3.5 w-3.5 text-accent" />
                  <span>{t("edit.localFile")}</span>
                </button>
              )}
            </div>

            {/* "Cerca su SteamGridDB" live search & picker */}
            <div className="flex-1 space-y-2.5">
              <div className="flex items-center justify-between">
                <label
                  htmlFor="sgdb-manual-search-input"
                  className="flex items-center gap-1.5 text-xs font-semibold text-zinc-100"
                >
                  <Sparkles className="h-3.5 w-3.5 text-accent" />
                  <span>{t("edit.sgdbSearch")}</span>
                </label>
                {game.steamgriddbId && (
                  <span className="font-mono text-2xs text-zinc-500">
                    SGDB #{game.steamgriddbId}
                  </span>
                )}
              </div>

              {!hasApiKey && isRunningInTauri() ? (
                <div className="rounded-lg border border-oled-border bg-oled-bg p-3.5 text-xs text-zinc-400 space-y-2">
                  <p>{t("edit.sgdbNeedsKey")}</p>
                  <button
                    type="button"
                    onClick={() => {
                      onClose();
                      onOpenSettings();
                    }}
                    className="rounded-md bg-accent/15 px-2.5 py-1 font-medium text-accent hover:bg-accent/25"
                  >
                    {t("edit.configureKey")}
                  </button>
                </div>
              ) : (
                <>
                  <div className="flex gap-2">
                    <div className="relative flex-1">
                      <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-zinc-500" />
                      <input
                        id="sgdb-manual-search-input"
                        type="text"
                        value={sgdbQuery}
                        onChange={(e) => setSgdbQuery(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            void handleSearchSgdb();
                          }
                        }}
                        placeholder={t("edit.sgdbPlaceholder")}
                        className="w-full rounded-lg border border-oled-border bg-oled-bg py-1.5 pl-9 pr-3 text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
                      />
                    </div>
                    <button
                      id="sgdb-search-submit-button"
                      type="button"
                      onClick={() => void handleSearchSgdb()}
                      disabled={isSearchingSgdb || !sgdbQuery.trim()}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-elevated px-3 py-1.5 text-xs font-medium text-zinc-100 transition-colors hover:border-zinc-600 disabled:opacity-50"
                    >
                      {isSearchingSgdb ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin text-accent" />
                      ) : (
                        <Search className="h-3.5 w-3.5 text-accent" />
                      )}
                      <span>{t("common.search")}</span>
                    </button>
                  </div>

                  <p className="text-2xs text-zinc-500">
                    {t("edit.sgdbHint")}
                  </p>

                  {sgdbResults.length > 0 ? (
                    <ul className="max-h-44 divide-y divide-oled-border scroll-area overflow-y-auto rounded-lg border border-oled-border bg-oled-bg">
                      {sgdbResults.map((item) => {
                        const isApplyingThis = applyingSgdbId === item.id;
                        const isCurrent = game.steamgriddbId === item.id;

                        return (
                          <li key={item.id}>
                            <button
                              id={`sgdb-result-${item.id}`}
                              type="button"
                              disabled={applyingSgdbId !== null}
                              onClick={() => void handleSelectSgdbMatch(item)}
                              className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-xs transition-colors hover:bg-oled-elevated disabled:opacity-60"
                            >
                              <div className="min-w-0 flex-1">
                                <div className="flex items-center gap-1.5">
                                  <span className="truncate font-medium text-zinc-100">
                                    {item.name}
                                  </span>
                                  {item.releaseYear && (
                                    <span className="shrink-0 text-2xs text-zinc-500">
                                      ({item.releaseYear})
                                    </span>
                                  )}
                                  {item.verified && (
                                    <span
                                      title={t("edit.sgdbVerified")}
                                      className="inline-flex shrink-0"
                                    >
                                      <CheckCircle2 className="h-3 w-3 text-emerald-400" />
                                    </span>
                                  )}
                                </div>
                                <span className="font-mono text-3xs text-zinc-500">
                                  ID: {item.id}
                                </span>
                              </div>

                              <span className="shrink-0 rounded-md border border-oled-border bg-oled-surface px-2 py-1 text-2xs font-medium text-accent">
                                {isApplyingThis
                                  ? t("edit.sgdbDownloading")
                                  : isCurrent
                                    ? t("edit.sgdbCurrent")
                                    : t("edit.sgdbApply")}
                              </span>
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  ) : (
                    hasSearchedOnce &&
                    !isSearchingSgdb && (
                      <div className="rounded-lg border border-dashed border-oled-border bg-oled-bg/50 px-3 py-3 text-center text-xs text-zinc-500">
                        {t("edit.sgdbNoResults", { query: sgdbQuery })}
                      </div>
                    )
                  )}
                </>
              )}
            </div>
          </div>

          {/* Manual metadata & executable editing */}
          <form
            id="edit-game-manual-form"
            onSubmit={handleSaveManual}
            className="space-y-4 border-t border-oled-border pt-5"
          >
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div className="space-y-1.5">
                <label
                  htmlFor="edit-game-title-input"
                  className="block text-xs font-medium text-zinc-300"
                >
                  {t("edit.displayTitle")}
                </label>
                <input
                  id="edit-game-title-input"
                  type="text"
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  className="w-full rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs text-zinc-100 focus:border-accent/60 focus:outline-none"
                />
              </div>

              <div className="space-y-1.5">
                <label
                  htmlFor="edit-game-genres-input"
                  className="block text-xs font-medium text-zinc-300"
                >
                  {t("edit.genres")}
                </label>
                <input
                  id="edit-game-genres-input"
                  type="text"
                  value={genresText}
                  onChange={(e) => setGenresText(e.target.value)}
                  placeholder={t("edit.genresPlaceholder")}
                  className="w-full rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
                />
              </div>
            </div>

            <div className="space-y-1.5">
              <label
                htmlFor="edit-game-exe-input"
                className="block text-xs font-medium text-zinc-300"
              >
                {t("edit.exePath")}
              </label>
              <div className="flex gap-2">
                <input
                  id="edit-game-exe-input"
                  type="text"
                  value={exePath}
                  onChange={(e) => setExePath(e.target.value)}
                  placeholder={
                    game.launchUri
                      ? t("edit.exePlaceholderUri", { uri: game.launchUri })
                      : t("edit.exePlaceholder")
                  }
                  className="flex-1 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
                />
                {isRunningInTauri() && (
                  <button
                    id="browse-game-exe-button"
                    type="button"
                    onClick={() => void handleBrowseExe()}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-elevated px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-600"
                  >
                    <FolderOpen className="h-3.5 w-3.5 text-accent" />
                    <span>{t("common.browse")}</span>
                  </button>
                )}
              </div>
            </div>

            <div className="space-y-1.5">
              <label
                htmlFor="edit-game-description-input"
                className="block text-xs font-medium text-zinc-300"
              >
                {t("edit.description")}
              </label>
              <textarea
                id="edit-game-description-input"
                rows={3}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t("edit.descriptionPlaceholder")}
                className="w-full resize-none rounded-lg border border-oled-border bg-oled-bg px-3 py-2 text-xs leading-relaxed text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
              />
            </div>
          </form>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2.5 border-t border-oled-border px-6 py-4">
          <button
            id="cancel-edit-game-button"
            type="button"
            onClick={onClose}
            className="rounded-lg border border-oled-border bg-oled-bg px-3.5 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
          >
            {t("common.close")}
          </button>
          <button
            id="save-edit-game-button"
            type="submit"
            form="edit-game-manual-form"
            disabled={isSaving}
            className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:opacity-60"
          >
            <Check className="h-3.5 w-3.5" />
            <span>{isSaving ? t("edit.saving") : t("edit.saveChanges")}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
