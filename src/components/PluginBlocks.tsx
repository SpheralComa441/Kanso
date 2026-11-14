import { ExternalLink, Info, TriangleAlert } from "lucide-react";
import { useI18n } from "../i18n";
import { openPluginLink } from "../lib/backend";
import type { PluginBlock, PluginSection } from "../types/game";

/** Host of an `https` link, shown before the user leaves for the browser. */
function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function open(url: string) {
  void openPluginLink(url).catch(() => {});
}

function Block({ block }: { block: PluginBlock }) {
  const { t } = useI18n();

  switch (block.type) {
    case "stats":
      return (
        <dl className="grid grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] gap-2">
          {block.items.map((item, index) => (
            <div
              key={index}
              title={item.hint}
              className="rounded-lg border border-oled-border/70 bg-oled-bg/50 px-3 py-2"
            >
              <dt className="truncate text-3xs text-zinc-500" title={item.label}>
                {item.label}
              </dt>
              <dd className="select-text truncate text-sm font-semibold text-zinc-100" title={item.value}>
                {item.value}
              </dd>
              {item.hint && <dd className="truncate text-3xs text-zinc-500">{item.hint}</dd>}
            </div>
          ))}
        </dl>
      );

    case "text":
      return (
        <p className="select-text whitespace-pre-line text-xs leading-relaxed text-zinc-300">
          {block.text}
        </p>
      );

    case "list":
      return (
        <ul className="divide-y divide-oled-border/60">
          {block.items.map((item, index) => {
            const body = (
              <>
                <span className="flex items-start gap-1.5 text-xs font-medium text-zinc-200">
                  <span className="min-w-0 flex-1">{item.title}</span>
                  {item.url && <ExternalLink className="mt-0.5 h-3 w-3 shrink-0 text-zinc-500" />}
                </span>
                {item.subtitle && (
                  <span className="mt-0.5 line-clamp-2 block text-2xs leading-relaxed text-zinc-500">
                    {item.subtitle}
                  </span>
                )}
              </>
            );
            return (
              <li key={index}>
                {item.url ? (
                  <button
                    type="button"
                    onClick={() => open(item.url as string)}
                    title={t("plugins.opensBrowser", { host: hostOf(item.url) })}
                    className="block w-full rounded-md px-1 py-2 text-left transition-colors hover:bg-oled-bg/60"
                  >
                    {body}
                  </button>
                ) : (
                  <div className="select-text px-1 py-2">{body}</div>
                )}
              </li>
            );
          })}
        </ul>
      );

    case "links":
      return (
        <div className="flex flex-wrap gap-2">
          {block.items.map((item, index) => (
            <button
              key={index}
              type="button"
              onClick={() => open(item.url)}
              title={t("plugins.opensBrowser", { host: hostOf(item.url) })}
              className="inline-flex items-center gap-1.5 rounded-lg border border-oled-border bg-oled-bg/60 px-2.5 py-1 text-2xs font-medium text-zinc-300 transition-colors hover:border-zinc-600 hover:text-zinc-100"
            >
              <span>{item.label}</span>
              <ExternalLink className="h-3 w-3 text-zinc-500" />
            </button>
          ))}
        </div>
      );

    case "meter": {
      const percent = Math.max(0, Math.min(100, (block.value / block.max) * 100));
      return (
        <div className="space-y-1.5">
          <div className="flex items-baseline justify-between gap-3 text-2xs">
            <span className="min-w-0 truncate text-zinc-300">{block.label}</span>
            {block.valueLabel && (
              <span className="shrink-0 text-zinc-500">{block.valueLabel}</span>
            )}
          </div>
          {/* The track is a lighter step of the fill's own color */}
          <div
            role="meter"
            aria-label={block.label}
            aria-valuemin={0}
            aria-valuemax={block.max}
            aria-valuenow={block.value}
            aria-valuetext={block.valueLabel}
            className="h-2 overflow-hidden rounded-full bg-accent/15"
          >
            <div className="h-full rounded-full bg-accent" style={{ width: `${percent}%` }} />
          </div>
        </div>
      );
    }

    case "notice": {
      const warning = block.tone === "warning";
      const Icon = warning ? TriangleAlert : Info;
      return (
        <p
          className={`flex items-start gap-2 rounded-lg border px-3 py-2 text-2xs leading-relaxed ${
            warning
              ? "border-amber-500/25 bg-amber-500/10 text-amber-200"
              : "border-oled-border bg-oled-bg/50 text-zinc-300"
          }`}
        >
          <Icon className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <span className="select-text">{block.text}</span>
        </p>
      );
    }

    default:
      return null;
  }
}

/** The blocks of a plugin's sections, without their titles (the caller owns the headings). */
export function PluginBlocks({ blocks }: { blocks: PluginBlock[] }) {
  return (
    <div className="space-y-2.5">
      {blocks.map((block, index) => (
        <Block key={index} block={block} />
      ))}
    </div>
  );
}

/** Sections as a plugin command returns them: optional title, then the blocks. */
export function PluginSections({ sections }: { sections: PluginSection[] }) {
  return (
    <div className="space-y-4">
      {sections.map((section, index) => (
        <section key={index} className="space-y-2">
          {section.title && (
            <h3 className="text-3xs font-semibold uppercase tracking-[0.12em] text-zinc-500">
              {section.title}
            </h3>
          )}
          <PluginBlocks blocks={section.blocks} />
        </section>
      ))}
    </div>
  );
}
