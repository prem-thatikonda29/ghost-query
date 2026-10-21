import { useState, useEffect, useRef } from "react";
import {
  ChatInput,
  ChatInputTextArea,
  ChatInputSubmit,
} from "./components/ui/chat-input";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "./components/ui/button";
import { SettingsPage, SettingsView } from "./components/ui/settings-page";
import { Trash2, Copy, Check, Settings, Plus, RefreshCw } from "lucide-react";
import { Markdown } from "./components/markdown";

interface ConversationSummary {
  id: string;
  title: string;
  created_at: number;
  updated_at: number;
}

interface StoredMessage {
  id: number;
  role: "user" | "assistant";
  content: string;
  status: "complete" | "cancelled";
  timestamp: number;
}

const activeLabelFor = (view: SettingsView) => {
  const active = view.providers.find((p) => p.id === view.active);
  return active ? `${active.name} · ${active.model || "no model"}` : "";
};

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
  const [activeLabel, setActiveLabel] = useState("");
  const [startupNotices, setStartupNotices] = useState<string[]>([]);
  const [copiedStates, setCopiedStates] = useState<{ [key: string]: boolean }>({});
  const currentIdRef = useRef("");
  const threadRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

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
      await refresh();
      setResponse("");
      setPendingUser("");
      setRegenerating(false);
      setIsLoading(false);
      setMessage("");
    });

    const unlistenError = listen<string>("ai-response-error", (event) => {
      setError(event.payload);
      setResponse("");
      setPendingUser("");
      setRegenerating(false);
      setIsLoading(false);
    });

    const unlistenCancelled = listen<string>("ai-response-cancelled", async (event) => {
      // A stopped reply with text is saved; the prompt then leaves the input box.
      if (event.payload) setMessage("");
      await refresh();
      setResponse("");
      setPendingUser("");
      setRegenerating(false);
      setIsLoading(false);
    });

    return () => {
      unlistenChunk.then((unlisten) => unlisten());
      unlistenDone.then((unlisten) => unlisten());
      unlistenError.then((unlisten) => unlisten());
      unlistenCancelled.then((unlisten) => unlisten());
    };
  }, []);

  // Frameless ghost window: Esc hides it (the global shortcut brings it back) and
  // Ctrl/Cmd+Q quits, since there is no Dock icon or title bar to do either.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        invoke("hide_window");
      } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "q") {
        e.preventDefault();
        invoke("quit_app");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    invoke<string[]>("get_startup_notices")
      .then(setStartupNotices)
      .catch(() => {});
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // On mount: open the most recent conversation (or a fresh one) and load settings.
  useEffect(() => {
    inputRef.current?.focus();
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
        setActiveLabel(activeLabelFor(view));
        // First run: the active provider needs a key, so open settings straight away.
        const active = view.providers.find((p) => p.id === view.active);
        if (active && active.needs_key && !active.has_key) setShowSettings(true);
      })
      .catch((err) => console.error("Failed to load settings:", err));
  }, []);

  useEffect(() => {
    if (threadRef.current) {
      threadRef.current.scrollTop = threadRef.current.scrollHeight;
    }
  }, [messages, response, pendingUser]);

  const newChat = async () => {
    try {
      const created = await invoke<ConversationSummary>("create_conversation");
      await refreshConversations();
      await selectConversation(created.id);
    } catch (err) {
      setError(String(err));
    }
  };

  const deleteChat = async () => {
    try {
      await invoke("delete_conversation", { conversationId: currentId });
      let list = await refreshConversations();
      if (list.length === 0) {
        await invoke("create_conversation");
        list = await refreshConversations();
      }
      await selectConversation(list[0].id);
    } catch (err) {
      setError(String(err));
    }
  };

  const send = async (prompt: string, regenerate: boolean) => {
    setIsLoading(true);
    setError("");
    setResponse("");
    setRegenerating(regenerate);
    setPendingUser(regenerate ? "" : prompt);
    try {
      await invoke("ask_ai_stream", {
        conversationId: currentId,
        prompt,
        regenerate,
      });
    } catch (err) {
      setError(String(err));
      setPendingUser("");
      setRegenerating(false);
      setIsLoading(false);
    }
  };

  const handleSubmit = () => {
    if (message.trim() && !isLoading) send(message, false);
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

  if (showSettings) {
    return (
      <SettingsPage
        onBack={() => setShowSettings(false)}
        onChanged={(view) => setActiveLabel(activeLabelFor(view))}
      />
    );
  }

  // While regenerating, the answer being replaced is hidden.
  const lastMessage = messages[messages.length - 1];
  const shownMessages =
    regenerating && lastMessage?.role === "assistant" ? messages.slice(0, -1) : messages;
  const lastAssistantId = lastMessage?.role === "assistant" ? lastMessage.id : null;

  return (
    <div className="w-full flex flex-col h-screen p-4">
      <div className="flex-1 flex flex-col max-w-4xl mx-auto w-full min-h-0">
        {/* Header with controls */}
        <div data-tauri-drag-region className="flex justify-between items-center mb-4 gap-2">
          <div data-tauri-drag-region className="flex items-center gap-2 min-w-0">
            <h1 data-tauri-drag-region className="text-lg font-semibold shrink-0">Ghost Query</h1>
            <select
              className="min-w-0 max-w-xs rounded-lg border border-input bg-background px-2 py-1 text-sm"
              value={currentId}
              disabled={isLoading}
              onChange={(e) => selectConversation(e.target.value)}
              title="Conversations"
            >
              {conversations.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.title}
                </option>
              ))}
            </select>
          </div>
          <div className="flex gap-2 shrink-0">
            <Button variant="outline" size="sm" onClick={newChat} disabled={isLoading}>
              <Plus className="h-4 w-4 mr-1" />
              New chat
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={deleteChat}
              disabled={isLoading || messages.length === 0}
              title="Delete this conversation"
            >
              <Trash2 className="h-4 w-4 mr-1" />
              Delete
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowSettings(true)}
              title="Provider settings"
            >
              <Settings className="h-4 w-4 mr-1" />
              {activeLabel || "Settings"}
            </Button>
          </div>
        </div>

        {startupNotices.map((notice) => (
          <div key={notice} className="mb-3 p-3 rounded-lg bg-muted border text-xs text-muted-foreground">
            {notice}
          </div>
        ))}

        {/* Message thread */}
        <div ref={threadRef} className="flex-1 min-h-0 overflow-y-auto mb-4 space-y-3">
          {shownMessages.map((msg) =>
            msg.role === "user" ? (
              <div key={msg.id} className="flex justify-end">
                <div className="max-w-[85%] rounded-lg bg-blue-100 dark:bg-blue-900/20 px-3 py-2 text-sm whitespace-pre-wrap">
                  {msg.content}
                </div>
              </div>
            ) : (
              <div key={msg.id} className="rounded-lg bg-muted/50 border p-4 group">
                <Markdown text={msg.content} />
                <div className="mt-2 flex items-center gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => copyToClipboard(msg.content, `msg-${msg.id}`)}
                    title="Copy"
                  >
                    {copiedStates[`msg-${msg.id}`] ? (
                      <Check className="h-4 w-4" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                  </Button>
                  {msg.id === lastAssistantId && !isLoading && (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => send("", true)}
                      title="Regenerate"
                    >
                      <RefreshCw className="h-4 w-4" />
                    </Button>
                  )}
                  {msg.status === "cancelled" && (
                    <span className="text-xs text-muted-foreground">(incomplete)</span>
                  )}
                </div>
              </div>
            )
          )}

          {pendingUser && (
            <div className="flex justify-end">
              <div className="max-w-[85%] rounded-lg bg-blue-100 dark:bg-blue-900/20 px-3 py-2 text-sm whitespace-pre-wrap">
                {pendingUser}
              </div>
            </div>
          )}

          {isLoading && (
            <div className="rounded-lg bg-muted/50 border p-4">
              {response ? (
                <Markdown text={response} />
              ) : (
                <span className="text-sm text-muted-foreground">Thinking...</span>
              )}
            </div>
          )}
        </div>

        {/* Error Display */}
        {error && (
          <div className="mb-4 p-4 rounded-lg bg-destructive/10 border border-destructive/20">
            <div className="text-sm text-destructive">{error}</div>
          </div>
        )}

        {/* Input Area */}
        <div className="w-full flex-shrink-0">
          <ChatInput
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onSubmit={handleSubmit}
            loading={isLoading}
            onStop={handleStop}
          >
            <ChatInputTextArea ref={inputRef} placeholder="Type your question here" />
            <ChatInputSubmit />
          </ChatInput>
        </div>
      </div>
    </div>
  );
}

export default App;
