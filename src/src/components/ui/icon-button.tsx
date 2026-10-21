import * as React from "react";
import * as Tooltip from "@radix-ui/react-tooltip";
import { cn } from "../../lib/utils";

interface IconButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /// What the button does; also its accessible name.
  label: string;
  /// Keyboard shortcut shown in the tooltip, e.g. "⌘N".
  shortcut?: string;
  side?: "top" | "right" | "bottom" | "left";
}

/// An icon-only button whose name (and shortcut) appears in a tooltip on hover or
/// keyboard focus. Needs a `Tooltip.Provider` above it (App has one).
export const IconButton = React.forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, shortcut, side = "bottom", className, children, ...props },
  ref
) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <button
          ref={ref}
          type="button"
          aria-label={label}
          aria-keyshortcuts={shortcut}
          className={cn(
            "inline-flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-[7px] text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring/70 disabled:pointer-events-none disabled:opacity-40 [&_svg]:h-4 [&_svg]:w-4",
            className
          )}
          {...props}
        >
          {children}
        </button>
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content
          side={side}
          sideOffset={6}
          collisionPadding={4}
          className="z-50 flex items-center gap-2 whitespace-nowrap rounded-md bg-tooltip px-2 py-1 text-xs text-tooltip-foreground shadow-md"
        >
          {label}
          {shortcut && (
            <kbd className="rounded bg-tooltip-foreground/15 px-1 font-sans text-[11px]">{shortcut}</kbd>
          )}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
});
