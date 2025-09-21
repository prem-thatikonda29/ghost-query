# Ghost Query Proxy Server

A Node.js/Express proxy server for the Ghost Query AI application, providing secure access to Gemini and Perplexity APIs with intelligent rate limiting.

## Features

- 🔒 **Secure API Key Management** - API keys are kept server-side
- ⚡ **Smart Rate Limiting** - Different limits for different APIs
- 🌐 **Open Access** - No authentication required for easy distribution
- 📊 **Comprehensive Error Handling** - Detailed error responses
- 🚀 **Vercel Optimized** - Ready for serverless deployment
- 🔍 **Health Monitoring** - Built-in health check endpoint

## Rate Limits

- **General API**: 100 requests per 15 minutes per IP
- **Gemini API**: 50 requests per 15 minutes per IP
- **Perplexity API**: 30 requests per 15 minutes per IP

## API Endpoints

### Health Check

```
GET /health
```

### Available Models

```
GET /api/models
```

### Gemini API

```
POST /api/gemini
Content-Type: application/json

{
  "model": "gemini-1.5-flash",
  "prompt": "Your question here",
  "temperature": 0.7,
  "maxTokens": 2048
}
```

### Perplexity API

```
POST /api/perplexity
Content-Type: application/json

{
  "model": "sonar",
  "prompt": "Your question here"
}
```

## Local Development

1. **Install dependencies:**

   ```bash
   npm install
   ```

2. **Set up environment variables:**

   ```bash
   cp env.example .env
   # Edit .env with your actual API keys
   ```

3. **Run locally:**
   ```bash
   npm run dev
   ```

## Vercel Deployment

1. **Install Vercel CLI:**

   ```bash
   npm i -g vercel
   ```

2. **Deploy:**

   ```bash
   vercel
   ```

3. **Set environment variables in Vercel dashboard:**

   - `GEMINI_API_KEY`
   - `PERPLEXITY_API_KEY`

4. **Your proxy server will be available at:**
   ```
   https://your-project.vercel.app
   ```

## Environment Variables

| Variable             | Description           | Required |
| -------------------- | --------------------- | -------- |
| `GEMINI_API_KEY`     | Google Gemini API key | Yes      |
| `PERPLEXITY_API_KEY` | Perplexity API key    | Yes      |

## Security Features

- **Helmet.js** for security headers
- **CORS** enabled for cross-origin requests
- **Rate limiting** to prevent abuse
- **Input validation** on all endpoints
- **Error sanitization** to prevent information leakage

## Response Format

### Success Response

```json
{
  "success": true,
  "content": "AI response here",
  "model": "gemini-1.5-flash",
  "provider": "gemini"
}
```

### Error Response

```json
{
  "error": "Error message",
  "details": "Additional error details"
}
```

## Monitoring

The server includes comprehensive logging and error handling. Monitor your Vercel function logs for:

- API usage patterns
- Rate limit hits
- Error rates
- Performance metrics

## Cost Optimization

- Smart rate limiting reduces API costs
- 30-second timeouts prevent hanging requests
- Efficient error handling reduces unnecessary retries
- Vercel's serverless architecture scales automatically
