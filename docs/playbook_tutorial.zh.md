# Playbook 配置与使用完全指南

Playbook 是 RustPBX 的核心配置文件，它采用 Markdown 格式，通过 Front Matter (YAML) 定义 AI 引擎参数，通过 Markdown 内容定义业务流程、场景和 AI 提示词（Prompt）。

## 1. 基本结构

Playbook 文件由三部分组成：
1.  **Front Matter (`---`)**: YAML 格式的全局配置，包括 ASR/TTS/LLM 引擎参数。
2.  **全局提示词 (Global Prompt)**: 定义 AI 的角色身份、行为准则和工具使用规则。
3.  **场景 (`# Scene: ...`)**: 定义不同的对话阶段及其特定的提示词、按键处理和流程跳转。

---

## 2. 全局配置 (Front Matter)

### 2.1 基础引擎配置
```yaml
asr:
  provider: "aliyun" # 或 "openai", "tencent", "deepgram"
  language: "zh-CN"
tts:
  provider: "aliyun"
  model: "sambert-zhichu-v1"
  speed: 1.0
  volume: 50
llm:
  provider: "openai"
  model: "gpt-4o"
  apiKey: "${OPENAI_API_KEY}" # 支持环境变量
  baseUrl: "https://api.openai.com/v1"
  features: ["http_tool", "voice_emotion"] # 启用增强功能
```

### 2.2 交互行为配置
```yaml
greeting: "您好，我是您的 AI 助手，请问有什么可以帮您？"
denoise: true # 启用语音降噪
interruption:
  strategy: "both" # 打断策略: "none", "vad", "asr", "both"
  minSpeechMs: 500 # 用户说话超过 500ms 才触发打断
  fillerWordFilter: true # 自动过滤 "嗯"、"那个" 等语气词
followup:
  timeout: 10000 # 如果用户 10 秒没说话，AI 主动开启跟进
  max: 2 # 最多连续跟进 2 次
```

### 2.3 辅助功能配置
```yaml
ambiance:
  path: "./config/office.wav" # 通话背景音乐
  duckLevel: 0.1 # AI 说话时背景音自动降低到的音量系数 (0.1 = 10%)
  normalLevel: 0.5 # 默认背景音量
recorder:
  recorderFile: "recordings/call_{id}.wav" # 自动开启通话录音
```

---

## 3. 场景管理 (Scenes)

通过 `Scene: [ID]` 定义对话阶段，AI 会在切换场景时自动更新其 System Prompt。

```markdown
# Scene: start
## welcome
AI 角色：你现在是欢迎人员。

<dtmf digit="1" action="goto" scene="sale"/>
<dtmf digit="2" action="transfer" target="sip:operator@domain.com"/>

# Scene: sale
## product_info
AI 角色：你现在是销售顾问。请向用户介绍我们的理财产品。
```

---

## 4. 动作指令 (Commands)

Playbook 支持两种方式触发系统动作：

### 4.1 XML 简易指令 (推荐)
这类指令可以直接写在角色描述或直接被模型输出，支持流式处理：

-   **挂断**: `<hangup/>`
-   **转接**: `<refer to="sip:1001@127.0.0.1"/>`
-   **音效播放**: `<play file="config/media/ding.wav"/>`
-   **场景跳转**: `<goto scene="support"/>`

### 4.2 JSON 工具调用 (自定义推理)
用于复杂操作，如 HTTP 调用。结果会自动喂回给 AI 进行下一次推理。

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

## 5. 进阶功能

### 5.1 Realtime 模式配置
如果模型支持 Realtime API (如 OpenAI gpt-4o-realtime)，可跳过序列化 pipeline 以获得超低延迟：

```yaml
realtime:
  provider: "openai"
  model: "gpt-4o-realtime-preview"
  voice: "alloy"
  turn_detection:
    type: "server_vad"
    threshold: 0.5
```

### 5.2 RAG (检索增强生成)
在 llm 配置中启用功能后，AI 可以调用内置的知识库检索逻辑。

### 5.3 Post-hook (通话结果上报)
通话挂断后，自动生成摘要并推送到业务系统：

```yaml
posthook:
  url: "https://your-crm.com/api/callback"
  summary: "detailed" # 摘要精细度: short, detailed, intent, json
  include_history: true
```

---

## 6. 最佳实践规则

1.  **短句原则**: 在提示词中要求 AI 使用短句，因为系统会按句子流式合成语音，句子越短响应越快。
2.  **打断保护**: 如果 AI 说话很关键，可以在 Front Matter 中设置 `interruption.strategy: "none"` 临时禁止打断。
3.  **转接兜底**: 在提供转接功能时，务必告知 AI 如果转接失败该如何安抚用户。
4.  **变量注入**: Playbook 支持 Minijinja 模板语法，你可以在启动呼叫时动态注入变量，例如 `{{ user_name }}`。
