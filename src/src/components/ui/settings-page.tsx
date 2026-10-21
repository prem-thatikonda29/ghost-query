import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft } from "lucide-react";
import { Button } from "./button";

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  url_editable: boolean;
  needs_key: boolean;
  has_key: boolean;
  model: string;
  key_hint: string;
}

export interface SettingsView {
  active: string;
  providers: ProviderView[];
  hide_from_capture: boolean;
}

interface ModelInfo {
  id: string;
  name: string;
  context_length: number | null;
}

interface SettingsPageProps {
  onBack: () => void;
  onChanged: (view: SettingsView) => void;
}

const inputClass =
  "w-full rounded-lg border border-input bg-background px-3 py-2 text-sm outline-offset-2 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring/70 disabled:opacity-60";

export function SettingsPage({ onBack, onChanged }: SettingsPageProps) {
  const [view, setView] = useState<SettingsView | null>(null);
  const [selectedId, setSelectedId] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [newKey, setNewKey] = useState("");
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [status, setStatus] = useState("");
  const [isError, setIsError] = useState(false);
  const [busy, setBusy] = useState(false);

  const provider = view?.providers.find((p) => p.id === selectedId);

  const loadModels = async (id: string, url: string) => {
    try {
      setModels(await invoke<ModelInfo[]>("list_models", { id, baseUrl: url }));
    } catch {
      setModels([]);
    }
  };

  const select = (v: SettingsView, id: string) => {
    const p = v.providers.find((x) => x.id === id);
    if (!p) return;
    setSelectedId(id);
    setBaseUrl(p.base_url);
    setModel(p.model);
    setNewKey("");
    setModels([]);
    loadModels(id, p.base_url);
  };

  useEffect(() => {
    invoke<SettingsView>("get_settings")
      .then((v) => {
        setView(v);
        select(v, v.active);
      })
      .catch((err) => {
        setIsError(true);
        setStatus(String(err));
      });
  }, []);

  const report = (message: string, error = false) => {
    setStatus(message);
    setIsError(error);
  };

  const save = async () => {
    if (!provider) return;
    setBusy(true);
    report("");
    try {
      const updated = await invoke<SettingsView>("save_provider", {
        id: provider.id,
        baseUrl,
        model,
        apiKey: newKey.trim() || null,
      });
      setView(updated);
      setNewKey("");
      onChanged(updated);
      report(`Saved. Now using ${provider.name}.`);
      loadModels(provider.id, baseUrl);
    } catch (err) {
      report(String(err), true);
    } finally {
      setBusy(false);
    }
  };

  const toggleCapture = async (enabled: boolean) => {
    try {
      const updated = await invoke<SettingsView>("set_hide_from_capture", { enabled });
      setView(updated);
      onChanged(updated);
    } catch (err) {
      report(String(err), true);
    }
  };

  const removeKey = async () => {
    if (!provider) return;
    try {
      const updated = await invoke<SettingsView>("remove_api_key", { id: provider.id });
      setView(updated);
      onChanged(updated);
      report("Key removed from your keychain.");
    } catch (err) {
      report(String(err), true);
    }
  };

  return (
    <div className="w-full flex flex-col h-screen p-4">
      <div className="flex-1 flex flex-col max-w-xl mx-auto w-full min-h-0 overflow-y-auto">
        <div data-tauri-drag-region className="flex items-center gap-2 mb-4">
          <Button variant="outline" size="sm" onClick={onBack}>
            <ArrowLeft className="h-4 w-4 mr-1" />
            Back to chat
          </Button>
          <h1 data-tauri-drag-region className="text-lg font-semibold">Settings</h1>
        </div>

        {view && provider && (
          <div className="space-y-4">
            <label className="block text-xs space-y-1">
              <span className="text-muted-foreground">Provider</span>
              <select
                className={inputClass}
                value={selectedId}
                onChange={(e) => select(view, e.target.value)}
              >
                {view.providers.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                    {p.id === view.active ? " (in use)" : ""}
                    {p.has_key ? " ✓" : ""}
                  </option>
                ))}
              </select>
            </label>

            <label className="block text-xs space-y-1">
              <span className="text-muted-foreground">
                Base URL{provider.url_editable ? " (any OpenAI-compatible endpoint, up to /v1)" : ""}
              </span>
              <input
                className={inputClass}
                value={baseUrl}
                disabled={!provider.url_editable}
                placeholder="https://your-provider.example/v1"
                onChange={(e) => setBaseUrl(e.target.value)}
                onBlur={() => provider.url_editable && loadModels(provider.id, baseUrl)}
                spellCheck={false}
              />
            </label>

            <label className="block text-xs space-y-1">
              <span className="text-muted-foreground">
                Model ID (suggestions come from the provider once a key is saved; you can type any ID)
              </span>
              <input
                className={inputClass}
                list="model-suggestions"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                spellCheck={false}
              />
              <datalist id="model-suggestions">
                {models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                  </option>
                ))}
              </datalist>
            </label>

            <label className="block text-xs space-y-1">
              <span className="text-muted-foreground">
                API key{" "}
                {provider.has_key
                  ? "(saved in your system keychain)"
                  : provider.needs_key
                  ? "(required)"
                  : "(optional)"}
              </span>
              <div className="flex gap-2">
                <input
                  className={inputClass}
                  type="password"
                  autoComplete="off"
                  placeholder={
                    provider.has_key
                      ? "Enter a new key to replace it"
                      : provider.key_hint || "Paste your API key"
                  }
                  value={newKey}
                  onChange={(e) => setNewKey(e.target.value)}
                />
                {provider.has_key && (
                  <Button size="sm" variant="outline" onClick={removeKey}>
                    Remove
                  </Button>
                )}
              </div>
            </label>

            {status && (
              <div className={`text-xs ${isError ? "text-destructive" : "text-muted-foreground"}`}>
                {status}
              </div>
            )}

            <div className="flex justify-end gap-2">
              <Button size="sm" onClick={save} disabled={busy}>
                Save and use
              </Button>
            </div>
          </div>
        )}

        {view && (
          <div className="mt-8 pt-4 border-t space-y-3">
            <label className="flex items-start gap-2 text-xs">
              <input
                type="checkbox"
                className="mt-0.5"
                checked={view.hide_from_capture}
                onChange={(e) => toggleCapture(e.target.checked)}
              />
              <span>
                Hide this window from screenshots and screen sharing
                <span className="block text-muted-foreground">
                  Asks the operating system to exclude the window. Some capture methods (for
                  example newer macOS screen-recording APIs) may still capture it.
                </span>
              </span>
            </label>
            <div className="text-xs text-muted-foreground">
              Ctrl+Shift+Space shows or hides the window. Esc hides it.
            </div>
            <Button size="sm" variant="outline" onClick={() => invoke("quit_app")}>
              Quit Ghost Query
            </Button>
          </div>
        )}

        {!view && status && <div className="text-xs text-destructive">{status}</div>}
      </div>
    </div>
  );
}
