import { useEffect, useLayoutEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { ChevronLeft, ChevronRight, Pause, Pin, PinOff, Play } from "lucide-react";
import { useI18n } from "../i18n";

/** One of the cards of a game's sheet that come after its figures and its achievements. */
export interface DetailCard {
  /** Stable name of the card: `about`, `activity`, `files`, `plugin:<id>`. */
  key: string;
  title: string;
  content: ReactNode;
  /** A small control shown next to the title (a plugin's refresh button). */
  action?: ReactNode;
}

const PINNED_PREF = "kanso.ui.detail.pinned";
const MAX_PINNED = 24;

/** The cards the user keeps in sight on every game's sheet, in the order they were pinned. */
export function readPinnedCards(): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(PINNED_PREF) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return [...new Set(parsed.filter((key): key is string => typeof key === "string" && key.length <= 80))].slice(
      0,
      MAX_PINNED
    );
  } catch {
    return [];
  }
}

export function writePinnedCards(keys: string[]): void {
  try {
    localStorage.setItem(PINNED_PREF, JSON.stringify(keys.slice(0, MAX_PINNED)));
  } catch {
    // The choice simply won't be remembered
  }
}

/** How long a card stays before the rail moves on, by how much the interface moves. */
const STAY_MS: Record<string, number> = { subtle: 9000, smooth: 7000, lively: 5000 };
/** The narrowest a card of the rail gets, in rem: below it, one card fewer is shown. */
const MIN_CARD_REM = 19;

/** A click on something that has its own job (a link, a button, a bar of a chart) is not a pin. */
function isOwnControl(target: EventTarget | null, card: HTMLElement): boolean {
  if (!(target instanceof Element)) return false;
  const control = target.closest("a, button, input, select, textarea, summary, [role='button'], [tabindex]");
  return control !== null && control !== card && card.contains(control);
}

interface CardRailProps {
  cards: DetailCard[];
  /** Keeps a card in sight: it leaves the rail and becomes a tile of the sheet. */
  onPin: (key: string) => void;
}

/**
 * The cards of a game's sheet that are not shown at once, side by side in a box that moves on
 * by one card every few seconds. It stops while the pointer or the keyboard is on it, and for
 * good when the user pauses it; with the animations off it never moves by itself, and the
 * arrows show the next cards. A click on a card (or its pin) keeps it in sight.
 *
 * How it moves follows the animations chosen in Settings: how long a card stays, how long the
 * step takes and with which easing.
 */
