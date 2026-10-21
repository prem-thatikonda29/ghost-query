import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./button";

interface SettingsView {
  base_url: string;
  model: string;
  has_key: boolean;
}

interface ModelInfo {
  id: string;
  name: string;
  context_length: number | null;
}

interface SettingsPanelProps {
  onClose: () => void;
  onSaved: (view: SettingsView) => void;
}

const inputClass =
  "w-full rounded-lg border border-input bg-background px-3 py-2 text-sm outline-offset-2 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring/70";

export function SettingsPanel({ onClose, onSaved }: SettingsPanelProps) {
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [hasKey, setHasKey] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);

  const loadModels = async () => {
    try {
      setModels(await invoke<ModelInfo[]>("list_models"));
    } catch {
      setModels([]);
    }
  };

  useEffect(() => {
    invoke<SettingsView>("get_settings")
      .then((view) => {
        setBaseUrl(view.base_url);
        setModel(view.model);
        setHasKey(view.has_key);
      })
      .catch((err) => setStatus(String(err)));
    loadModels();
  }, []);

  const save = async () => {
    setBusy(true);
    setStatus("");
    try {
      if (newKey.trim()) {
        await invoke("set_api_key", { key: newKey });
        setNewKey("");
      }
      await invoke("save_settings", { baseUrl, model });
      const view = await invoke<SettingsView>("get_settings");
      setHasKey(view.has_key);
      onSaved(view);
      onClose();
    } catch (err) {
      setStatus(String(err));
    } finally {
      setBusy(false);
    }
  };

  const removeKey = async () => {
    try {
      await invoke("clear_api_key");
      setHasKey(false);
    } catch (err) {
      setStatus(String(err));
    }
  };

  return (
    <div className="mb-4 rounded-lg border p-4 bg-muted/30 space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium">Provider settings</h3>
        <Button size="sm" variant="ghost" onClick={onClose}>
          Close
        </Button>
      </div>

      <label className="block text-xs space-y-1">
        <span className="text-muted-foreground">
          Base URL (any OpenAI-compatible endpoint, up to /v1)
        </span>
        <input
          className={inputClass}
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          onBlur={loadModels}
          spellCheck={false}
        />
      </label>

      <label className="block text-xs space-y-1">
        <span className="text-muted-foreground">
          Model ID (free models are listed first; you can type any ID)
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
          API key {hasKey ? "(saved in your system keychain)" : "(not set)"}
        </span>
        <div className="flex gap-2">
          <input
            className={inputClass}
            type="password"
            autoComplete="off"
            placeholder={hasKey ? "Enter a new key to replace it" : "sk-or-..."}
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
          />
          {hasKey && (
            <Button size="sm" variant="outline" onClick={removeKey}>
              Remove
            </Button>
          )}
        </div>
      </label>

      {status && <div className="text-xs text-destructive">{status}</div>}

      <div className="flex justify-end">
        <Button size="sm" onClick={save} disabled={busy}>
          Save
        </Button>
      </div>
    </div>
  );
}
