"use client";

import { useState } from "react";
import { Button } from "./button";
import { ChevronDown, Bot } from "lucide-react";
import { cn } from "../../lib/utils";

export interface ModelOption {
  id: string;
  name: string;
  provider: string;
  description: string;
}

interface ModelSelectorProps {
  selectedModel: string;
  onModelChange: (modelId: string) => void;
  className?: string;
}

const MODELS: ModelOption[] = [
  {
    id: "gemini-2.0-flash",
    name: "Gemini 2.0 Flash",
    provider: "Google",
    description: "Advanced Gemini model for fast responses",
  },
  // {
  //   id: "gemini-1.5-pro",
  //   name: "Gemini 1.5 Pro",
  //   provider: "Google",
  //   description: "Most capable model for complex tasks",
  // },
  // {
  //   id: "sonar",
  //   name: "Perplexity Sonar",
  //   provider: "Perplexity",
  //   description: "Fast answers with reliable search results",
  // },
];

export function ModelSelector({
  selectedModel,
  onModelChange,
  className,
}: ModelSelectorProps) {
  const [isOpen, setIsOpen] = useState(false);

  const selectedModelData =
    MODELS.find((model) => model.id === selectedModel) || MODELS[0];

  return (
    <>
      {MODELS.length === 1 ? (
        <div className={cn("flex items-center gap-2", className)}>
          <Bot className="h-4 w-4" />
          <span className="text-sm font-medium">{selectedModelData.name}</span>
        </div>
      ) : (
        <div className={cn("relative", className)}>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setIsOpen(!isOpen)}
            className="w-full justify-between"
          >
            <div className="flex items-center gap-2">
              <Bot className="h-4 w-4" />
              <span className="text-sm font-medium">
                {selectedModelData.name}
              </span>
            </div>
            <ChevronDown
              className={cn(
                "h-4 w-4 transition-transform",
                isOpen && "rotate-180"
              )}
            />
          </Button>

          {isOpen && (
            <div className="absolute top-full left-0 right-0 mt-1 bg-background border rounded-md shadow-lg z-50">
              <div className="p-1">
                {MODELS.map((model) => (
                  <button
                    key={model.id}
                    onClick={() => {
                      onModelChange(model.id);
                      setIsOpen(false);
                    }}
                    className={cn(
                      "w-full text-left p-2 rounded-sm hover:bg-muted transition-colors",
                      selectedModel === model.id && "bg-muted"
                    )}
                  >
                    <div className="flex items-center justify-between">
                      <div>
                        <div className="font-medium text-sm">{model.name}</div>
                      </div>
                    </div>
                    <div className="text-xs text-muted-foreground mt-1">
                      {model.description}
                    </div>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </>
  );
}
