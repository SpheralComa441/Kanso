import { KeyRound, X } from "lucide-react";

interface ApiKeyBannerProps {
  onConfigure: () => void;
  onDismiss: () => void;
}

export function ApiKeyBanner({ onConfigure, onDismiss }: ApiKeyBannerProps) {
  return (
    <div
      role="status"
      className="shrink-0 border-b border-oled-border bg-oled-surface/70 px-4 py-2.5"
    >
      <div className="flex items-center justify-between gap-4 text-xs">
        <div className="flex items-center gap-2.5 text-zinc-300">
          <KeyRound className="h-3.5 w-3.5 shrink-0 text-accent" />
          <p>
            <span className="font-medium text-zinc-100">
              SteamGridDB API Key non configurata.
            </span>{" "}
            La scansione e l&apos;avvio dei giochi funzionano normalmente;
            aggiungi la chiave nelle Impostazioni per abilitare il download
            automatico delle copertine.
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          <button
            id="banner-configure-apikey-button"
            type="button"
            onClick={onConfigure}
            className="rounded-md bg-accent/15 px-2.5 py-1 font-medium text-accent transition-colors hover:bg-accent/25"
          >
            Configura ora
          </button>
          <button
            id="banner-dismiss-button"
            type="button"
            onClick={onDismiss}
            aria-label="Nascondi avviso"
            className="rounded-md p-1 text-zinc-500 transition-colors hover:bg-oled-elevated hover:text-zinc-300"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}
