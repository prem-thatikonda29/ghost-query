import { useEffect, useMemo, useRef, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import { invoke } from "@tauri-apps/api/core";
import { Check, ChevronDown, Search } from "lucide-react";
import type { ProviderView, SettingsView } from "./settings-sheet";

interface ModelInfo {
  id: string;
  name: string;
  context_length: number | null;
}

/// At most this many matches are listed per provider (OpenRouter alone has hundreds).
const PER_PROVIDER = 50;

/// "google/gemma-4-31b-it:free" → "gemma-4-31b-it".
export function shortModelName(model: string): string {
  return model.split("/").pop()!.replace(/:free$/, "") || model;
}

export function activeProvider(view: SettingsView | null): ProviderView | undefined {
  return view?.providers.find((p) => p.id === view.active);
}

/// Providers that can be used right now: they have a key (or need none) and a URL.
const usable = (p: ProviderView) => (p.has_key || !p.needs_key) && p.base_url !== "";

const chipClass =
  "flex h-7 max-w-[220px] shrink-0 items-center gap-1 rounded-[7px] bg-muted px-2 text-xs font-medium text-muted-foreground hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring/70 data-[state=open]:text-foreground";

function chipLabel(view: SettingsView | null) {
  const active = activeProvider(view);
  return active?.model ? shortModelName(active.model) : "Choose model";
}

interface ModelPickerProps {
  view: SettingsView | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onPicked: (view: SettingsView) => void;
  onManage: () => void;
  onError: (message: string) => void;
}

/// The composer's model chip and its searchable list of every set-up provider's models.
export function ModelPicker({ view, open, onOpenChange, onPicked, onManage, onError }: ModelPickerProps) {
  const [query, setQuery] = useState("");
  const [models, setModels] = useState<Record<string, ModelInfo[] | "loading" | "failed">>({});
  const searchRef = useRef<HTMLInputElement>(null);
  // Providers whose list has been fetched or is being fetched.
  const requested = useRef(new Set<string>());
  const label = chipLabel(view);
  const providers = useMemo(() => view?.providers.filter(usable) ?? [], [view]);

  // Each provider's list is fetched the first time the picker opens, then kept.
  useEffect(() => {
    if (!open) return;
    setQuery("");
    for (const p of providers) {
      if (requested.current.has(p.id)) continue;
      requested.current.add(p.id);
      setModels((prev) => ({ ...prev, [p.id]: "loading" }));
      invoke<ModelInfo[]>("list_models", { id: p.id, baseUrl: p.base_url })
        .then((list) => setModels((prev) => ({ ...prev, [p.id]: list ?? [] })))
        .catch(() => {
          // Tried again the next time the picker opens.
          requested.current.delete(p.id);
          setModels((prev) => ({ ...prev, [p.id]: "failed" }));
        });
    }
  }, [open, providers]);

  const pick = async (p: ProviderView, model: string) => {
    onOpenChange(false);
    try {
      onPicked(
        await invoke<SettingsView>("save_provider", { id: p.id, baseUrl: p.base_url, model, apiKey: null, activate: true })
      );
    } catch (err) {
      onError(String(err));
    }
  };

  const q = query.trim().toLowerCase();
  const sections = providers.map((p) => {
    const state = models[p.id];
    const list = Array.isArray(state) ? state : [];
    // The saved model is always offered, even if the provider does not list it.
    const all = list.some((m) => m.id === p.model) || !p.model ? list : [{ id: p.model, name: "", context_length: null }, ...list];
    const matches = all.filter((m) => !q || m.id.toLowerCase().includes(q) || m.name.toLowerCase().includes(q));
    return { p, state, matches: matches.slice(0, PER_PROVIDER), more: matches.length - PER_PROVIDER };
  });
  const active = activeProvider(view);
  const exact = sections.some((s) => s.matches.some((m) => m.id.toLowerCase() === q));

  return (
    <Popover.Root open={open} onOpenChange={onOpenChange}>
      <Popover.Trigger asChild>
        <button type="button" aria-label={`Model: ${label}. Change model`} className={chipClass}>
          <span className="truncate">{label}</span>
          <ChevronDown aria-hidden="true" className="h-3 w-3 shrink-0" />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="top"
          align="start"
          sideOffset={8}
          collisionPadding={8}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            searchRef.current?.focus();
          }}
          className="z-40 flex max-h-[min(420px,var(--radix-popover-content-available-height))] w-80 flex-col rounded-[10px] border border-border bg-popover text-popover-foreground shadow-xl outline-none"
        >
          <div className="flex items-center gap-2 border-b border-border px-3">
            <Search aria-hidden="true" className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <input
              ref={searchRef}
              aria-label="Search models"
              placeholder="Search models"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                // Enter takes a typed model ID that is not in any list.
                if (e.key === "Enter" && q && !exact && active && usable(active)) pick(active, query.trim());
              }}
              spellCheck={false}
              className="h-9 min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-muted-foreground"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-1">
            {providers.length === 0 && (
              <p className="px-2 py-3 text-[13px] text-muted-foreground">No provider is set up yet. Add an API key in Settings.</p>
            )}
            {sections.map(({ p, state, matches, more }) => (
              <section key={p.id} aria-label={p.name} className="pb-1">
                <div className="px-2 pb-0.5 pt-1.5 text-[11px] font-medium text-muted-foreground">{p.name}</div>
                {state === "loading" && matches.length === 0 && (
                  <div className="px-2 py-1 text-[13px] text-muted-foreground">Loading models…</div>
                )}
                {state === "failed" && (
                  <div className="px-2 py-1 text-[12px] text-muted-foreground">Could not load this provider's models.</div>
                )}
                {matches.map((m) => {
                  const selected = p.id === view?.active && m.id === p.model;
                  return (
                    <button
                      key={m.id}
                      type="button"
                      onClick={() => pick(p, m.id)}
                      aria-current={selected ? "true" : undefined}
                      className="flex w-full items-center gap-2 rounded-[7px] px-2 py-1.5 text-left text-[13px] outline-none hover:bg-muted focus-visible:bg-muted"
                    >
                      <Check aria-hidden="true" className={`h-3.5 w-3.5 shrink-0 text-primary ${selected ? "" : "invisible"}`} />
                      <span className="min-w-0 flex-1 truncate" title={m.id}>
                        {m.name || m.id}
                      </span>
                    </button>
                  );
                })}
                {more > 0 && <div className="px-2 py-1 text-[11px] text-muted-foreground">{more} more; refine the search</div>}
              </section>
            ))}
            {q && !exact && active && usable(active) && (
              <button
                type="button"
                onClick={() => pick(active, query.trim())}
                className="flex w-full items-center rounded-[7px] px-2 py-1.5 text-left text-[13px] outline-none hover:bg-muted focus-visible:bg-muted"
              >
                <span className="truncate">
                  Use “{query.trim()}” with {active.name}
                </span>
              </button>
            )}
          </div>
          <button
            type="button"
            onClick={() => {
              onOpenChange(false);
              onManage();
            }}
            className="border-t border-border px-3 py-2 text-left text-[12px] text-muted-foreground outline-none hover:text-foreground focus-visible:text-foreground"
          >
            Providers and API keys…
          </button>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
