import { X } from "lucide-react";
import { useI18n } from "../i18n";

interface FilterChipProps {
  label: string;
  onRemove: () => void;
  icon?: React.ReactNode;
  id?: string;
}

/** An active filter, in sight above what it narrows: one click removes it. */
export function FilterChip({ label, onRemove, icon, id }: FilterChipProps) {
  const { t } = useI18n();
  return (
    <button
      id={id}
      type="button"
      onClick={onRemove}
      title={t("filters.remove", { label })}
      className="group inline-flex items-center gap-1.5 rounded-full border border-oled-border bg-oled-surface py-0.5 pl-2.5 pr-1.5 text-2xs font-medium text-zinc-200 transition-colors hover:border-zinc-600"
    >
      {icon}
      <span>{label}</span>
      <X className="h-3 w-3 text-zinc-500 group-hover:text-zinc-200" />
    </button>
  );
}
