import { memo, useState } from "react";
import ReactMarkdown from "react-markdown";
import { invoke } from "@tauri-apps/api/core";
import { Check, Copy } from "lucide-react";
import { Button } from "./ui/button";

// Defined at module level so React keeps the same component identity between renders;
// a component declared inside App would be remounted for every streamed chunk.

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Button
      size="sm"
      variant="ghost"
      className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity"
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text);
          setCopied(true);
          setTimeout(() => setCopied(false), 2000);
        } catch (err) {
          console.error("Failed to copy text:", err);
        }
      }}
    >
      {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
    </Button>
  );
}

function CodeBlock({ inline, className, children, ...props }: any) {
  const match = /language-(\w+)/.exec(className || "");
  const codeText = String(children).replace(/\n$/, "");

  if (!inline && match) {
    return (
      <div className="relative group">
        <pre className="bg-black text-gray-100 p-4 rounded-lg overflow-x-auto text-base">
          <code className={className} {...props}>
            {children}
          </code>
        </pre>
        <CopyButton text={codeText} />
      </div>
    );
  }

  return (
    <code className="bg-gray-100 dark:bg-gray-800 px-1 py-0.5 rounded text-sm" {...props}>
      {children}
    </code>
  );
}

// Model replies are untrusted. A plain link would navigate this (only) window away from
// the app, so links open in the system browser instead; the backend accepts http(s) only.
function Link({ href, children }: { href?: string; children?: React.ReactNode }) {
  return (
    <a
      href={href}
      title={href}
      className="underline cursor-pointer"
      onClick={(e) => {
        e.preventDefault();
        if (href) invoke("open_link", { url: href }).catch((err) => console.error(err));
      }}
    >
      {children}
    </a>
  );
}

// Remote images would be fetched automatically (a way for injected text to leak the
// conversation through the URL), so show the alt text instead.
function BlockedImage({ alt }: { alt?: string }) {
  return <span className="text-muted-foreground">[image: {alt || "blocked"}]</span>;
}

const components = { code: CodeBlock, a: Link, img: BlockedImage } as any;

/// Memoized so finished messages are not re-parsed while a new reply streams in.
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="text-sm leading-relaxed prose prose-sm max-w-none dark:prose-invert">
      <ReactMarkdown components={components}>{text}</ReactMarkdown>
    </div>
  );
});
