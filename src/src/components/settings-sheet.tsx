import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight, Plus, X } from "lucide-react";
import { IconButton } from "./ui/icon-button";
import { cn, MOD } from "../lib/utils";

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  url_editable: boolean;
  needs_key: boolean;
  has_key: boolean;
  configured: boolean;
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

/// Can be used right now: it has a key (or needs none) and somewhere to send to.
const isReady = (p: ProviderView) => (p.has_key || !p.needs_key) && p.base_url !== "";

const fieldClass =
  "h-8 min-w-0 flex-1 rounded-md border border-border bg-field px-2.5 text-[13px] text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-primary/60";
const primaryButton =
  "h-7 rounded-md bg-primary px-3 text-[12px] font-medium text-white hover:bg-primary/90 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring/70 disabled:opacity-50";
const quietButton =
  "h-7 rounded-md px-2.5 text-[12px] font-medium text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring/70 disabled:opacity-50";

interface SettingsSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onChanged: (view: SettingsView) => void;
}

/// Settings slide over the chat as one short column: the providers in use (a row
/// opens to edit it), the rest behind "Add provider", then General. Esc or the close
/// button returns to the chat as it was.
export function SettingsSheet({ open, onOpenChange, onChanged }: SettingsSheetProps) {
  const [view, setView] = useState<SettingsView | null>(null);
  const [expanded, setExpanded] = useState("");
  const [showAll, setShowAll] = useState(false);
  const [loadError, setLoadError] = useState("");
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    setShowAll(false);
    invoke<SettingsView>("get_settings")
      .then((v) => {
        setView(v);
        setLoadError("");
        // A provider in use that cannot answer yet (no key) opens ready to fix.
        const active = v.providers.find((p) => p.id === v.active);
        setExpanded(active && !isReady(active) ? active.id : "");
      })
      .catch((err) => setLoadError(String(err)));
  }, [open]);

  const update = (v: SettingsView) => {
    setView(v);
    onChanged(v);
  };

  const listed = view?.providers.filter((p) => p.configured || p.id === view.active) ?? [];
  const others = view?.providers.filter((p) => !listed.includes(p)) ?? [];

  const row = (p: ProviderView) => (
    <ProviderRow
      key={p.id}
      provider={p}
      active={view?.active === p.id}
      open={expanded === p.id}
      onToggle={() => setExpanded((cur) => (cur === p.id ? "" : p.id))}
      onChanged={update}
    />
  );

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
          <div data-tauri-drag-region className="flex h-11 shrink-0 items-center justify-between pl-5 pr-2">
            <Dialog.Title data-tauri-drag-region className="text-[13px] font-semibold">
              Settings
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton label="Close settings" shortcut="Esc">
                <X />
              </IconButton>
            </Dialog.Close>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto flex max-w-[520px] flex-col gap-6 px-5 pb-8 pt-3">
              {loadError && (
                <p role="alert" className="text-[13px] text-destructive">
                  {loadError}
                </p>
              )}

              {view && (
                <>
                  <Section title="Providers">
                    {listed.map(row)}
                    {showAll && others.map(row)}
                    {!showAll && others.length > 0 && (
                      <button
                        type="button"
                        onClick={() => setShowAll(true)}
                        className="flex h-10 w-full items-center gap-2.5 px-3 text-left text-[13px] text-primary outline-none hover:bg-muted/60 focus-visible:bg-muted/60"
                      >
                        <Plus aria-hidden="true" className="h-3.5 w-3.5" />
                        Add provider
                      </button>
                    )}
                  </Section>

                  <General view={view} onChanged={update} />
                </>
              )}
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/// A titled, rounded group of rows, like a macOS settings list.
function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section aria-label={title} className="flex flex-col gap-1.5">
      <h2 className="px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{title}</h2>
      <div className="flex flex-col divide-y divide-border overflow-hidden rounded-xl border border-border bg-field">
        {children}
      </div>
    </section>
  );
}

interface ProviderRowProps {
  provider: ProviderView;
  active: boolean;
  open: boolean;
  onToggle: () => void;
  onChanged: (v: SettingsView) => void;
}