export function CardRail({ cards, onPin }: CardRailProps) {
  const { t } = useI18n();
  const viewportRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [index, setIndex] = useState(0);
  const [engaged, setEngaged] = useState(false);
  const [paused, setPaused] = useState(false);

  // How many cards fit side by side, and how wide each one is
  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const measure = () => setWidth(viewport.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(viewport);
    return () => observer.disconnect();
  }, []);

  const rem = typeof document === "undefined" ? 16 : parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  const gap = 0.75 * rem;
  const perView = Math.max(1, Math.min(cards.length, Math.floor((width + gap) / (MIN_CARD_REM * rem + gap))));
  const cardWidth = width > 0 ? (width - gap * (perView - 1)) / perView : 0;
  const lastIndex = Math.max(0, cards.length - perView);
  const shownIndex = Math.min(index, lastIndex);
  const moves = cards.length > perView;

  // Moves on by itself, unless something says not to
  useEffect(() => {
    if (!moves || engaged || paused) return;
    const level = document.documentElement.dataset.motion ?? "none";
    const stay = STAY_MS[level];
    if (!stay) return;
    const timer = window.setTimeout(() => {
      if (!document.hidden) setIndex(shownIndex >= lastIndex ? 0 : shownIndex + 1);
    }, stay);
    return () => window.clearTimeout(timer);
  }, [moves, engaged, paused, shownIndex, lastIndex]);

  if (cards.length === 0) return null;

  const step = (by: number) => setIndex(Math.max(0, Math.min(lastIndex, shownIndex + by)));
  const pinFromCard = (event: MouseEvent<HTMLElement>, key: string) => {
    if (isOwnControl(event.target, event.currentTarget)) return;
    // Selecting a line of text ends with a click too
    if ((window.getSelection()?.toString() ?? "") !== "") return;
    onPin(key);
  };
  const control =
    "rounded-md p-1 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-100 disabled:opacity-30 disabled:hover:bg-transparent";

  return (
    <section
      id="detail-rail"
      aria-roledescription="carousel"
      aria-label={t("detail.more")}
      className="card-rail shrink-0"
      onMouseEnter={() => setEngaged(true)}
      onMouseLeave={() => setEngaged(false)}
      onFocus={() => setEngaged(true)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setEngaged(false);
      }}
    >
      <div className="flex items-center justify-between gap-3 px-1 pb-2.5">
        <div className="flex min-w-0 items-baseline gap-2.5">
          <h3 className="micro-label shrink-0">{t("detail.more")}</h3>
          <span className="truncate text-2xs text-zinc-500">{t("detail.pinHint")}</span>
        </div>
        {moves && (
          <div className="flex shrink-0 items-center gap-0.5">
            <button
              id="rail-pause-button"
              type="button"
              onClick={() => setPaused(!paused)}
              aria-pressed={paused}
              aria-label={t(paused ? "detail.railPlay" : "detail.railPause")}
              title={t(paused ? "detail.railPlay" : "detail.railPause")}
              className={`rail-pause ${control}`}
            >
              {paused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
            </button>
            <button
              id="rail-previous-button"
              type="button"
              onClick={() => step(-1)}
              disabled={shownIndex === 0}
              aria-label={t("detail.railPrevious")}
              title={t("detail.railPrevious")}
              className={control}
            >
              <ChevronLeft className="h-3.5 w-3.5" />
            </button>
            <span id="rail-position" className="min-w-[3.25rem] text-center text-2xs tabular-nums text-zinc-500">
              {perView > 1
                ? t("detail.railPosition", { from: shownIndex + 1, to: shownIndex + perView, total: cards.length })
                : t("detail.railPositionOne", { from: shownIndex + 1, total: cards.length })}
            </span>
            <button
              id="rail-next-button"
              type="button"
              onClick={() => step(1)}
              disabled={shownIndex >= lastIndex}
              aria-label={t("detail.railNext")}
              title={t("detail.railNext")}
              className={control}
            >
              <ChevronRight className="h-3.5 w-3.5" />
            </button>
          </div>
        )}
      </div>

      <div ref={viewportRef} className="rail-viewport">
        <div
          className="rail-track"
          style={{ gap: `${gap}px`, transform: `translateX(${-shownIndex * (cardWidth + gap)}px)` }}
        >
          {cards.map((card, position) => {
            const inSight = position >= shownIndex && position < shownIndex + perView;
            return (
              <article
                key={card.key}
                data-card={card.key}
                data-in-sight={inSight ? "" : undefined}
                aria-hidden={inSight ? undefined : true}
                // Cards out of sight are not there for the keyboard either
                ref={(element) => {
                  if (element) element.inert = !inSight;
                }}
                onClick={(event) => pinFromCard(event, card.key)}
                className="rail-card"
                style={{ width: cardWidth > 0 ? `${cardWidth}px` : undefined }}
              >
                <header className="flex items-center justify-between gap-2">
                  <h4 className="card-title min-w-0 truncate">{card.title}</h4>
                  <div className="flex shrink-0 items-center gap-0.5">
                    {card.action}
                    <button
                      type="button"
                      data-pin={card.key}
                      onClick={() => onPin(card.key)}
                      aria-label={t("detail.pin", { title: card.title })}
                      title={t("detail.pin", { title: card.title })}
                      className="rail-pin rounded p-1 text-zinc-500 transition-colors hover:text-zinc-100"
                    >
                      <Pin className="h-3 w-3" />
                    </button>
                  </div>
                </header>
                <div className="rail-card-body scroll-area">{card.content}</div>
              </article>
            );
          })}
        </div>
      </div>
    </section>
  );
}

interface PinnedCardProps {
  card: DetailCard;
  /** Gives the card back to the rail; absent where cards cannot be pinned at all. */
  onUnpin?: (key: string) => void;
}

/** A card kept in sight: a tile of the sheet, whole, with the way back to the rail. */
export function PinnedCard({ card, onUnpin }: PinnedCardProps) {
  const { t } = useI18n();
  return (
    <section
      className="tile pinned-card shrink-0 space-y-2.5 p-4"
      data-card={card.key}
      data-rank={onUnpin ? "pinned" : "card"}
    >
      <header className="flex items-center justify-between gap-2">
        <h3 className="card-title min-w-0 truncate">{card.title}</h3>
        <div className="flex shrink-0 items-center gap-0.5">
          {card.action}
          {onUnpin && (
            <button
              type="button"
              data-unpin={card.key}
              onClick={() => onUnpin(card.key)}
              aria-label={t("detail.unpin", { title: card.title })}
              title={t("detail.unpin", { title: card.title })}
              className="rounded p-1 text-zinc-400 transition-colors hover:text-zinc-100"
            >
              <PinOff className="h-3 w-3" />
            </button>
          )}
        </div>
      </header>
      {card.content}
    </section>
  );
}
