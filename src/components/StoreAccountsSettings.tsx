import { useEffect, useState } from "react";
import { ExternalLink, LogIn, LogOut } from "lucide-react";
import { useI18n, type MessageKey } from "../i18n";
import {
  isRunningInTauri,
  listStoreAccounts,
  openStoreSignIn,
  storeSignIn,
  storeSignOut,
} from "../lib/backend";
import { PLATFORM_META } from "../lib/platforms";
import type { GamePlatform, StoreAccountInfo } from "../types/game";

/** What the sign-in page of each store ends on, and what to copy from it. */
const STEP_TWO: Partial<Record<GamePlatform, MessageKey>> = {
  epic: "accounts.step2.epic",
  gog: "accounts.step2.gog",
};

interface StoreAccountsSettingsProps {
  /** Re-read the accounts whenever the dialog opens: a session may have expired meanwhile. */
  isOpen: boolean;
  /** Someone just signed in: the library has games to fetch. */
  onSignedIn: () => void;
}

/**
 * The stores Kanso can sign in to. Signing in happens on the store's own page, in the
 * browser; here the user only pastes the code that page ends on. Nothing in this panel waits
 * for the Save button of the dialog: an account is signed in or out as soon as it is asked.
 */
export function StoreAccountsSettings({ isOpen, onSignedIn }: StoreAccountsSettingsProps) {
  const { t } = useI18n();
  const [accounts, setAccounts] = useState<StoreAccountInfo[]>([]);
  /** The store whose sign-in steps are showing. */
  const [signingIn, setSigningIn] = useState<GamePlatform | null>(null);
  const [pasted, setPasted] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [justSignedIn, setJustSignedIn] = useState<GamePlatform | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    let active = true;
    setSigningIn(null);
    setPasted("");
    setError(null);
    setJustSignedIn(null);
    listStoreAccounts()
      .then((listed) => {
        if (active) setAccounts(listed);
      })
      .catch(() => {
        if (active) setAccounts([]);
      });
    return () => {
      active = false;
    };
  }, [isOpen]);

  const replace = (info: StoreAccountInfo) =>
    setAccounts((current) =>
      current.map((account) => (account.platform === info.platform ? info : account))
    );

  const failure = (err: unknown) => setError(typeof err === "string" ? err : t("accounts.error"));

  const connect = async (platform: GamePlatform) => {
    setBusy(true);
    setError(null);
    try {
      replace(await storeSignIn(platform, pasted));
      setSigningIn(null);
      setPasted("");
      setJustSignedIn(platform);
      onSignedIn();
    } catch (err) {
      failure(err);
    } finally {
      setBusy(false);
    }
  };

  const signOut = async (platform: GamePlatform) => {
    setError(null);
    try {
      await storeSignOut(platform);
      replace({ platform, connected: false, displayName: null, needsSignIn: false });
      setJustSignedIn(null);
    } catch (err) {
      failure(err);
    }
  };

  if (accounts.length === 0) return null;

  return (
    <div className="space-y-2">
      {accounts.map((account) => {
        const store = PLATFORM_META[account.platform].label;
        const open = signingIn === account.platform;
        const signedIn = account.connected && !account.needsSignIn;
        const status = !account.connected
          ? t("accounts.notConnected")
          : account.needsSignIn
            ? t("accounts.expired")
            : account.displayName
              ? t("accounts.connectedAs", { name: account.displayName })
              : t("accounts.connected");

        return (
          <div
            key={account.platform}
            data-account={account.platform}
            className="rounded-lg border border-oled-border bg-oled-bg/60 px-3 py-2.5"
          >
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <p className="text-xs font-medium text-zinc-100">{store}</p>
                <p
                  data-account-status
                  className={`truncate text-2xs ${
                    account.needsSignIn ? "text-amber-300" : "text-zinc-500"
                  }`}
                >
                  {status}
                </p>
              </div>
              {signedIn ? (
                <button
                  id={`account-sign-out-${account.platform}`}
                  type="button"
                  onClick={() => void signOut(account.platform)}
                  className="inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-oled-border px-2.5 py-1.5 text-xs text-zinc-300 transition-colors hover:border-zinc-600 hover:text-zinc-100"
                >
                  <LogOut className="h-3.5 w-3.5" />
                  <span>{t("accounts.signOut")}</span>
                </button>
              ) : (
                <button
                  id={`account-sign-in-${account.platform}`}
                  type="button"
                  aria-expanded={open}
                  onClick={() => {
                    setSigningIn(open ? null : account.platform);
                    setPasted("");
                    setError(null);
                  }}
                  className="inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-oled-border px-2.5 py-1.5 text-xs text-zinc-200 transition-colors hover:border-zinc-600 hover:text-zinc-100"
                >
                  <LogIn className="h-3.5 w-3.5" />
                  <span>{t(open ? "accounts.cancel" : "accounts.signIn")}</span>
                </button>
              )}
            </div>

            {justSignedIn === account.platform && signedIn && (
              <p role="status" className="mt-2 text-2xs leading-relaxed text-zinc-400">
                {t("accounts.signedIn", { store })}
              </p>
            )}

            {open && (
              <ol className="mt-3 space-y-3 border-t border-oled-border pt-3 text-2xs leading-relaxed text-zinc-400">
                <li className="space-y-1.5">
                  <p>
                    <span className="font-semibold text-zinc-200">1.</span>{" "}
                    {t("accounts.step1", { store })}
                  </p>
                  <button
                    id={`account-open-page-${account.platform}`}
                    type="button"
                    onClick={() => void openStoreSignIn(account.platform).catch(failure)}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-oled-elevated px-2.5 py-1.5 text-xs font-medium text-zinc-100 transition-colors hover:bg-oled-surface"
                  >
                    <ExternalLink className="h-3.5 w-3.5" />
                    <span>{t("accounts.openPage", { store })}</span>
                  </button>
                </li>
                <li className="space-y-1.5">
                  <label htmlFor={`account-code-${account.platform}`} className="block">
                    <span className="font-semibold text-zinc-200">2.</span>{" "}
                    {t(STEP_TWO[account.platform] ?? "accounts.step2.epic")}
                  </label>
                  <div className="flex gap-2">
                    <input
                      id={`account-code-${account.platform}`}
                      type="text"
                      value={pasted}
                      onChange={(e) => setPasted(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && pasted.trim() && !busy) {
                          e.preventDefault();
                          void connect(account.platform);
                        }
                      }}
                      placeholder={t("accounts.codePlaceholder")}
                      autoComplete="off"
                      spellCheck={false}
                      className="min-w-0 flex-1 rounded-md border border-oled-border bg-oled-bg px-2.5 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-600 focus:border-accent/50 focus:outline-none"
                    />
                    <button
                      id={`account-connect-${account.platform}`}
                      type="button"
                      disabled={busy || !pasted.trim()}
                      onClick={() => void connect(account.platform)}
                      className="shrink-0 rounded-lg bg-accent px-3 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:opacity-50"
                    >
                      {t(busy ? "accounts.connecting" : "accounts.connect")}
                    </button>
                  </div>
                  {!isRunningInTauri() && (
                    <p className="text-zinc-500">{t("accounts.previewNote")}</p>
                  )}
                </li>
              </ol>
            )}
          </div>
        );
      })}

      {error && (
        <p role="alert" className="select-text break-words text-2xs leading-relaxed text-red-300">
          {error}
        </p>
      )}
    </div>
  );
}
