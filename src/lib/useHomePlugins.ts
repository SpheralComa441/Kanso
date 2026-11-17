import { useEffect, useMemo, useRef, useState } from "react";
import { useI18n } from "../i18n";
import type { HomeConfig, HomeDataAnswer, PluginInfo } from "../types/game";
import { homePluginPart, pluginHomeData } from "./backend";

/** What one plugin adds to the Home page, and how to ask it again. */
export interface HomePluginPartData {
  plugin: PluginInfo;
  /** `null` until the plugin has answered once. */
  answer: HomeDataAnswer | null;
  /** The plugin is being asked; what it said last stays on screen meanwhile. */
  isLoading: boolean;
  /** Asks the plugin again, past its cache. */
  refresh: () => void;
}

interface PartState {
  answer: HomeDataAnswer | null;
  isLoading: boolean;
}

/**
 * What the plugins add to the Home page: one entry per plugin that is switched on, provides a
 * part of Home and was not left out by the user, in the plugins' order.
 *
 * The plugins are asked only while `shown` (Home is on screen, and its theme has a place for
 * them), each on its own: one that is slow or fails does not hold the others back. What they
 * answered stays here between visits to the page, so that coming back to Home draws it at
 * once while the backend is asked again (it answers from its cache while that is fresh).
 */
export function useHomePlugins(
  plugins: PluginInfo[],
  hidden: HomeConfig["hidden"],
  shown: boolean
): HomePluginPartData[] {
  const { t } = useI18n();
  const [states, setStates] = useState<Record<string, PartState>>({});
  // The last request made to each plugin: an answer to an earlier one is not put on screen
  const tickets = useRef<Record<string, number>>({});

  // By content: the settings are a new object after every save, whatever changed in them
  const hiddenKey = hidden.join("\n");
  const offered = useMemo(
    () => plugins.filter((plugin) => plugin.enabled && plugin.home && !hidden.includes(homePluginPart(plugin.id))),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [plugins, hiddenKey]
  );

  const load = (plugin: PluginInfo, force: boolean) => {
    const ticket = (tickets.current[plugin.id] ?? 0) + 1;
    tickets.current[plugin.id] = ticket;
    const settle = (answer: HomeDataAnswer) => {
      if (tickets.current[plugin.id] !== ticket) return;
      setStates((current) => {
        const previous = current[plugin.id]?.answer?.data ?? null;
        // The same view again is the same object: nothing on screen starts over (cards
        // already turned stay turned)
        const same = previous !== null && JSON.stringify(previous) === JSON.stringify(answer.data);
        return {
          ...current,
          [plugin.id]: { answer: same ? { ...answer, data: previous } : answer, isLoading: false },
        };
      });
    };
    setStates((current) => ({
      ...current,
      [plugin.id]: { answer: current[plugin.id]?.answer ?? null, isLoading: true },
    }));
    pluginHomeData(plugin.id, force)
      .then(settle)
      .catch((error) =>
        settle({
          pluginId: plugin.id,
          pluginName: plugin.name,
          data: null,
          error: typeof error === "string" ? error : t("plugins.actionError"),
          logs: [],
          fetchedAt: Date.now(),
        })
      );
  };

  useEffect(() => {
    if (!shown) return;
    for (const plugin of offered) load(plugin, false);
    // `load` and `t` change identity on every render; the page shown and the plugins offered
    // (re-read whenever the settings are saved or the language changes) are what matter
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shown, offered]);

  return offered.map((plugin) => ({
    plugin,
    answer: states[plugin.id]?.answer ?? null,
    isLoading: states[plugin.id]?.isLoading ?? false,
    refresh: () => load(plugin, true),
  }));
}
