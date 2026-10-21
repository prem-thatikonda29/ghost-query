import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { invoke } from "@tauri-apps/api/core";
import { SlidersHorizontal, X } from "lucide-react";
import { IconButton } from "./ui/icon-button";
import { cn, MOD } from "../lib/utils";

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

const GENERAL = "general";

/// One line under a provider's name: can it be used, and why not.
function status(p: ProviderView): { text: string; ready: boolean } {
  if (p.url_editable && !p.base_url) return { text: "Not set up", ready: false };
  if (p.has_key) return { text: "Key saved", ready: true };
  if (!p.needs_key) return { text: "No key needed", ready: true };
  return { text: "No key", ready: false };
}

const fieldClass =
  "h-9 w-full rounded-lg border border-border bg-field px-3 text-[13px] text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary/60 disabled:opacity-60";
const primaryButton =
  "h-8 rounded-lg bg-primary px-3 text-[13px] font-medium text-white hover:bg-primary/90 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring/70 disabled:opacity-50";
const secondaryButton =
  "h-8 rounded-lg bg-muted px-3 text-[13px] font-medium text-foreground hover:bg-muted/70 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring/70 disabled:opacity-50";

interface SettingsSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onChanged: (view: SettingsView) => void;
}

/// Settings slide over the chat: providers on the left with whether each is ready to
/// use, the chosen one (or General) on the right. Esc or the close button returns to
/// the chat as it was.
export function SettingsSheet({ open, onOpenChange, onChanged }: SettingsSheetProps) {
  const [view, setView] = useState<SettingsView | null>(null);
  const [selected, setSelected] = useState("");
  const [loadError, setLoadError] = useState("");
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    invoke<SettingsView>("get_settings")
      .then((v) => {
        setView(v);
        setLoadError("");
        // Opens on the provider in use, or on General if it is ready to go.
        const active = v.providers.find((p) => p.id === v.active);
        setSelected((prev) => prev || (active && !status(active).ready ? active.id : GENERAL));
      })
      .catch((err) => setLoadError(String(err)));
  }, [open]);

  const update = (v: SettingsView) => {
    setView(v);
    onChanged(v);
  };

  const provider = view?.providers.find((p) => p.id === selected);

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Content
          ref={contentRef}
          aria-describedby={undefined}
          // Focus the sheet itself rather than its first button (the close button,
          // whose tooltip would pop up on every open).
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            contentRef.current?.focus();
          }}
          className="gq-sheet-in fixed inset-0 z-30 flex flex-col overflow-hidden rounded-[14px] border border-border bg-sheet text-foreground outline-none"
        >
          <div
            data-tauri-drag-region
            className="flex h-11 shrink-0 items-center justify-between border-b border-border pl-4 pr-2"
          >
            <Dialog.Title data-tauri-drag-region className="text-[13px] font-semibold">
              Settings
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton label="Close settings" shortcut="Esc">
                <X />
              </IconButton>
            </Dialog.Close>
          </div>

          {loadError && <p className="p-4 text-[13px] text-destructive">{loadError}</p>}

          {view && (
            <div className="flex min-h-0 flex-1">
              <nav aria-label="Settings sections" className="w-52 shrink-0 overflow-y-auto border-r border-border p-2">
                <NavItem selected={selected === GENERAL} onClick={() => setSelected(GENERAL)}>
                  <SlidersHorizontal aria-hidden="true" className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>General</span>
                </NavItem>
                <div className="px-2 pb-1 pt-4 text-[11px] font-medium text-muted-foreground">Providers</div>
                {view.providers.map((p) => {
                  const s = status(p);
                  return (
                    <NavItem key={p.id} selected={selected === p.id} onClick={() => setSelected(p.id)}>
                      <span
                        aria-hidden="true"
                        className={cn("h-1.5 w-1.5 shrink-0 rounded-full", s.ready ? "bg-success" : "bg-muted-foreground/40")}
                      />
                      <span className="min-w-0 flex-1 truncate">{p.name}</span>
                      <span className="shrink-0 text-[11px] text-muted-foreground">
                        {p.id === view.active ? "In use" : s.text}
                      </span>
                    </NavItem>
                  );
                })}
              </nav>

              <div className="min-w-0 flex-1 overflow-y-auto px-6 py-5">
                {selected === GENERAL && <General view={view} onChanged={update} />}
                {provider && <ProviderDetail key={provider.id} provider={provider} active={view.active === provider.id} onChanged={update} />}
              </div>
            </div>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function NavItem({ selected, onClick, children }: { selected: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-current={selected ? "page" : undefined}
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-2 rounded-[7px] px-2 py-1.5 text-left text-[13px] outline-none hover:bg-muted focus-visible:bg-muted",
        selected && "bg-muted"
      )}
    >
      {children}
    </button>
  );
}

