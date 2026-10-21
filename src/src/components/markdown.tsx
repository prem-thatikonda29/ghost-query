import { memo, useState } from "react";
import ReactMarkdown from "react-markdown";
import { invoke } from "@tauri-apps/api/core";
import { Check, Copy } from "lucide-react";
import { IconButton } from "./ui/icon-button";

// Defined at module level so React keeps the same component identity between renders;
// a component declared inside App would be remounted for every streamed chunk.

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <IconButton
      label={copied ? "Copied" : "Copy code"}
      side="left"
      className="absolute right-1.5 top-1.5 h-7 w-7 opacity-0 group-hover/code:opacity-100 focus-visible:opacity-100 [&_svg]:h-3.5 [&_svg]:w-3.5"
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
      {copied ? <Check /> : <Copy />}
    </IconButton>
  );
}

// react-markdown renders fenced code as <pre><code>. The <pre> is dropped (below) and
// drawn here instead, so a block is told apart from inline code by its language class
// or its line breaks.
function CodeBlock({ className, children, node: _node, ...props }: any) {
  const codeText = String(children).replace(/\n$/, "");
  const isBlock = /language-/.test(className || "") || codeText.includes("\n");

  if (isBlock) {
    return (
      <div className="group/code relative my-3">
        <pre className="m-0 overflow-x-auto rounded-lg border border-border bg-code px-3.5 py-3 font-mono text-[12.5px] leading-relaxed text-foreground">
          <code className={className} {...props}>
            {codeText}
          </code>
        </pre>
        <CopyButton text={codeText} />
      </div>
    );
  }

  return (
    <code className="rounded-[5px] bg-muted px-1 py-0.5 font-mono text-[0.88em] font-normal" {...props}>
      {children}
    </code>
  );
}

function PassThrough({ children }: { children?: React.ReactNode }) {
  return <>{children}</>;
}

// Model replies are untrusted. A plain link would navigate this (only) window away from
// the app, so links open in the system browser instead; the backend accepts http(s) only.
function Link({ href, children }: { href?: string; children?: React.ReactNode }) {
  return (
    <a
      href={href}
      title={href}
      className="cursor-pointer underline underline-offset-2"
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

const components = { code: CodeBlock, pre: PassThrough, a: Link, img: BlockedImage } as any;

/// Memoized so finished messages are not re-parsed while a new reply streams in.
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="gq-prose prose prose-sm max-w-none text-sm leading-relaxed prose-code:before:content-none prose-code:after:content-none">
      <ReactMarkdown components={components}>{text}</ReactMarkdown>
    </div>
  );
});
