# Playbook Configuration & Usage Guide

Playbook is the core configuration system of RustPBX. It uses the Markdown format, where the Front Matter (YAML) defines AI engine parameters, and the Markdown content defines the business flow, scenes, and AI prompts.

## 1. Basic Structure

A Playbook file consists of three parts:
1.  **Front Matter (`---`)**: YAML format for global configurations, including ASR, TTS, and LLM engine parameters.
2.  **Global Prompt**: Defines the AI's persona, behavior guidelines, and tool usage rules.
3.  **Scenes (`# Scene: ...`)**: Defines different stages of the conversation with their own specific prompts, DTMF handling, and workflow transitions.

---

## 2. Global Configuration (Front Matter)

### 2.1 Engine Configuration
```yaml
asr:
  provider: "openai" # Options: "openai", "aliyun", "tencent", "deepgram", "sensevoice"
  language: "en-US"
  # extra parameter for passing specific engine configurations
  extra:
    silence_threshold: "0.05" # Only for sensevoice: silence threshold (default 0.01), increase to reduce noise triggers
tts:
  provider: "supertonic" # Default: "supertonic" for English (en), "msedge" for Chinese (zh)
  model: "M1"
  speed: 1.0
  volume: 50
llm:
  provider: "openai"
  model: "gpt-4o"
  apiKey: "OPENAI_API_KEY"
  #baseUrl: "https://api.openai.com/v1"
  language: "en" # Default: "zh". Used for loading language-specific tool instructions and features
  features: ["http_tool", "voice_emotion"] # Enable enhanced capabilities
  # toolInstructions: "Custom tool instructions..." # Optional: Override default tool usage instructions
```

### 2.2 Interaction Behavior
```yaml
greeting: "Hello, I am your AI assistant. How can I help you today?"
denoise: true # Enable noise reduction
interruption:
  strategy: "both" # Strategies: "none", "vad", "asr", "both"
  minSpeechMs: 500 # User must speak for at least 500ms to trigger interruption
  fillerWordFilter: true # Automatically filter fillers like "um", "ah", "uh"
followup:
  timeout: 10000 # AI proactively speaks if user is silent for 10 seconds
  max: 2 # Maximum number of consecutive follow-ups
```

### 2.3 Add-on Features
```yaml
ambiance:
  path: "./config/office.wav" # Background music for the call
  duckLevel: 0.1 # Volume reduction factor for background music when AI speaks (0.1 = 10%)
  normalLevel: 0.5 # Default background volume
recorder:
  recorderFile: "recordings/call_{id}.wav" # Automatically record the call
```

---

## 3. Scene Management

Define conversation stages via `# Scene: [ID]`. The AI will automatically update its System Prompt when switching scenes.

```markdown
# Scene: start
## welcome
AI Role: You are a receptionist welcoming the user.

<dtmf digit="1" action="goto" scene="sales"/>
<dtmf digit="2" action="transfer" target="sip:support@domain.com"/>

# Scene: sales
## product_info
AI Role: You are a sales consultant. Introduce our products to the user.
```

---

## 4. Action Commands

The Playbook supports two ways to trigger system actions:

### 4.1 XML Commands (Recommended)
Simple tags that can be output by the model during streaming for real-time execution:

-   **Hang up**: `<hangup/>`
-   **Transfer**: `<refer to="sip:1001@127.0.0.1"/>`
-   **Play audio file**: `<play file="config/media/ding.wav"/>`
-   **Switch scene**: `<goto scene="support"/>`

### 4.2 JSON Tool Calling
Used for complex operations like HTTP calls. Results are fed back to the AI for a follow-up response.

```json
{
  "tools": [
    {
      "name": "http",
      "url": "https://api.example.com/query",
      "method": "POST",
      "body": { "id": "123" }
    }
  ]
}
```

---

## 5. Advanced Features

### 5.1 Realtime Mode
If using a model that supports the Realtime API (e.g., OpenAI gpt-4o-realtime), you can enable ultra-low latency mode:

```yaml
realtime:
  provider: "openai"
  model: "gpt-4o-realtime-preview"
  voice: "alloy"
  turn_detection:
    type: "server_vad"
    threshold: 0.5
```

### 5.2 RAG (Retrieval-Augmented Generation)
When enabled in the LLM config, the AI can call built-in knowledge base retrieval logic.

### 5.3 Customizing Tool Instructions
By default, the system includes tool usage instructions in the prompt based on the `language` setting (e.g., English for "en", Chinese for "zh"). These instructions tell the LLM how to use commands like `<hangup/>`, `<refer/>`, etc.

**Method 1: Use Language-Specific Defaults**
Set the `language` field in the LLM config:
```yaml
llm:
  language: "en" # Will use features/tool_instructions.en.md
```

**Method 2: Provide Custom Instructions**
Override the default tool instructions entirely:
```yaml
llm:
  toolInstructions: |
    Custom instructions for your specific use case.
    You can define your own tool usage format here.
```

**Method 3: Modify Feature Files**
Edit the files directly:
- `features/tool_instructions.en.md` for English
- `features/tool_instructions.zh.md` for Chinese
- Add your own language: `features/tool_instructions.ja.md` for Japanese

This allows you to:
- Translate tool instructions to any language
- Add domain-specific guidance
- Customize the format and style of instructions

### 5.4 Post-hook (Reporting)
Automatically generate a summary and push it to your business system after the call ends:

```yaml
posthook:
  url: "https://your-crm.com/api/callback"
  summary: "detailed" # Types: short, detailed, intent, json
  include_history: true
```

---

## 6. Best Practices

1.  **Short Sentences**: Instruct the AI to use short sentences. The system synthesizes audio per sentence; shorter sentences lead to faster responses.
2.  **Interruption Protection**: If the AI's speech is critical, set `interruption.strategy: "none"` temporarily in the Front Matter.
3.  **Transfer Fallback**: When offering transfers, always instruct the AI on how to handle failed transfers politely.
4.  **Variable Injection**: Playbooks support Minijinja templates. You can inject dynamic variables مانند `{{ user_name }}` when starting a call.
