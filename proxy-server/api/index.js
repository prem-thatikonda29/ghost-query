const express = require("express");
const cors = require("cors");
const helmet = require("helmet");
const rateLimit = require("express-rate-limit");
const axios = require("axios");

const app = express();

// Security middleware
app.use(helmet());
app.use(
  cors({
    origin: "*", // Allow all origins for open access
    methods: ["GET", "POST"],
    allowedHeaders: ["Content-Type", "Authorization"],
  })
);

// Body parsing middleware
app.use(express.json({ limit: "10mb" }));
app.use(express.urlencoded({ extended: true, limit: "10mb" }));

// Smart rate limiting for distributed usage
// Different limits for different endpoints
const createRateLimit = (windowMs, max, message) =>
  rateLimit({
    windowMs,
    max,
    message: { error: message },
    standardHeaders: true,
    legacyHeaders: false,
    // Use IP + User-Agent for better identification
    keyGenerator: (req) => `${req.ip}-${req.get("User-Agent")}`,
    // Skip rate limiting for successful requests (helpful for debugging)
    skipSuccessfulRequests: false,
  });

// Rate limits optimized for AI API usage
const geminiRateLimit = createRateLimit(
  15 * 60 * 1000, // 15 minutes
  50, // 50 requests per 15 minutes
  "Gemini API rate limit exceeded. Please try again later."
);

const perplexityRateLimit = createRateLimit(
  15 * 60 * 1000, // 15 minutes
  30, // 30 requests per 15 minutes (Perplexity is more expensive)
  "Perplexity API rate limit exceeded. Please try again later."
);

// General API rate limit
const generalRateLimit = createRateLimit(
  15 * 60 * 1000, // 15 minutes
  100, // 100 total requests per 15 minutes
  "General API rate limit exceeded. Please try again later."
);

// Health check endpoint
app.get("/health", (req, res) => {
  res.json({
    status: "healthy",
    timestamp: new Date().toISOString(),
    service: "Ghost Query Proxy Server",
  });
});

// Gemini API endpoint with streaming support
app.post("/api/gemini", generalRateLimit, geminiRateLimit, async (req, res) => {
  try {
    const {
      model,
      prompt,
      temperature = 0.7,
      maxTokens = 2048,
      stream = false,
    } = req.body;

    if (!model || !prompt) {
      return res.status(400).json({
        error: "Missing required fields: model and prompt are required",
      });
    }

    if (!process.env.GEMINI_API_KEY) {
      return res.status(500).json({
        error: "Gemini API key not configured",
      });
    }

    const url = `https://generativelanguage.googleapis.com/v1beta/models/${model}:streamGenerateContent?key=${process.env.GEMINI_API_KEY}`;

    const requestBody = {
      contents: [
        {
          parts: [{ text: prompt }],
        },
      ],
      generationConfig: {
        temperature,
        maxOutputTokens: maxTokens,
      },
    };

    if (stream) {
      // For now, use non-streaming for Gemini and simulate streaming
      const response = await axios.post(
        url.replace("streamGenerateContent", "generateContent"),
        requestBody,
        {
          headers: {
            "Content-Type": "application/json",
          },
          timeout: 30000,
        }
      );

      if (response.data.candidates && response.data.candidates.length > 0) {
        const content =
          response.data.candidates[0].content?.parts?.[0]?.text || "";

        // Set up streaming response headers
        res.setHeader("Content-Type", "text/event-stream");
        res.setHeader("Cache-Control", "no-cache");
        res.setHeader("Connection", "keep-alive");
        res.setHeader("Access-Control-Allow-Origin", "*");
        res.setHeader("Access-Control-Allow-Headers", "Cache-Control");

        // Simulate streaming by sending content in chunks
        const words = content.split(" ");
        let index = 0;

        const sendChunk = () => {
          if (index < words.length) {
            const chunk = words[index] + (index < words.length - 1 ? " " : "");
            res.write(
              `data: ${JSON.stringify({
                content: chunk,
                model,
                provider: "gemini",
              })}\n\n`
            );
            index++;
            setTimeout(sendChunk, 50); // 50ms delay between chunks
          } else {
            res.write("data: [DONE]\n\n");
            res.end();
          }
        };

        sendChunk();
      } else {
        res.setHeader("Content-Type", "text/event-stream");
        res.write(
          `data: ${JSON.stringify({
            error: "No response from Gemini API",
          })}\n\n`
        );
        res.end();
      }
    } else {
      // Non-streaming response (fallback)
      const response = await axios.post(
        url.replace("streamGenerateContent", "generateContent"),
        requestBody,
        {
          headers: {
            "Content-Type": "application/json",
          },
          timeout: 30000,
        }
      );

      if (response.data.candidates && response.data.candidates.length > 0) {
        const content =
          response.data.candidates[0].content?.parts?.[0]?.text || "";
        res.json({
          success: true,
          content,
          model: model,
          provider: "gemini",
        });
      } else {
        res.status(500).json({
          error: "No response from Gemini API",
        });
      }
    }
  } catch (error) {
    console.error("Gemini API Error:", error.response?.data || error.message);

    if (stream) {
      res.write(
        `data: ${JSON.stringify({ error: "Gemini API request failed" })}\n\n`
      );
      res.end();
    } else {
      if (error.response?.status === 429) {
        res.status(429).json({
          error: "Gemini API rate limit exceeded",
        });
      } else if (error.response?.status === 400) {
        res.status(400).json({
          error: "Invalid request to Gemini API",
          details: error.response.data?.error?.message || "Bad request",
        });
      } else {
        res.status(500).json({
          error: "Gemini API request failed",
          details: error.response?.data?.error?.message || error.message,
        });
      }
    }
  }
});

