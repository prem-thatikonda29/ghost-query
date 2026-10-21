import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const isMac = navigator.userAgent.includes("Mac");

/// How the platform's shortcut modifier is written in hints.
export const MOD = isMac ? "⌘" : "Ctrl+";
