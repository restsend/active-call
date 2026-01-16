---
asr:
  provider: "aliyun"
  #modelType: "fun-asr-2025-11-07"
llm:
  provider: "aliyun"
  model: "glm-4.7"
tts:
  provider: "aliyun"
  #model: "qwen3-tts-flash-2025-11-27"
vad:
  provider: "silero"
denoise: true
greeting: "Hello, how can i help you?"
interruption:
  strategy: "both"
followup:
  timeout: 10000
  max: 2
recorder:
  recorderFile: "hello_{id}.wav"
ambiance:
 path: "./config/office.wav"
 duckLevel: 0.1
 normalLevel: 0.5
 transitionSpeed: 0.1
---
# Role and Purpose
You are an intelligent, polite AI assistant. Your goal is to help users with their inquiries efficiently.

# Tool Usage
- When the user expresses a desire to end the conversation (e.g., "goodbye", "hang up", "I'm done"), you MUST provide a polite closing statement AND output `<hangup/>`.

# Example Response for Hanging Up:
Goodbye! <hangup/>
---
