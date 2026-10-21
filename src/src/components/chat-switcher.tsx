import { useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import { Check, ChevronDown, Trash2 } from "lucide-react";
import { IconButton } from "./ui/icon-button";

export interface ConversationSummary {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
}

/// "now", "12m", "3h", "Yesterday", then a short date.
function ago(ms: number, now: number): string {
  const minutes = Math.floor((now - ms) / 60000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 24 * 60) return `${Math.floor(minutes / 60)}h`;
  if (minutes < 48 * 60) return "Yesterday";
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

interface ChatSwitcherProps {
  conversations: ConversationSummary[];
  currentId: string;
  disabled: boolean;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
}

/// The panel header's chat title; clicking it lists recent chats to switch to or delete.
export function ChatSwitcher({ conversations, currentId, disabled, onSelect, onDelete }: ChatSwitcherProps) {
  const [open, setOpen] = useState(false);
  const current = conversations.find((c) => c.id === currentId);
  const now = Date.now();

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          disabled={disabled}
          aria-label={`Chat: ${current?.title ?? "New chat"}. Show recent chats`}
          className="flex h-[30px] min-w-0 items-center gap-1.5 rounded-[7px] px-2 text-[13px] font-semibold text-foreground hover:bg-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring/70 disabled:opacity-60 data-[state=open]:bg-muted"
        >
          <span className="truncate">{current?.title ?? "New chat"}</span>
          <ChevronDown aria-hidden="true" className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={6}
          collisionPadding={8}
          className="z-40 w-80 rounded-[10px] border border-border bg-popover p-1 text-popover-foreground shadow-xl outline-none"
        >
          <div className="px-2 pb-1 pt-1.5 text-[11px] font-medium text-muted-foreground">Recent chats</div>
          <ul className="max-h-72 overflow-y-auto">
            {conversations.map((c) => (
              <li key={c.id} className="group flex items-center rounded-[7px] hover:bg-muted">
                <button
                  type="button"
                  aria-current={c.id === currentId ? "true" : undefined}
                  onClick={() => {
                    setOpen(false);
                    if (c.id !== currentId) onSelect(c.id);
                  }}
                  className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left text-[13px] outline-none focus-visible:bg-muted"
                >
                  <Check
                    aria-hidden="true"
                    className={`h-3.5 w-3.5 shrink-0 text-primary ${c.id === currentId ? "" : "invisible"}`}
                  />
                  <span className="truncate">{c.title}</span>
                  <span className="ml-auto shrink-0 pl-2 text-[11px] text-muted-foreground">
                    {ago(c.updated_at, now)}
                  </span>
                </button>
                <IconButton
                  label="Delete chat"
                  side="right"
                  className="mr-1 h-6 w-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 [&_svg]:h-3.5 [&_svg]:w-3.5"
                  onClick={() => onDelete(c.id)}
                >
                  <Trash2 />
                </IconButton>
              </li>
            ))}
          </ul>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