function ProviderDetail({ provider, active, onChanged }: { provider: ProviderView; active: boolean; onChanged: (v: SettingsView) => void }) {
  const [baseUrl, setBaseUrl] = useState(provider.base_url);
  const [model, setModel] = useState(provider.model);
  const [newKey, setNewKey] = useState("");
  const [replacing, setReplacing] = useState(false);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const keyRef = useRef<HTMLInputElement>(null);

  // Only the newest request may fill the suggestions, so a slow answer for an old URL
  // cannot replace them.
  const request = useRef(0);
  const loadModels = (url: string) => {
    const seq = ++request.current;
    if (!url) return setModels([]);
    invoke<ModelInfo[]>("list_models", { id: provider.id, baseUrl: url })
      .then((list) => seq === request.current && setModels(list ?? []))
      .catch(() => seq === request.current && setModels([]));
  };
  useEffect(() => loadModels(provider.base_url), []);

  // A provider that cannot be used without a key opens with the key field ready.
  useEffect(() => {
    if (provider.needs_key && !provider.has_key) keyRef.current?.focus();
  }, []);

  const save = async (activate: boolean) => {
    setBusy(true);
    setMessage(null);
    try {
      const updated = await invoke<SettingsView>("save_provider", {
        id: provider.id,
        baseUrl,
        model,
        apiKey: newKey.trim() || null,
        activate,
      });
      setNewKey("");
      setReplacing(false);
      onChanged(updated);
      setMessage({ text: activate ? `Saved. Now using ${provider.name}.` : "Saved.", error: false });
      loadModels(baseUrl);
    } catch (err) {
      setMessage({ text: String(err), error: true });
    } finally {
      setBusy(false);
    }
  };

  const removeKey = async () => {
    try {
      onChanged(await invoke<SettingsView>("remove_api_key", { id: provider.id }));
      setMessage({ text: "Key removed from your keychain.", error: false });
    } catch (err) {
      setMessage({ text: String(err), error: true });
    }
  };

  const keyId = `key-${provider.id}`;
  const urlId = `url-${provider.id}`;
  const modelId = `model-${provider.id}`;

  return (
    <div className="flex max-w-md flex-col gap-5">
      <div className="flex items-center gap-2">
        <h2 className="text-base font-semibold">{provider.name}</h2>
        {active && (
          <span className="rounded-full bg-primary/15 px-2 py-0.5 text-[11px] font-medium text-primary">In use</span>
        )}
      </div>

      <div className="flex flex-col gap-1.5">
        <label htmlFor={keyId} className="text-[12px] font-medium">
          API key{!provider.needs_key && <span className="font-normal text-muted-foreground"> (optional)</span>}
        </label>
        {provider.has_key && !replacing ? (
          <div className="flex items-center gap-2">
            <p id={keyId} className="flex-1 text-[13px] text-muted-foreground">
              Saved in your system keychain.
            </p>
            <button type="button" className={secondaryButton} onClick={() => setReplacing(true)}>
              Replace
            </button>
            <button type="button" className={secondaryButton} onClick={removeKey}>
              Remove
            </button>
          </div>
        ) : (
          <input
            ref={keyRef}
            id={keyId}
            type="password"
            autoComplete="off"
            autoFocus={replacing}
            placeholder={provider.key_hint || "Paste your API key"}
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
            className={fieldClass}
          />
        )}
        <p className="text-[11px] text-muted-foreground">Keys stay in your system keychain and are never shown again.</p>
      </div>

      <div className="flex flex-col gap-1.5">
        <label htmlFor={urlId} className="text-[12px] font-medium">
          Base URL
        </label>
        {provider.url_editable ? (
          <>
            <input
              id={urlId}
              value={baseUrl}
              placeholder="https://your-provider.example/v1"
              onChange={(e) => setBaseUrl(e.target.value)}
              onBlur={() => loadModels(baseUrl.trim())}
              spellCheck={false}
              className={fieldClass}
            />
            <p className="text-[11px] text-muted-foreground">Any OpenAI-compatible endpoint, up to /v1.</p>
          </>
        ) : (
          <p id={urlId} className="truncate font-mono text-[12px] text-muted-foreground">
            {provider.base_url}
          </p>
        )}
      </div>

      <div className="flex flex-col gap-1.5">
        <label htmlFor={modelId} className="text-[12px] font-medium">
          Model
        </label>
        <input
          id={modelId}
          list={`${modelId}-suggestions`}
          value={model}
          placeholder="Model ID"
          onChange={(e) => setModel(e.target.value)}
          spellCheck={false}
          className={fieldClass}
        />
        <datalist id={`${modelId}-suggestions`}>
          {models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </datalist>
        <p className="text-[11px] text-muted-foreground">
          Suggestions appear once a key is saved. You can also switch models from the chip under the message box.
        </p>
      </div>

      <div className="flex items-center justify-end gap-2">
        {message && (
          <p role={message.error ? "alert" : "status"} className={cn("mr-auto text-[12px]", message.error ? "text-destructive" : "text-muted-foreground")}>
            {message.text}
          </p>
        )}
        {active ? (
          <button type="button" className={primaryButton} disabled={busy} onClick={() => save(true)}>
            Save
          </button>
        ) : (
          <>
            <button type="button" className={secondaryButton} disabled={busy} onClick={() => save(false)}>
              Save
            </button>
            <button type="button" className={primaryButton} disabled={busy} onClick={() => save(true)}>
              Save and use
            </button>
          </>
        )}
      </div>
    </div>
  );
}

function General({ view, onChanged }: { view: SettingsView; onChanged: (v: SettingsView) => void }) {
  const [error, setError] = useState("");

  const toggleCapture = async () => {
    try {
      onChanged(await invoke<SettingsView>("set_hide_from_capture", { enabled: !view.hide_from_capture }));
      setError("");
    } catch (err) {
      setError(String(err));
    }
  };

  const shortcuts: [string, string][] = [
    ["Show or hide Ghost Query", "Ctrl Shift Space"],
    ["Step back (close, collapse, hide)", "Esc"],
    ["Open the chat from the bar", `${MOD}↑`],
    ["New chat", `${MOD}N`],
    ["Settings", `${MOD},`],
    ["Quit", `${MOD}Q`],
  ];

  return (
    <div className="flex max-w-md flex-col gap-6">
      <h2 className="text-base font-semibold">General</h2>

      <div className="flex items-start justify-between gap-4">
        <div className="flex flex-col gap-1">
          <span id="capture-label" className="text-[13px] font-medium">
            Hide from screenshots and screen sharing
          </span>
          <span className="text-[11px] leading-relaxed text-muted-foreground">
            Asks the system to leave this window out. Some capture methods, such as newer macOS screen-recording
            tools, may still show it.
          </span>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={view.hide_from_capture}
          aria-labelledby="capture-label"
          onClick={toggleCapture}
          className={cn(
            "relative mt-0.5 h-[22px] w-[38px] shrink-0 rounded-full transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring/70",
            view.hide_from_capture ? "bg-primary" : "bg-muted-foreground/30"
          )}
        >
          <span
            aria-hidden="true"
            className={cn(
              "absolute left-[2px] top-[2px] h-[18px] w-[18px] rounded-full bg-white shadow transition-transform",
              view.hide_from_capture && "translate-x-4"
            )}
          />
        </button>
      </div>
      {error && <p role="alert" className="-mt-4 text-[12px] text-destructive">{error}</p>}

      <div className="flex flex-col gap-2">
        <h3 className="text-[13px] font-medium">Keyboard shortcuts</h3>
        <dl className="flex flex-col divide-y divide-border rounded-lg border border-border">
          {shortcuts.map(([action, keys]) => (
            <div key={action} className="flex items-center justify-between px-3 py-2 text-[13px]">
              <dt className="text-muted-foreground">{action}</dt>
              <dd>
                <kbd className="rounded bg-muted px-1.5 py-0.5 font-sans text-[12px]">{keys}</kbd>
              </dd>
            </div>
          ))}
        </dl>
      </div>

      <div>
        <button type="button" className={secondaryButton} onClick={() => invoke("quit_app")}>
          Quit Ghost Query
        </button>
      </div>
    </div>
  );
}
