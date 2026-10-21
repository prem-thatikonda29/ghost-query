import { forwardRef } from "react";
import { ArrowUp, Ghost, Maximize2, Square } from "lucide-react";
import { IconButton } from "./ui/icon-button";
import { MOD } from "../lib/utils";

interface BarProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  onExpand: () => void;
  loading: boolean;
  onStop: () => void;
  placeholder: string;
}

/// The Spotlight-like state of the window: one line to ask from. The empty parts of
/// the bar drag the window. Tooltips open sideways, as the bar has no room below.
export const Bar = forwardRef<HTMLInputElement, BarProps>(function Bar(
  { value, onChange, onSubmit, onExpand, loading, onStop, placeholder },
  ref
) {
  const empty = value.trim().length === 0;
  return (
    <div data-tauri-drag-region className="flex h-full items-center gap-2 pl-[18px] pr-2.5">
      <Ghost aria-hidden="true" className="h-[18px] w-[18px] shrink-0 text-primary" />
      <input
        ref={ref}
        aria-label="Ask Ghost Query"
        placeholder={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.nativeEvent.isComposing && !empty) {
            e.preventDefault();
            onSubmit();
          }
        }}
        spellCheck={false}
        className="ml-0.5 min-w-0 flex-1 truncate bg-transparent text-base text-foreground outline-none placeholder:text-muted-foreground"
      />
      <IconButton label="Open chat" shortcut={`${MOD}↑`} side="left" className="h-8 w-8" onClick={onExpand}>
        <Maximize2 />
      </IconButton>
      {loading ? (
        <IconButton
          label="Stop"
          side="left"
          className="h-8 w-8 rounded-lg bg-primary text-white hover:bg-primary/90 hover:text-white [&_svg]:h-3.5 [&_svg]:w-3.5"
          onClick={onStop}
        >
          <Square />
        </IconButton>
      ) : (
        <IconButton
          label="Send"
          shortcut="↵"
          side="left"
          disabled={empty}
          className="h-8 w-8 rounded-lg bg-primary text-white hover:bg-primary/90 hover:text-white disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100"
          onClick={onSubmit}
        >
          <ArrowUp />
        </IconButton>
      )}
    </div>
  );
});
