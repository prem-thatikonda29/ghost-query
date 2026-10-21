import { useState, useEffect, useRef } from "react";
import { ChatInput, ChatInputTextArea } from "./components/ui/chat-input";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SettingsSheet, SettingsView } from "./components/settings-sheet";
import { ArrowUp, Check, Copy, Ghost, Minimize2, Plus, RefreshCw, Settings2, Square, X } from "lucide-react";
import { Markdown } from "./components/markdown";
import { Bar } from "./components/bar";
import { ChatSwitcher, ConversationSummary } from "./components/chat-switcher";
import { ModelPicker } from "./components/model-picker";
import { IconButton } from "./components/ui/icon-button";
import { MOD } from "./lib/utils";

interface StoredMessage {
  id: number;
  role: "user" | "assistant";
  content: string;
  status: "complete" | "cancelled";
  timestamp: number;
}

// Asking from the bar after this long without a message starts a new chat.
const QUIET_MS = 5 * 60 * 1000;

// The window is either a one-line bar or the chat panel (see window.rs).
type WindowMode = "bar" | "panel";

const prefersReducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function App() {
  const [message, setMessage] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  // Text streamed so far for the reply in flight (not stored until it finishes).
  const [response, setResponse] = useState("");
  // The user's message while its reply is in flight (also not stored yet).
  const [pendingUser, setPendingUser] = useState("");
  const [regenerating, setRegenerating] = useState(false);
  const [error, setError] = useState("");
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [currentId, setCurrentId] = useState("");
  const [messages, setMessages] = useState<StoredMessage[]>([]);
  const [showSettings, setShowSettings] = useState(false);
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  // Refreshed whenever the window is summoned, for the bar's quiet-period check.
  const [now, setNow] = useState(() => Date.now());
  const [startupNotices, setStartupNotices] = useState<string[]>([]);
  const [copiedStates, setCopiedStates] = useState<{ [key: string]: boolean }>({});
  const [mode, setMode] = useState<WindowMode>("bar");
  const modeRef = useRef<WindowMode>("bar");
  const currentIdRef = useRef("");
  const threadRef = useRef<HTMLDivElement>(null);
  // Whether the thread follows new text; off while the user has scrolled up to read.
  const followRef = useRef(true);
  // The prompt being answered, cleared from the composer on send and put back if
  // the request fails before anything is saved.
  const inFlightPrompt = useRef("");
  // Read out by screen readers, which are not sent every streamed chunk.
  const [announcement, setAnnouncement] = useState("");
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const barRef = useRef<HTMLInputElement>(null);

  // Growing: the window is resized first, then the panel is drawn into it.
  // Shrinking: the bar is drawn first, then the window shrinks around it.
  const expand = async () => {
    if (modeRef.current === "panel") return;
    modeRef.current = "panel";
    try {
      await invoke("set_window_mode", { mode: "panel", animate: !prefersReducedMotion() });
    } catch (err) {
      console.error("Failed to expand the window:", err);
    }
    setMode("panel");
  };

  const collapse = async () => {
    if (modeRef.current === "bar") return;
    modeRef.current = "bar";
    setMode("bar");
    setShowSettings(false);
    setPickerOpen(false);
    try {
      await invoke("set_window_mode", { mode: "bar", animate: !prefersReducedMotion() });
    } catch (err) {
      console.error("Failed to collapse the window:", err);
    }
  };

  const openSettings = async () => {
    await expand();
    setShowSettings(true);
  };

  const refreshConversations = async () => {
    const list = await invoke<ConversationSummary[]>("list_conversations");
    setConversations(list);
    return list;
  };

  const loadMessages = async (id: string) => {
    setMessages(await invoke<StoredMessage[]>("get_messages", { conversationId: id }));
  };

  const selectConversation = async (id: string) => {
    currentIdRef.current = id;
    followRef.current = true;
    setCurrentId(id);
    setResponse("");
    setPendingUser("");
    setError("");
    await loadMessages(id);
  };

  // Called from the stream listeners, which are registered once, so it reads the
  // current conversation from a ref rather than from stale state.
  const refresh = async () => {
    if (!currentIdRef.current) return;
    await Promise.all([loadMessages(currentIdRef.current), refreshConversations()]);
  };

  useEffect(() => {
    const unlistenChunk = listen<string>("ai-response-chunk", (event) => {
      setResponse((prev) => prev + event.payload);
    });

    const unlistenDone = listen<string>("ai-response-done", async () => {
      inFlightPrompt.current = "";
      await refresh();
      setResponse("");
      setPendingUser("");
      setRegenerating(false);
      setIsLoading(false);
      setAnnouncement("Reply finished");
    });

    const unlistenError = listen<string>("ai-response-error", (event) => {
      restorePrompt();
      setError(event.payload);
      setResponse("");
      setPendingUser("");
      setRegenerating(false);
      setIsLoading(false);
    });

    const unlistenCancelled = listen<string>("ai-response-cancelled", async (event) => {
      // A stopped reply with text is saved with its prompt; stopped before any text,
      // nothing is saved and the prompt goes back into the composer.
      if (event.payload) inFlightPrompt.current = "";
      else restorePrompt();
      setAnnouncement("Reply stopped");
      await refresh();
      setResponse("");
      setPendingUser("");
      setRegenerating(false);
      setIsLoading(false);
    });

    // Dismissing the window (Esc, the shortcut) folds it back into the bar.
    const unlistenCollapsed = listen("window-collapsed", () => {
      modeRef.current = "bar";
      setMode("bar");
      setShowSettings(false);
      setPickerOpen(false);
    });
    const unlistenShown = listen("window-shown", () => {
      setNow(Date.now());
      (modeRef.current === "bar" ? barRef.current : inputRef.current)?.focus();
    });

    return () => {
      unlistenCollapsed.then((unlisten) => unlisten());
      unlistenShown.then((unlisten) => unlisten());
      unlistenChunk.then((unlisten) => unlisten());
      unlistenDone.then((unlisten) => unlisten());
      unlistenError.then((unlisten) => unlisten());
      unlistenCancelled.then((unlisten) => unlisten());
    };
  }, []);

  // Frameless ghost window, so the keyboard does what a title bar would: Esc steps back
  // (settings → panel → bar → hidden; the global shortcut brings it back) and Cmd/Ctrl+Q
  // quits, since there is no Dock icon either.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      // An open popup (chat list, model picker, tooltip) closes first; Radix marks
      // that Esc as handled.
      if (e.key === "Escape" && !e.isComposing && !e.defaultPrevented) {
        if (showSettings) setShowSettings(false);
        else if (mode === "panel") collapse();
        else invoke("hide_window");
      } else if (mod && e.key.toLowerCase() === "q") {
        e.preventDefault();
        invoke("quit_app");
      } else if (mod && e.key.toLowerCase() === "n") {
        e.preventDefault();
        if (!isLoading) newChat();
      } else if (mod && e.key === "ArrowUp" && mode === "bar") {
        e.preventDefault();
        expand();
      } else if (mod && e.key === ",") {
        e.preventDefault();
        openSettings();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [mode, showSettings, isLoading, messages]);

  // Startup problems need room to be read, so they open the panel.
  useEffect(() => {
    invoke<string[]>("get_startup_notices")
      .then((notices) => {
        setStartupNotices(notices);
        if (notices.length > 0) expand();
      })
      .catch(() => {});
  }, []);

  // Focus follows the mode: the bar's field, or the panel's composer.
  useEffect(() => {
    if (mode === "bar") barRef.current?.focus();
    else if (!showSettings) inputRef.current?.focus();
  }, [mode, showSettings]);

  // On mount: open the most recent conversation (or a fresh one) and load settings.
  useEffect(() => {
    (async () => {
      try {
        let list = await refreshConversations();
        if (list.length === 0) {
          await invoke("create_conversation");
          list = await refreshConversations();
        }
        await selectConversation(list[0].id);
      } catch (err) {
        setError(String(err));
      }
    })();

    invoke<SettingsView>("get_settings")
      .then((view) => {
        setSettings(view);
        // First run: the active provider needs a key, so open settings straight away.
        const active = view.providers.find((p) => p.id === view.active);
        if (active && active.needs_key && !active.has_key) openSettings();
      })
      .catch((err) => console.error("Failed to load settings:", err));
  }, []);

  useEffect(() => {
    if (threadRef.current && followRef.current) {
      threadRef.current.scrollTop = threadRef.current.scrollHeight;
    }
  }, [messages, response, pendingUser, error]);

  const onThreadScroll = () => {
    const el = threadRef.current;
    if (el) followRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
  };

  // Called from the stream listeners (registered once), so it only uses setters and
  // refs. Anything typed meanwhile is kept rather than overwritten.
  const restorePrompt = () => {
    const prompt = inFlightPrompt.current;
    inFlightPrompt.current = "";
    if (prompt) setMessage((typed) => typed || prompt);
  };

  // An empty current chat is reused rather than piling up untitled ones.
  const newChat = async () => {
    if (messages.length === 0 && currentIdRef.current) return;
    try {
      const created = await invoke<ConversationSummary>("create_conversation");
      await refreshConversations();
      await selectConversation(created.id);
    } catch (err) {
      setError(String(err));
    }
  };

  const deleteChat = async (id: string) => {
    try {
      await invoke("delete_conversation", { conversationId: id });
      let list = await refreshConversations();
      if (list.length === 0) {
        await invoke("create_conversation");
        list = await refreshConversations();
      }
      if (id === currentIdRef.current || !list.some((c) => c.id === currentIdRef.current)) {
        await selectConversation(list[0].id);
      }
    } catch (err) {
      setError(String(err));
    }
  };

  // Uses the ref, so a chat created just before (see submitFromBar) is the one asked in.
  const send = async (prompt: string, regenerate: boolean) => {
    setIsLoading(true);
    setError("");
    setResponse("");
    setRegenerating(regenerate);
    setPendingUser(regenerate ? "" : prompt);
    setAnnouncement("");
    followRef.current = true;
    if (!regenerate) {
      inFlightPrompt.current = prompt;
      setMessage("");
    }
    try {
      await invoke("ask_ai_stream", {
        conversationId: currentIdRef.current,
        prompt,
        regenerate,
      });
    } catch (err) {
      restorePrompt();
      setError(String(err));
      setPendingUser("");
      setRegenerating(false);
      setIsLoading(false);
    }
  };

  const handleSubmit = () => {
    if (message.trim() && !isLoading) send(message, false);
  };

  const lastTimestamp = messages[messages.length - 1]?.timestamp;
  const continuing = lastTimestamp !== undefined && now - lastTimestamp < QUIET_MS;

  // Asking from the bar grows the window into the panel to show the answer. After a
  // quiet spell the question starts a new chat instead of continuing the last one.
  const submitFromBar = async () => {
    if (!message.trim() || isLoading) return;
    if (lastTimestamp !== undefined && Date.now() - lastTimestamp >= QUIET_MS) await newChat();
    await expand();
    send(message, false);
  };


  const handleStop = async () => {
    try {
      await invoke("stop_streaming");
    } catch (err) {
      console.error("Failed to stop streaming:", err);
      setIsLoading(false);
    }
  };

  const copyToClipboard = async (text: string, key: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedStates((prev) => ({ ...prev, [key]: true }));
      setTimeout(() => setCopiedStates((prev) => ({ ...prev, [key]: false })), 2000);
    } catch (err) {
      console.error("Failed to copy text:", err);
    }
  };

  // The rounded surface of the transparent window (see index.css).
  const shell = "h-full w-full overflow-hidden rounded-[14px] border border-border bg-background text-foreground";
  const currentTitle = conversations.find((c) => c.id === currentId)?.title ?? "";

  if (mode === "bar") {
    return (
      <div className={shell}>
        <Bar
          ref={barRef}
          value={message}
          onChange={setMessage}
          onSubmit={submitFromBar}
          onExpand={expand}
          loading={isLoading}
          onStop={handleStop}
          placeholder={
            continuing && currentTitle
              ? `Follow up on “${currentTitle.length > 40 ? `${currentTitle.slice(0, 39)}…` : currentTitle}”`
              : "Ask anything…"
          }
        />
      </div>
    );
  }

  // While regenerating, the answer being replaced is hidden.
  const lastMessage = messages[messages.length - 1];
  const shownMessages =
    regenerating && lastMessage?.role === "assistant" ? messages.slice(0, -1) : messages;
  const lastAssistantId = lastMessage?.role === "assistant" ? lastMessage.id : null;
  const canSend = message.trim().length > 0;
  const isEmpty = shownMessages.length === 0 && !pendingUser && !isLoading && !error && startupNotices.length === 0;

  return (
    <div className={`${shell} flex flex-col`}>
      {/* Faded in, as it is drawn only once the window has grown to its full size */}
      <div className="gq-fade-in flex min-h-0 flex-1 flex-col">
        <SettingsSheet open={showSettings} onOpenChange={setShowSettings} onChanged={setSettings} />

        {/* Header: drags the window; chat title on the left, icon buttons on the right */}
        <div
          data-tauri-drag-region
          className="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-border px-2"
        >
          <ChatSwitcher
            conversations={conversations}
            currentId={currentId}
            disabled={isLoading}
            onSelect={selectConversation}
            onDelete={deleteChat}
          />
          <div className="flex shrink-0 gap-0.5">
            <IconButton label="New chat" shortcut={`${MOD}N`} onClick={newChat} disabled={isLoading}>
              <Plus />
            </IconButton>
            <IconButton label="Collapse to bar" shortcut="Esc" onClick={collapse}>
              <Minimize2 />
            </IconButton>
            <IconButton label="Settings" shortcut={`${MOD},`} onClick={openSettings}>
              <Settings2 />
            </IconButton>
          </div>
        </div>

        {/* Thread */}
        <div
          ref={threadRef}
          onScroll={onThreadScroll}
          aria-label="Conversation"
          aria-busy={isLoading}
          className="min-h-0 flex-1 overflow-y-auto"
        >
          <div className="mx-auto flex min-h-full w-full max-w-[640px] flex-col gap-5 px-5 py-6">
            {isEmpty && (
              <div className="m-auto flex flex-col items-center gap-2 text-muted-foreground">
                <Ghost aria-hidden="true" className="h-6 w-6 opacity-60" />
                <p className="text-[13px]">Ask anything</p>
              </div>
            )}

            {startupNotices.map((notice) => (
              <div key={notice} role="status" className="rounded-lg bg-muted px-3 py-2 text-xs text-muted-foreground">
                {notice}
              </div>
            ))}

            {shownMessages.map((msg) =>
              msg.role === "user" ? (
                <div
                  key={msg.id}
                  className="max-w-[80%] self-end whitespace-pre-wrap rounded-[14px] bg-muted px-3 py-2 text-sm leading-relaxed"
                >
                  {msg.content}
                </div>
              ) : (
                <div key={msg.id} className="group flex flex-col gap-1">
                  <Markdown text={msg.content} />
                  <div
                    className={`-ml-1.5 flex items-center gap-0.5 ${
                      msg.id === lastAssistantId ? "" : "opacity-0 focus-within:opacity-100 group-hover:opacity-100"
                    }`}
                  >
                    <IconButton
                      label={copiedStates[`msg-${msg.id}`] ? "Copied" : "Copy reply"}
                      className="h-7 w-7 [&_svg]:h-[15px] [&_svg]:w-[15px]"
                      onClick={() => copyToClipboard(msg.content, `msg-${msg.id}`)}
                    >
                      {copiedStates[`msg-${msg.id}`] ? <Check /> : <Copy />}
                    </IconButton>
                    {msg.id === lastAssistantId && !isLoading && (
                      <IconButton
                        label="Regenerate reply"
                        className="h-7 w-7 [&_svg]:h-[15px] [&_svg]:w-[15px]"
                        onClick={() => send("", true)}
                      >
                        <RefreshCw />
                      </IconButton>
                    )}
                    {msg.status === "cancelled" && (
                      <span className="ml-1 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
                        Incomplete
                      </span>
                    )}
                  </div>
                </div>
              )
            )}

            {pendingUser && (
              <div className="max-w-[80%] self-end whitespace-pre-wrap rounded-[14px] bg-muted px-3 py-2 text-sm leading-relaxed">
                {pendingUser}
              </div>
            )}

            {isLoading &&
              (response ? (
                <Markdown text={response} />
              ) : (
                <span className="animate-pulse text-sm text-muted-foreground">Thinking…</span>
              ))}

            {error && (
              <div
                role="alert"
                className="flex items-start gap-3 rounded-lg bg-destructive/10 py-2 pl-3 pr-1.5 text-[13px] text-destructive"
              >
                <p className="min-w-0 flex-1 py-0.5">{error}</p>
                {/settings/i.test(error) && (
                  <button
                    type="button"
                    onClick={openSettings}
                    className="shrink-0 rounded py-0.5 font-medium underline-offset-2 outline-none hover:underline focus-visible:underline"
                  >
                    Open Settings
                  </button>
                )}
                <IconButton
                  label="Dismiss"
                  className="h-6 w-6 text-destructive hover:bg-destructive/10 hover:text-destructive [&_svg]:h-3.5 [&_svg]:w-3.5"
                  onClick={() => setError("")}
                >
                  <X />
                </IconButton>
              </div>
            )}
          </div>
        </div>
        <div aria-live="polite" className="sr-only">
          {announcement}
        </div>

        {/* Composer */}
        <div className="shrink-0 px-4 pb-4">
          <ChatInput
            variant="unstyled"
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onSubmit={handleSubmit}
            loading={isLoading}
            onStop={handleStop}
            className="mx-auto max-w-[672px] flex-col items-stretch gap-1.5 rounded-xl border border-border bg-field px-3.5 pb-2.5 pt-3 transition-colors focus-within:border-primary/50"
          >
            <ChatInputTextArea
              ref={inputRef}
              variant="unstyled"
              aria-label="Message"
              placeholder={messages.length > 0 ? "Ask a follow-up…" : "Ask anything…"}
              className="max-h-40 min-h-0 rounded-none bg-transparent p-0 text-sm leading-relaxed placeholder:text-muted-foreground"
            />
            <div className="flex items-center justify-between gap-2">
              <ModelPicker
                view={settings}
                open={pickerOpen}
                onOpenChange={setPickerOpen}
                onPicked={setSettings}
                onManage={openSettings}
                onError={setError}
              />
              {isLoading ? (
                <IconButton
                  label="Stop"
                  side="top"
                  className="h-8 w-8 rounded-lg bg-primary text-white hover:bg-primary/90 hover:text-white [&_svg]:h-3.5 [&_svg]:w-3.5"
                  onClick={handleStop}
                >
                  <Square />
                </IconButton>
              ) : (
                <IconButton
                  label="Send"
                  shortcut="↵"
                  side="top"
                  disabled={!canSend}
                  className="h-8 w-8 rounded-lg bg-primary text-white hover:bg-primary/90 hover:text-white disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100"
                  onClick={handleSubmit}
                >
                  <ArrowUp />
                </IconButton>
              )}
            </div>
          </ChatInput>
        </div>
      </div>
    </div>
  );
}

export default App;
