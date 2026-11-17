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

/** How often the plugins are asked again while Home stays on screen. */
const ASK_AGAIN_MS = 15 * 60 * 1000;
/** The window coming back asks again too, unless the plugins were asked a moment ago. */
const ASK_BACK_MIN_MS = 60 * 1000;

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
 * While Home stays on screen they are asked again every quarter of an hour and when the
 * window comes back, quietly: Home is about today, also when it is left open for days.
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

  // `quiet`: asked in passing, as time goes by. Nothing on screen says so, and nothing
  // changes unless the answer did
  const load = (plugin: PluginInfo, force: boolean, quiet = false) => {
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
    if (!quiet) {
      setStates((current) => ({
        ...current,
        [plugin.id]: { answer: current[plugin.id]?.answer ?? null, isLoading: true },
      }));
    }
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
    let asked = Date.now();
    const ask = (quiet: boolean) => {
      asked = Date.now();
      for (const plugin of offered) load(plugin, false, quiet);
    };
    ask(false);
    // Home can stay on screen for days (the window waits in the tray while a game runs):
    // the plugins are asked again as time passes, and when the window comes back. The
    // backend answers from its cache while that is fresh, so asking costs next to nothing
    const timer = window.setInterval(() => ask(true), ASK_AGAIN_MS);
    const back = () => {
      if (document.visibilityState === "visible" && Date.now() - asked >= ASK_BACK_MIN_MS) ask(true);
    };
    window.addEventListener("focus", back);
    document.addEventListener("visibilitychange", back);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", back);
      document.removeEventListener("visibilitychange", back);
    };
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
