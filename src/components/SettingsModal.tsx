import { useEffect, useState } from "react";
import {
  Check,
  Eye,
  EyeOff,
  FolderPlus,
  KeyRound,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import { isRunningInTauri, pickCustomFolder } from "../lib/backend";
import type { AppConfig } from "../types/game";

interface SettingsModalProps {
  isOpen: boolean;
  initialConfig: AppConfig;
  onClose: () => void;
  onSave: (updated: AppConfig) => Promise<void>;
}

export function SettingsModal({
  isOpen,
  initialConfig,
  onClose,
  onSave,
}: SettingsModalProps) {
  const [apiKey, setApiKey] = useState(initialConfig.steamgriddbApiKey);
  const [showKey, setShowKey] = useState(false);
  const [folders, setFolders] = useState<string[]>(initialConfig.customFolders);
  const [manualFolderInput, setManualFolderInput] = useState("");
  const [minimizeOnLaunch, setMinimizeOnLaunch] = useState(
    initialConfig.minimizeToTrayOnLaunch
  );
  const [isSaving, setIsSaving] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      setApiKey(initialConfig.steamgriddbApiKey);
      setFolders(initialConfig.customFolders);
      setMinimizeOnLaunch(initialConfig.minimizeToTrayOnLaunch);
      setManualFolderInput("");
      setShowKey(false);
      setErrorMsg(null);
    }
  }, [isOpen, initialConfig]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const addFolderPath = (rawPath: string) => {
    const trimmed = rawPath.trim();
    if (!trimmed) return;
    const norm = trimmed.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
    const exists = folders.some(
      (f) => f.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase() === norm
    );
    if (!exists) {
      setFolders((prev) => [...prev, trimmed]);
    }
  };

  const handleBrowseFolder = async () => {
    try {
      const picked = await pickCustomFolder();
      if (picked) {
        addFolderPath(picked);
      }
    } catch (err) {
      setErrorMsg(
        err instanceof Error
          ? err.message
          : "Impossibile aprire il selettore cartelle."
      );
    }
  };

  const handleAddManualFolder = () => {
    if (!manualFolderInput.trim()) return;
    addFolderPath(manualFolderInput);
    setManualFolderInput("");
  };

  const handleRemoveFolder = (index: number) => {
    setFolders((prev) => prev.filter((_, idx) => idx !== index));
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSaving(true);
    setErrorMsg(null);
    try {
      await onSave({
        steamgriddbApiKey: apiKey.trim(),
        customFolders: folders,
        minimizeToTrayOnLaunch: minimizeOnLaunch,
      });
      onClose();
    } catch (err) {
      setErrorMsg(
        err instanceof Error
          ? err.message
          : "Errore durante il salvataggio delle impostazioni."
      );
    } finally {
      setIsSaving(false);
    }
  };

  const keyConfigured = apiKey.trim().length > 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-modal-title"
        className="w-full max-w-lg overflow-hidden rounded-xl border border-oled-border bg-oled-surface shadow-2xl"
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-oled-border px-6 py-4">
          <div>
            <h2
              id="settings-modal-title"
              className="text-base font-semibold text-zinc-100"
            >
              Impostazioni
            </h2>
            <p className="mt-0.5 text-xs text-zinc-400">
              Configurazione salvata localmente in AppData sul tuo PC
            </p>
          </div>
          <button
            id="close-settings-button"
            type="button"
            onClick={onClose}
            aria-label="Chiudi impostazioni"
            className="rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-oled-elevated hover:text-zinc-200"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-6 px-6 py-5">
          {errorMsg && (
            <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-xs text-red-200">
              {errorMsg}
            </div>
          )}

          {/* SteamGridDB API Key */}
          <section className="space-y-2">
            <div className="flex items-center justify-between">
              <label
                htmlFor="steamgriddb-api-key-input"
                className="flex items-center gap-1.5 text-xs font-medium text-zinc-200"
              >
                <KeyRound className="h-3.5 w-3.5 text-accent" />
                <span>SteamGridDB API Key</span>
              </label>
              <span
                className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${
                  keyConfigured
                    ? "bg-emerald-500/15 text-emerald-300"
                    : "bg-zinc-800 text-zinc-400"
                }`}
              >
                {keyConfigured ? "Configurata" : "Non impostata"}
              </span>
            </div>

            <div className="relative">
              <input
                id="steamgriddb-api-key-input"
                type={showKey ? "text" : "password"}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder="Incolla qui la tua API key di SteamGridDB..."
                autoComplete="off"
                spellCheck={false}
                className="w-full rounded-lg border border-oled-border bg-oled-bg py-2 pl-3 pr-10 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
              />
              <button
                id="toggle-apikey-visibility"
                type="button"
                onClick={() => setShowKey((v) => !v)}
                aria-label={showKey ? "Nascondi chiave" : "Mostra chiave"}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded p-1 text-zinc-400 hover:text-zinc-200"
              >
                {showKey ? (
                  <EyeOff className="h-3.5 w-3.5" />
                ) : (
                  <Eye className="h-3.5 w-3.5" />
                )}
              </button>
            </div>
            <p className="text-[11px] leading-relaxed text-zinc-500">
              Ottieni gratuitamente una chiave su{" "}
              <span className="font-mono text-zinc-400">
                steamgriddb.com/profile/preferences/api
              </span>
              . La chiave resta solo sul tuo dispositivo e non usa file{" "}
              <code className="text-zinc-400">.env</code>.
            </p>
          </section>

          {/* Custom / Standalone Folders */}
          <section className="space-y-2.5">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-zinc-200">
                Cartelle Giochi Personalizzate (Standalone)
              </span>
              {isRunningInTauri() && (
                <button
                  id="browse-custom-folder-button"
                  type="button"
                  onClick={handleBrowseFolder}
                  className="inline-flex items-center gap-1.5 rounded-md border border-oled-border bg-oled-elevated px-2.5 py-1 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-600"
                >
                  <FolderPlus className="h-3.5 w-3.5 text-accent" />
                  <span>Sfoglia...</span>
                </button>
              )}
            </div>

            <div className="flex gap-2">
              <input
                id="manual-folder-input"
                type="text"
                value={manualFolderInput}
                onChange={(e) => setManualFolderInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    handleAddManualFolder();
                  }
                }}
                placeholder="Es. D:\Games o E:\Giochi Portatili"
                className="flex-1 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-accent/60 focus:outline-none"
              />
              <button
                id="add-manual-folder-button"
                type="button"
                onClick={handleAddManualFolder}
                className="inline-flex items-center gap-1 rounded-lg border border-oled-border bg-oled-elevated px-3 py-1.5 text-xs font-medium text-zinc-200 transition-colors hover:border-zinc-600"
              >
                <Plus className="h-3.5 w-3.5" />
                <span>Aggiungi</span>
              </button>
            </div>

            {folders.length === 0 ? (
              <div className="rounded-lg border border-dashed border-oled-border bg-oled-bg/50 px-3 py-3 text-center text-xs text-zinc-500">
                Nessuna cartella aggiuntiva configurata.
              </div>
            ) : (
              <ul className="max-h-36 space-y-1.5 overflow-y-auto pr-1">
                {folders.map((folder, index) => (
                  <li
                    key={`${folder}-${index}`}
                    className="flex items-center justify-between gap-2 rounded-lg border border-oled-border bg-oled-bg px-3 py-1.5 text-xs"
                  >
                    <span
                      className="truncate font-mono text-zinc-300"
                      title={folder}
                    >
                      {folder}
                    </span>
                    <button
                      id={`remove-folder-${index}`}
                      type="button"
                      onClick={() => handleRemoveFolder(index)}
                      aria-label={`Rimuovi ${folder}`}
                      className="rounded p-1 text-zinc-500 transition-colors hover:bg-red-500/10 hover:text-red-400"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </li>
                ))}
              </ul>
            )}

            <p className="text-[11px] text-zinc-500">
              Gli eseguibili di servizio (<code className="text-zinc-400">unins</code>,{" "}
              <code className="text-zinc-400">crash</code>,{" "}
              <code className="text-zinc-400">redist</code>,{" "}
              <code className="text-zinc-400">dxsetup</code>,{" "}
              <code className="text-zinc-400">vc_redist</code>,{" "}
              <code className="text-zinc-400">launcher</code>) vengono esclusi
              automaticamente.
            </p>
          </section>

          {/* Launch behavior */}
          <section className="flex items-center justify-between rounded-lg border border-oled-border bg-oled-bg px-3.5 py-2.5">
            <div>
              <p className="text-xs font-medium text-zinc-200">
                Minimizza nella barra di sistema all&apos;avvio
              </p>
              <p className="text-[11px] text-zinc-500">
                Riduce automaticamente Kanso quando avvii un gioco
              </p>
            </div>
            <input
              id="minimize-to-tray-checkbox"
              type="checkbox"
              checked={minimizeOnLaunch}
              onChange={(e) => setMinimizeOnLaunch(e.target.checked)}
              className="h-4 w-4 rounded border-zinc-600 bg-oled-surface accent-accent"
            />
          </section>

          {/* Footer */}
          <div className="flex items-center justify-end gap-2.5 border-t border-oled-border pt-4">
            <button
              id="cancel-settings-button"
              type="button"
              onClick={onClose}
              className="rounded-lg border border-oled-border bg-oled-bg px-3.5 py-1.5 text-xs font-medium text-zinc-300 transition-colors hover:bg-oled-elevated"
            >
              Annulla
            </button>
            <button
              id="save-settings-button"
              type="submit"
              disabled={isSaving}
              className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-1.5 text-xs font-semibold text-zinc-950 transition-colors hover:bg-accent-hover disabled:opacity-60"
            >
              <Check className="h-3.5 w-3.5" />
              <span>{isSaving ? "Salvataggio..." : "Salva impostazioni"}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
