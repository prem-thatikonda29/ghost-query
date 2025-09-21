import { useState, useEffect, useRef } from "react";
import {
  ChatInput,
  ChatInputTextArea,
  ChatInputSubmit,
} from "./components/ui/chat-input";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "./components/ui/button";
import { ModelSelector } from "./components/ui/model-selector";
import { Trash2, MessageSquare, Copy, Check } from "lucide-react";
import ReactMarkdown from "react-markdown";

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
  const [selectedModel, setSelectedModel] = useState("gemini-1.5-flash");
  const [copiedStates, setCopiedStates] = useState<{ [key: string]: boolean }>(
    {}
  );
  const [fullResponseCopied, setFullResponseCopied] = useState(false);
  const responseRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Set up event listeners for streaming responses
  useEffect(() => {
    const unlistenChunk = listen<string>("ai-response-chunk", (event) => {
      setResponse((prev) => prev + event.payload);
    });

    const unlistenDone = listen<string>("ai-response-done", (_event) => {
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
        await invoke("ask_ai_stream", {
          prompt: message,
          model: selectedModel,
        });
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

  const copyToClipboard = async (
    text: string,
    type: "code" | "full",
    codeId?: string
  ) => {
    try {
      await navigator.clipboard.writeText(text);

      if (type === "code" && codeId) {
        setCopiedStates((prev) => ({ ...prev, [codeId]: true }));
        setTimeout(() => {
          setCopiedStates((prev) => ({ ...prev, [codeId]: false }));
        }, 2000);
      } else if (type === "full") {
        setFullResponseCopied(true);
        setTimeout(() => setFullResponseCopied(false), 2000);
      }
    } catch (err) {
      console.error("Failed to copy text:", err);
    }
  };

  // Function to normalize streaming text (remove invisible characters)
  const normalizeStreamingText = (text: string): string => {
    if (!text) return text;
    return (
      text
        // convert zero-width joiners/spaces to a normal space
        .replace(/[\u200b\u200c\u200d\u2060]/g, " ")
        // normalize non-breaking/narrow spaces to a normal space
        .replace(/[\u00a0\u202f]/g, " ")
    );
  };

  // Function to clean up currency and percentage formatting
  const cleanCurrencyAndPercent = (text: string): string => {
    if (!text) return text;

    // Fix common currency formatting issues
    let cleaned = text
      // Fix broken currency like "$517.93" -> "$517.93"
      .replace(/\$(\d+\.?\d*)/g, "$$$1")
      // Fix percentage formatting
      .replace(/(\d+(?:\.\d+)?)\s*%/g, "$1%")
      // Fix broken numbers with missing spaces
      .replace(/(\d+)([A-Za-z])/g, "$1 $2")
      // Fix broken decimal points
      .replace(/(\d+)\.(\d+)/g, "$1.$2")
      // Normalize multiple spaces
      .replace(/\s+/g, " ")
      // Fix broken sentences
      .replace(/([.!?])([A-Z])/g, "$1 $2");

    return cleaned;
  };

  // Function to parse and format JSON responses
  const formatResponse = (response: string) => {
    try {
      // Clean the response first - remove any extra text before/after JSON
      let cleanResponse = response.trim();

      // Try to find JSON object boundaries
      const jsonStart = cleanResponse.indexOf("{");
      const jsonEnd = cleanResponse.lastIndexOf("}");

      if (jsonStart !== -1 && jsonEnd !== -1 && jsonEnd > jsonStart) {
        cleanResponse = cleanResponse.substring(jsonStart, jsonEnd + 1);
      }

      // Try to parse as JSON
      const parsed = JSON.parse(cleanResponse);

      // Check if it's a structured response with our expected format
      if (
        parsed.summary &&
        parsed.details &&
        parsed.key_points &&
        parsed.status
      ) {
        // Clean up the content before formatting
        const cleanSummary = cleanCurrencyAndPercent(
          normalizeStreamingText(parsed.summary)
        );
        const cleanDetails = cleanCurrencyAndPercent(
          normalizeStreamingText(parsed.details)
        );
        const cleanKeyPoints = parsed.key_points.map((point: string) =>
          cleanCurrencyAndPercent(normalizeStreamingText(point))
        );

        return `## ${cleanSummary}\n\n${cleanDetails}\n\n### Key Points\n\n${cleanKeyPoints
          .map((point: string) => `- ${point}`)
          .join("\n")}`;
      }
    } catch (e) {
      // If JSON parsing fails, try to extract key information manually
      console.log("JSON parsing failed, attempting manual extraction");

      // Look for common patterns in the malformed response
      const summaryMatch = response.match(/"summary":\s*"([^"]+)"/);
      const detailsMatch = response.match(/"details":\s*"([^"]+)"/);
      const keyPointsMatch = response.match(/"key_points":\s*\[(.*?)\]/);

      if (summaryMatch && detailsMatch) {
        let formatted = `## ${cleanCurrencyAndPercent(
          normalizeStreamingText(summaryMatch[1])
        )}\n\n${cleanCurrencyAndPercent(
          normalizeStreamingText(detailsMatch[1])
        )}`;

        if (keyPointsMatch) {
          const points = keyPointsMatch[1]
            .split(",")
            .map((point) => point.trim().replace(/"/g, ""))
            .filter((point) => point.length > 0)
            .map((point) =>
              cleanCurrencyAndPercent(normalizeStreamingText(point))
            );

          if (points.length > 0) {
            formatted += `\n\n### Key Points\n\n${points
              .map((point) => `- ${point}`)
              .join("\n")}`;
          }
        }

        return formatted;
      }
    }

    // If all else fails, apply basic cleaning to the raw response
    return cleanCurrencyAndPercent(normalizeStreamingText(response));
  };

  // Custom components for ReactMarkdown
  const CodeBlock = ({ node, inline, className, children, ...props }: any) => {
    const match = /language-(\w+)/.exec(className || "");
    const language = match ? match[1] : "";
    const codeText = String(children).replace(/\n$/, "");
    // Use a hash of the content as a stable ID
    const codeId = `code-${codeText.slice(0, 20).replace(/\s/g, "")}`;
    const isCopied = copiedStates[codeId];

    if (!inline && match) {
      return (
        <div className="relative group">
          <pre className="bg-black text-gray-100 p-4 rounded-lg overflow-x-auto text-base">
            <code className={className} {...props}>
              {children}
            </code>
          </pre>
          <Button
            size="sm"
            variant="ghost"
            className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity"
            onClick={() => copyToClipboard(codeText, "code", codeId)}
          >
            {isCopied ? (
              <Check className="h-4 w-4" />
            ) : (
              <Copy className="h-4 w-4" />
            )}
          </Button>
        </div>
      );
    }

    return (
      <code
        className="bg-gray-100 dark:bg-gray-800 px-1 py-0.5 rounded text-sm"
        {...props}
      >
        {children}
      </code>
    );
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
            <ModelSelector
              selectedModel={selectedModel}
              onModelChange={setSelectedModel}
              className="w-48"
            />
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
                  className={`text-xs p-2 rounded relative group ${
                    msg.role === "user"
                      ? "bg-blue-100 dark:bg-blue-900/20 ml-4"
                      : "bg-green-100 dark:bg-green-900/20 mr-4"
                  }`}
                >
                  <div className="font-medium text-xs mb-1 flex justify-between items-center">
                    <span>{msg.role === "user" ? "You" : "AI"}</span>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="opacity-0 group-hover:opacity-100 transition-opacity h-6 w-6 p-0"
                      onClick={() =>
                        copyToClipboard(formatResponse(msg.content), "full")
                      }
                    >
                      <Copy className="h-3 w-3" />
                    </Button>
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
            className="flex-1 mb-4 p-4 rounded-lg bg-muted/50 border overflow-y-auto max-h-[60vh] relative"
          >
            <div className="text-sm leading-relaxed prose prose-sm max-w-none dark:prose-invert">
              <ReactMarkdown
                components={{
                  code: CodeBlock,
                }}
              >
                {formatResponse(response)}
              </ReactMarkdown>
              {isLoading && !response && (
                <span className="text-muted-foreground">Thinking...</span>
              )}
              {/* Copy Full Response Button - positioned at end of content */}
              {response && (
                <div className="mt-8 flex justify-start">
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      copyToClipboard(formatResponse(response), "full")
                    }
                  >
                    {fullResponseCopied ? (
                      <Check className="h-4 w-4" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                  </Button>
                </div>
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
