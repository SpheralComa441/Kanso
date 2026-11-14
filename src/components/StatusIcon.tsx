import { Ban, Bookmark, CircleCheck, Gamepad2, type LucideIcon } from "lucide-react";
import type { MessageKey } from "../i18n";
import type { GameStatus } from "../types/game";

export const STATUS_LABEL_KEYS: Record<GameStatus, MessageKey> = {
  backlog: "status.backlog",
  playing: "status.playing",
  finished: "status.finished",
  abandoned: "status.abandoned",
};

const STATUS_ICONS: Record<GameStatus, LucideIcon> = {
  backlog: Bookmark,
  playing: Gamepad2,
  finished: CircleCheck,
  abandoned: Ban,
};

interface StatusIconProps {
  status: GameStatus;
  className?: string;
}

/** The glyph that stands for a game status wherever there is no room for its name. */
export function StatusIcon({ status, className = "h-3.5 w-3.5" }: StatusIconProps) {
  const Icon = STATUS_ICONS[status];
  return <Icon className={className} aria-hidden="true" />;
}