function ProviderRow({ provider, active, open, onToggle, onChanged }: ProviderRowProps) {
  const ready = isReady(provider);
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex h-10 w-full items-center gap-2.5 px-3 text-left text-[13px] outline-none hover:bg-muted/60 focus-visible:bg-muted/60"
      >
        <span
          role="img"
          aria-label={ready ? "Ready" : "Not set up"}
          className={cn("h-2 w-2 shrink-0 rounded-full", ready ? "bg-success" : "border border-muted-foreground/50")}
        />
        <span className="min-w-0 flex-1 truncate">{provider.name}</span>
        {active && <span className="text-[12px] text-muted-foreground">In use</span>}
        <ChevronRight
          aria-hidden="true"
          className={cn("h-3.5 w-3.5 text-muted-foreground transition-transform", open && "rotate-90")}
        />
      </button>
      {open && <ProviderEditor provider={provider} active={active} onChanged={onChanged} />}
    </div>
  );
}

function ProviderEditor({ provider, active, onChanged }: { provider: ProviderView; active: boolean; onChanged: (v: SettingsView) => void }) {
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

  useEffect(() => {
    loadModels(provider.base_url);
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
      setMessage({ text: "Saved", error: false });
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
      setMessage({ text: "Key removed", error: false });
    } catch (err) {
      setMessage({ text: String(err), error: true });
    }
  };

  const keyId = `key-${provider.id}`;
  const urlId = `url-${provider.id}`;
  const modelId = `model-${provider.id}`;
  const labelClass = "w-16 shrink-0 text-[12px] text-muted-foreground";

  return (
    <div className="flex flex-col gap-2.5 border-t border-border bg-muted/40 px-3 py-3 pl-[30px]">
      <div className="flex items-center gap-3">
        <label htmlFor={keyId} className={labelClass}>
          API key
        </label>
        {provider.has_key && !replacing ? (
          <div className="flex flex-1 items-center gap-1">
            <span id={keyId} className="flex-1 text-[13px]">
              Saved in keychain
            </span>
            <button type="button" className={quietButton} onClick={() => setReplacing(true)}>
              Replace
            </button>
            <button type="button" className={quietButton} onClick={removeKey}>
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
            placeholder={provider.needs_key ? provider.key_hint || "Paste your API key" : "Optional"}
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
            className={fieldClass}
          />
        )}
      </div>

      {provider.url_editable && (
        <div className="flex items-center gap-3">
          <label htmlFor={urlId} className={labelClass}>
            Base URL
          </label>
          <input
            id={urlId}
            value={baseUrl}
            placeholder="https://your-provider.example/v1"
            onChange={(e) => setBaseUrl(e.target.value)}
            onBlur={() => loadModels(baseUrl.trim())}
            spellCheck={false}
            className={fieldClass}
          />
        </div>
      )}

      <div className="flex items-center gap-3">
        <label htmlFor={modelId} className={labelClass}>
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
      </div>

      <div className="flex items-center justify-end gap-1.5">
        {message && (
          <p
            role={message.error ? "alert" : "status"}
            className={cn("mr-auto text-[12px]", message.error ? "text-destructive" : "text-muted-foreground")}
          >
            {message.text}
          </p>
        )}
        {active ? (
          <button type="button" className={primaryButton} disabled={busy} onClick={() => save(true)}>
            Save
          </button>
        ) : (
          <>
            <button type="button" className={quietButton} disabled={busy} onClick={() => save(false)}>
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
    ["Show or hide", "Ctrl Shift Space"],
    ["Step back", "Esc"],
    ["Open the chat from the bar", `${MOD}↑`],
    ["New chat", `${MOD}N`],
    ["Settings", `${MOD},`],
  ];
  const rowClass = "flex h-10 items-center justify-between gap-4 px-3 text-[13px]";

  return (
    <>
      <Section title="General">
        <div className={rowClass}>
          <span id="capture-label">Hide from screenshots and screen sharing</span>
          <button
            type="button"
            role="switch"
            aria-checked={view.hide_from_capture}
            aria-labelledby="capture-label"
            onClick={toggleCapture}
            className={cn(
              "relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring/70",
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
        <button
          type="button"
          onClick={() => invoke("quit_app")}
          className={cn(rowClass, "w-full text-left outline-none hover:bg-muted/60 focus-visible:bg-muted/60")}
        >
          <span>Quit Ghost Query</span>
          <kbd className="font-sans text-[12px] text-muted-foreground">{MOD}Q</kbd>
        </button>
      </Section>
      {error && (
        <p role="alert" className="-mt-4 px-1 text-[12px] text-destructive">
          {error}
        </p>
      )}

      <Section title="Shortcuts">
        {shortcuts.map(([action, keys]) => (
          <div key={action} className={rowClass}>
            <span>{action}</span>
            <kbd className="font-sans text-[12px] text-muted-foreground">{keys}</kbd>
          </div>
        ))}
      </Section>
    </>
  );
}