// Perplexity API endpoint with streaming support
app.post(
  "/api/perplexity",
  generalRateLimit,
  perplexityRateLimit,
  async (req, res) => {
    try {
      const { model, prompt, stream = false } = req.body;

      if (!model || !prompt) {
        return res.status(400).json({
          error: "Missing required fields: model and prompt are required",
        });
      }

      if (!process.env.PERPLEXITY_API_KEY) {
        return res.status(500).json({
          error: "Perplexity API key not configured",
        });
      }

      const url = "https://api.perplexity.ai/chat/completions";

      const requestBody = {
        model: model,
        messages: [
          {
            role: "user",
            content: prompt,
          },
        ],
        stream: stream,
      };

      if (stream) {
        // Set up streaming response
        res.setHeader("Content-Type", "text/event-stream");
        res.setHeader("Cache-Control", "no-cache");
        res.setHeader("Connection", "keep-alive");
        res.setHeader("Access-Control-Allow-Origin", "*");
        res.setHeader("Access-Control-Allow-Headers", "Cache-Control");

        const response = await axios.post(url, requestBody, {
          headers: {
            Authorization: `Bearer ${process.env.PERPLEXITY_API_KEY}`,
            "Content-Type": "application/json",
          },
          responseType: "stream",
          timeout: 30000,
        });

        response.data.on("data", (chunk) => {
          const lines = chunk.toString().split("\n");
          for (const line of lines) {
            if (line.trim() && line.startsWith("data: ")) {
              const data = line.slice(6);
              if (data === "[DONE]") {
                res.write("data: [DONE]\n\n");
                res.end();
                return;
              }
              try {
                const parsed = JSON.parse(data);
                if (parsed.choices && parsed.choices[0]?.delta?.content) {
                  const content = parsed.choices[0].delta.content;
                  res.write(
                    `data: ${JSON.stringify({
                      content,
                      model,
                      provider: "perplexity",
                    })}\n\n`
                  );
                }
              } catch (e) {
                // Skip invalid JSON
              }
            }
          }
        });

        response.data.on("end", () => {
          res.write("data: [DONE]\n\n");
          res.end();
        });

        response.data.on("error", (error) => {
          console.error("Perplexity streaming error:", error);
          res.write(
            `data: ${JSON.stringify({ error: "Streaming error occurred" })}\n\n`
          );
          res.end();
        });
      } else {
        // Non-streaming response (fallback)
        const response = await axios.post(url, requestBody, {
          headers: {
            Authorization: `Bearer ${process.env.PERPLEXITY_API_KEY}`,
            "Content-Type": "application/json",
          },
          timeout: 30000,
        });

        if (response.data.choices && response.data.choices.length > 0) {
          const content = response.data.choices[0].message?.content || "";
          const citations = response.data.citations || [];
          const searchResults = response.data.search_results || [];

          res.json({
            success: true,
            content,
            citations,
            searchResults,
            model: model,
            provider: "perplexity",
          });
        } else {
          res.status(500).json({
            error: "No response from Perplexity API",
          });
        }
      }
    } catch (error) {
      console.error(
        "Perplexity API Error:",
        error.response?.data || error.message
      );

      if (stream) {
        res.write(
          `data: ${JSON.stringify({
            error: "Perplexity API request failed",
          })}\n\n`
        );
        res.end();
      } else {
        if (error.response?.status === 429) {
          res.status(429).json({
            error: "Perplexity API rate limit exceeded",
          });
        } else if (error.response?.status === 400) {
          res.status(400).json({
            error: "Invalid request to Perplexity API",
            details: error.response.data?.error?.message || "Bad request",
          });
        } else {
          res.status(500).json({
            error: "Perplexity API request failed",
            details: error.response?.data?.error?.message || error.message,
          });
        }
      }
    }
  }
);

// Available models endpoint
app.get("/api/models", (req, res) => {
  res.json({
    gemini: [
      {
        id: "gemini-1.5-flash",
        name: "Gemini 1.5 Flash",
        provider: "Google",
        description: "Fast and efficient for quick responses",
      },
      {
        id: "gemini-1.5-pro",
        name: "Gemini 1.5 Pro",
        provider: "Google",
        description: "Most capable model for complex tasks",
      },
    ],
    perplexity: [
      {
        id: "sonar",
        name: "Sonar",
        provider: "Perplexity",
        description: "Fast answers with reliable search results",
      },
    ],
  });
});

// Error handling middleware
app.use((error, req, res, next) => {
  console.error("Unhandled error:", error);
  res.status(500).json({
    error: "Internal server error",
    message: "Something went wrong on our end",
  });
});

// 404 handler
app.use((req, res) => {
  res.status(404).json({
    error: "Endpoint not found",
    availableEndpoints: [
      "POST /api/gemini",
      "POST /api/perplexity",
      "GET /api/models",
      "GET /health",
    ],
  });
});

// For Vercel serverless functions
module.exports = app;

// For local development
if (require.main === module) {
  const PORT = process.env.PORT || 3000;
  app.listen(PORT, () => {
    console.log(`Ghost Query Proxy Server running on port ${PORT}`);
    console.log(`Health check: http://localhost:${PORT}/health`);
    console.log(`Available endpoints:`);
    console.log(`  POST /api/gemini`);
    console.log(`  POST /api/perplexity`);
    console.log(`  GET /api/models`);
  });
}
