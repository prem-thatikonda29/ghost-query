import { forwardRef } from "react";
import { ArrowUp, Ghost, Maximize2, Square } from "lucide-react";
import { Button } from "./ui/button";
import { MOD } from "../lib/utils";

interface BarProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onExpand: () => void;
  loading: boolean;
  onStop: () => void;
}

/// The Spotlight-like state of the window: one line to ask from, nothing else. The
/// empty parts of the bar drag the window.
export const Bar = forwardRef<HTMLInputElement, BarProps>(function Bar(
  { value, onChange, onSubmit, onExpand, loading, onStop },
  ref
) {
  const empty = value.trim().length === 0;
  return (
    <div data-tauri-drag-region className="flex h-full items-center gap-2.5 pl-[18px] pr-2.5">
      <Ghost aria-hidden="true" className="h-[18px] w-[18px] shrink-0 text-primary" />
      <input
        ref={ref}
        aria-label="Ask Ghost Query"
        placeholder="Ask anything…"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.nativeEvent.isComposing && !empty) {
            e.preventDefault();
            onSubmit();
          }
        }}
        spellCheck={false}
        className="min-w-0 flex-1 bg-transparent text-base text-foreground outline-none placeholder:text-muted-foreground"
      />
      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 text-muted-foreground"
        aria-label="Open chat panel"
        title={`Open chat panel (${MOD}↑)`}
        onClick={onExpand}
      >
        <Maximize2 className="h-4 w-4" />
      </Button>
      {loading ? (
        <Button size="icon" className="h-8 w-8" aria-label="Stop" title="Stop" onClick={onStop}>
          <Square className="h-3.5 w-3.5" />
        </Button>
      ) : (
        <Button
          size="icon"
          className="h-8 w-8 disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100"
          aria-label="Send"
          title="Send (↵)"
          disabled={empty}
          onClick={onSubmit}
        >
          <ArrowUp className="h-4 w-4" />
        </Button>
      )}
    </div>
  );
});
