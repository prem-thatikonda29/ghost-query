import { useState, useEffect, useRef } from "react";
import {
  ChatInput,
  ChatInputTextArea,
  ChatInputSubmit,
} from "./components/ui/chat-input";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "./components/ui/button";
import { Trash2, MessageSquare } from "lucide-react";

interface ConversationMessage {
  id: string;
  role: string;
  content: string;
  timestamp: number;
}

function App() {
  const [message, setMessage] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [response, setResponse] = useState("");
  const [error, setError] = useState("");
  const [conversationHistory, setConversationHistory] = useState<
    ConversationMessage[]
  >([]);
  const [showHistory, setShowHistory] = useState(false);
  const responseRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Set up event listeners for streaming responses
  useEffect(() => {
    const unlistenChunk = listen<string>("ai-response-chunk", (event) => {
      setResponse((prev) => prev + event.payload);
    });

    const unlistenDone = listen<string>("ai-response-done", (event) => {
      setIsLoading(false);
      setMessage("");
      // Refresh conversation history after response is complete
      loadConversationHistory();
    });

    const unlistenError = listen<string>("ai-response-error", (event) => {
      setError(event.payload);
      setIsLoading(false);
      console.error("AI request failed:", event.payload);
    });

    const unlistenCancelled = listen<string>(
      "ai-response-cancelled",
      (event) => {
        setIsLoading(false);
        console.log("Stream cancelled:", event.payload);
      }
    );

    return () => {
      unlistenChunk.then((unlisten) => unlisten());
      unlistenDone.then((unlisten) => unlisten());
      unlistenError.then((unlisten) => unlisten());
      unlistenCancelled.then((unlisten) => unlisten());
    };
  }, []);

  // Auto-focus input when component mounts
  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.focus();
    }
  }, []);

  // Load conversation history on mount
  useEffect(() => {
    loadConversationHistory();
  }, []);

  const loadConversationHistory = async () => {
    try {
      const history = await invoke<ConversationMessage[]>(
        "get_conversation_history"
      );
      setConversationHistory(history);
    } catch (err) {
      console.error("Failed to load conversation history:", err);
    }
  };

  const clearConversation = async () => {
    try {
      await invoke("clear_conversation");
      setConversationHistory([]);
      setResponse("");
      setError("");
    } catch (err) {
      console.error("Failed to clear conversation:", err);
    }
  };

  // Auto-scroll to bottom when response updates
  useEffect(() => {
    if (responseRef.current) {
      responseRef.current.scrollTop = responseRef.current.scrollHeight;
    }
  }, [response]);

  const handleSubmit = async () => {
    if (message.trim()) {
      console.log("Message:", message);
      setIsLoading(true);
      setError("");
      setResponse("");

      try {
        await invoke("ask_ai_stream", { prompt: message });
      } catch (err) {
        setError(err as string);
        setIsLoading(false);
        console.error("AI request failed:", err);
      }
    }
  };

  const handleStop = async () => {
    try {
      await invoke("stop_streaming");
      setIsLoading(false);
    } catch (err) {
      console.error("Failed to stop streaming:", err);
      setIsLoading(false);
    }
  };

  return (
    <div className="w-full flex flex-col h-screen p-4">
      <div className="flex-1 flex flex-col max-w-4xl mx-auto w-full min-h-0">
        {/* Header with controls */}
        <div className="flex justify-between items-center mb-4">
          <div className="flex items-center gap-2">
            <h1 className="text-lg font-semibold">Ghost Query</h1>
            {conversationHistory.length > 0 && (
              <span className="text-xs text-muted-foreground">
                {conversationHistory.length} messages
              </span>
            )}
          </div>
          <div className="flex gap-2">
            {conversationHistory.length > 0 && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setShowHistory(!showHistory)}
              >
                <MessageSquare className="h-4 w-4 mr-1" />
                {showHistory ? "Hide" : "Show"} History
              </Button>
            )}
            {conversationHistory.length > 0 && (
              <Button variant="outline" size="sm" onClick={clearConversation}>
                <Trash2 className="h-4 w-4 mr-1" />
                Clear
              </Button>
            )}
          </div>
        </div>

        {/* Conversation History */}
        {showHistory && conversationHistory.length > 0 && (
          <div className="mb-4 max-h-[30vh] overflow-y-auto border rounded-lg p-4 bg-muted/30">
            <h3 className="text-sm font-medium mb-2">Conversation History</h3>
            <div className="space-y-2">
              {conversationHistory.map((msg) => (
                <div
                  key={msg.id}
                  className={`text-xs p-2 rounded ${
                    msg.role === "user"
                      ? "bg-blue-100 dark:bg-blue-900/20 ml-4"
                      : "bg-green-100 dark:bg-green-900/20 mr-4"
                  }`}
                >
                  <div className="font-medium text-xs mb-1">
                    {msg.role === "user" ? "You" : "AI"}
                  </div>
                  <div className="whitespace-pre-wrap">
                    {msg.content.length > 200
                      ? `${msg.content.substring(0, 200)}...`
                      : msg.content}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Response Area - Scrollable */}
        {(response || isLoading) && (
          <div
            ref={responseRef}
            className="flex-1 mb-4 p-4 rounded-lg bg-muted/50 border overflow-y-auto max-h-[60vh]"
          >
            <div className="whitespace-pre-wrap text-sm leading-relaxed">
              {response}
              {isLoading && !response && (
                <span className="text-muted-foreground">Thinking...</span>
              )}
            </div>
          </div>
        )}

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
            <ChatInputTextArea
              ref={inputRef}
              placeholder="Type your question here"
            />
            <ChatInputSubmit />
          </ChatInput>
        </div>
      </div>
    </div>
  );
}

export default App;
