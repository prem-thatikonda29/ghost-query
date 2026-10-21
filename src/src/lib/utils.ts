import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import type { KeyboardEvent } from "react";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const isMac = navigator.userAgent.includes("Mac");

/// How the platform's shortcut modifier is written in hints.
export const MOD = isMac ? "⌘" : "Ctrl+";

/// Up/Down arrow moves focus between a popup's `[data-item]` buttons (Down from
/// anywhere else in it starts at the first).
export function moveFocus(e: KeyboardEvent<HTMLElement>) {
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  const items = Array.from(e.currentTarget.querySelectorAll<HTMLElement>("[data-item]"));
  if (items.length === 0) return;
  e.preventDefault();
  const at = items.indexOf(document.activeElement as HTMLElement);
  const next = e.key === "ArrowDown" ? (at < 0 ? 0 : Math.min(at + 1, items.length - 1)) : Math.max(at - 1, 0);
  items[next].focus();
}
