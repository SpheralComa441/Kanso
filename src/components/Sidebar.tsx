import { Library, Star } from "lucide-react";
import type { ReactNode } from "react";
import type { GenreSummary, LibraryFilters } from "../lib/library";
import { hasActiveFilters } from "../lib/library";
import { PLATFORM_META, PLATFORM_ORDER } from "../lib/platforms";
import type { GamePlatform } from "../types/game";

interface SidebarProps {
  isOpen: boolean;
  filters: LibraryFilters;
  onFiltersChange: (next: LibraryFilters) => void;
  totalGames: number;
  favoriteCount: number;
  platformCounts: Record<GamePlatform, number>;
  genres: GenreSummary[];
}

interface FilterRowProps {
  id: string;
  label: string;
  count: number;
  active: boolean;
  disabled?: boolean;
  icon?: ReactNode;
  onClick: () => void;
}

function FilterRow({
  id,
  label,
  count,
  active,
  disabled = false,
  icon,
  onClick,
}: FilterRowProps) {
  return (
    <button
      id={id}
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-pressed={active}
      className={`flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs transition-colors disabled:cursor-default ${
        active
          ? "bg-oled-elevated font-medium text-zinc-100"
          : disabled
            ? "text-zinc-600"
            : "text-zinc-400 hover:bg-oled-surface hover:text-zinc-200"
      }`}
    >
      {icon}
      <span className="min-w-0 flex-1 truncate" title={label}>
        {label}
      </span>
      <span
        className={`shrink-0 tabular-nums ${
          active ? "text-accent" : disabled ? "text-zinc-700" : "text-zinc-500"
        }`}
      >
        {count}
      </span>
    </button>
  );
}

function SectionTitle({ children }: { children: ReactNode }) {
  return (
    <h2 className="px-2.5 pb-1.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-zinc-600">
      {children}
    </h2>
  );
}

/**
 * Collapsible quick-filter panel: favorites, platform and the genres found in the
 * library's metadata. Filters combine; clicking an active one switches it off.
 */
export function Sidebar({
  isOpen,
  filters,
  onFiltersChange,
  totalGames,
  favoriteCount,
  platformCounts,
  genres,
}: SidebarProps) {
  const nothingFiltered = !hasActiveFilters(filters);

  return (
    <aside
      id="library-sidebar"
      aria-label="Filtri libreria"
      className={`h-full shrink-0 overflow-hidden border-oled-border transition-[width,visibility] duration-200 ease-out ${
        isOpen ? "visible w-[var(--sidebar-width)] border-r" : "invisible w-0"
      }`}
    >
      <nav className="flex h-full w-[var(--sidebar-width)] flex-col gap-5 overflow-y-auto px-2.5 py-4">
        <section>
          <SectionTitle>Libreria</SectionTitle>
          <div className="space-y-0.5">
            <FilterRow
              id="filter-all"
              label="Tutti i giochi"
              count={totalGames}
              active={nothingFiltered}
              icon={<Library className="h-3.5 w-3.5 shrink-0" />}
              onClick={() =>
                onFiltersChange({
                  platform: null,
                  genre: null,
                  favoritesOnly: false,
                })
              }
            />
            <FilterRow
              id="filter-favorites"
              label="Preferiti"
              count={favoriteCount}
              active={filters.favoritesOnly}
              icon={
                <Star
                  className={`h-3.5 w-3.5 shrink-0 ${
                    filters.favoritesOnly ? "fill-accent text-accent" : ""
                  }`}
                />
              }
              onClick={() =>
                onFiltersChange({
                  ...filters,
                  favoritesOnly: !filters.favoritesOnly,
                })
              }
            />
          </div>
        </section>

        <section>
          <SectionTitle>Piattaforma</SectionTitle>
          <div className="space-y-0.5">
            {PLATFORM_ORDER.map((platform) => {
              const active = filters.platform === platform;
              const count = platformCounts[platform] ?? 0;
              return (
                <FilterRow
                  key={platform}
                  id={`filter-platform-${platform}`}
                  label={PLATFORM_META[platform].label}
                  count={count}
                  active={active}
                  disabled={count === 0 && !active}
                  onClick={() =>
                    onFiltersChange({
                      ...filters,
                      platform: active ? null : platform,
                    })
                  }
                />
              );
            })}
          </div>
        </section>

        <section className="min-h-0">
          <SectionTitle>Genere</SectionTitle>
          {genres.length === 0 ? (
            <p className="px-2.5 text-[11px] leading-relaxed text-zinc-600">
              I generi compaiono qui man mano che i metadata vengono
              sincronizzati.
            </p>
          ) : (
            <div className="space-y-0.5">
              {genres.map((genre) => {
                const active = filters.genre === genre.key;
                return (
                  <FilterRow
                    key={genre.key}
                    id={`filter-genre-${genre.key}`}
                    label={genre.label}
                    count={genre.count}
                    active={active}
                    onClick={() =>
                      onFiltersChange({
                        ...filters,
                        genre: active ? null : genre.key,
                      })
                    }
                  />
                );
              })}
            </div>
          )}
        </section>
      </nav>
    </aside>
  );
}
