class VoiceAgentClient {
    constructor(url, type = 'webrtc') {
        this.url = url;
        this.type = type;
        this.ws = null;
        this.pc = null;
        this.onEvent = null;
        this.onClose = null;
    }

    async connect() {
        return new Promise((resolve, reject) => {
            this.ws = new WebSocket(this.url);
            this.ws.onopen = () => resolve();
            this.ws.onerror = (e) => reject(e);
            this.ws.onclose = () => {
                if (this.onClose) this.onClose();
            };
            this.ws.onmessage = (event) => this.handleMessage(event);
        });
    }

    async handleMessage(event) {
        const msg = JSON.parse(event.data);
        if (this.onEvent) this.onEvent(msg);

        if (this.type === 'webrtc' && msg.event === 'answer') {
            await this.pc.setRemoteDescription(new RTCSessionDescription({
                type: 'answer',
                sdp: msg.sdp
            }));
        }
    }

    async startWebrtc() {
        this.pc = new RTCPeerConnection({
            iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
        });

        this.pc.ontrack = (event) => {
            const audio = new Audio();
            audio.srcObject = event.streams[0];
            audio.play();
        };

        try {
            const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
            stream.getTracks().forEach(track => this.pc.addTrack(track, stream));
        } catch (e) {
            console.error('Microphone access denied', e);
            throw e;
        }

        const offer = await this.pc.createOffer();
        await this.pc.setLocalDescription(offer);

        this.send({
            command: 'invite',
            option: {
                offer: offer.sdp,
                enable_ipv6: false
            }
        });
    }

    send(command) {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(command));
        }
    }

    sendTts(text, speaker = 'default') {
        this.send({
            command: 'tts',
            text: text,
            speaker: speaker
        });
    }

    sendInterrupt() {
        this.send({
            command: 'interrupt',
            graceful: true
        });
    }

    close() {
        if (this.ws) this.ws.close();
        if (this.pc) this.pc.close();
    }
}

function app() {
    return {
        currentTab: 'dashboard',
        wsConnected: false,
        callStatus: 'idle',
        debugConfig: {
            systemPrompt: 'You are a helpful assistant.',
            voiceId: 'default',
            temperature: 0.7,
            apiKey: '',
            llmEndpoint: 'https://api.openai.com/v1/chat/completions',
            llmModel: 'gpt-3.5-turbo',
            asrEndpoint: '',
            ttsEndpoint: ''
        },
        chatHistory: [],
        metrics: {
            latency: 0,
            tps: 0
        },
        stats: {
            totalCalls: 0,
            successRate: 0,
            avgDuration: '0s'
        },
        playbooks: [],
        selectedPlaybook: null,
        selectedPlaybookName: '',
        records: [],
        editor: null,
        client: null,

        navItems: [
            { id: 'dashboard', label: 'Dashboard', icon: '<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z"></path></svg>' },
            { id: 'debugger', label: 'Debugger', icon: '<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"></path></svg>' },
            { id: 'playbooks', label: 'Playbooks', icon: '<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"></path></svg>' },
            { id: 'records', label: 'Call Records', icon: '<svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01"></path></svg>' }
        ],

        init() {
            this.initMonaco();
            this.refreshData();

            // Check WS connection periodically
            setInterval(() => {
                // In a real app, we might ping the health endpoint
                fetch('/health').then(r => {
                    this.wsConnected = r.ok;
                }).catch(() => {
                    this.wsConnected = false;
                });
            }, 5000);
        },

        initMonaco() {
            require.config({ paths: { 'vs': 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.39.0/min/vs' } });
            require(['vs/editor/editor.main'], () => {
                this.editor = monaco.editor.create(document.getElementById('monaco-editor'), {
                    value: '',
                    language: 'markdown',
                    theme: 'vs-light',
                    automaticLayout: true,
                    minimap: { enabled: false }
                });
            });
        },

        async refreshData() {
            await this.fetchPlaybooks();
            await this.fetchRecords();
            // Mock stats for now
            this.stats = {
                totalCalls: this.records.length,
                successRate: 98,
                avgDuration: '45s'
            };
        },

        async fetchPlaybooks() {
            try {
                const res = await fetch('/api/playbooks');
                if (res.ok) {
                    this.playbooks = await res.json();
                }
            } catch (e) {
                console.error('Failed to fetch playbooks', e);
            }
        },

        async fetchRecords() {
            try {
                const res = await fetch('/api/records');
                if (res.ok) {
                    this.records = await res.json();
                }
            } catch (e) {
                console.error('Failed to fetch records', e);
            }
        },

        async selectPlaybook(pb) {
            this.selectedPlaybook = pb;
            this.selectedPlaybookName = pb.name;
            try {
                const res = await fetch(`/api/playbooks/${pb.name}`);
                if (res.ok) {
                    const content = await res.text();
                    if (this.editor) {
                        this.editor.setValue(content);
                    }
                }
            } catch (e) {
                console.error('Failed to load playbook content', e);
            }
        },

        createNewPlaybook() {
            this.selectedPlaybook = null;
            this.selectedPlaybookName = 'new-playbook.md';
            const template = `\`\`\`json
{
    "system_prompt": "You are a helpful assistant.",
    "voice_id": "default",
    "temperature": 0.7,
    "llm_endpoint": "https://api.openai.com/v1/chat/completions",
    "llm_model": "gpt-3.5-turbo",
    "asr_endpoint": "",
    "tts_endpoint": ""
}
\`\`\`

# Test Script

User: Hello
AI: Hi there!
`;
            if (this.editor) {
                this.editor.setValue(template);
            }
        },

        async savePlaybook() {
            if (!this.selectedPlaybookName) return;
            const content = this.editor.getValue();
            try {
                const res = await fetch(`/api/playbooks/${this.selectedPlaybookName}`, {
                    method: 'POST',
                    body: content
                });
                if (res.ok) {
                    alert('Saved successfully');
                    this.fetchPlaybooks();
                } else {
                    alert('Failed to save');
                }
            } catch (e) {
                console.error('Save failed', e);
                alert('Error saving playbook');
            }
        },

        async startDebugCall() {
            if (!this.debugConfig.apiKey) {
                alert('Please enter OpenAI API Key');
                return;
            }

            this.callStatus = 'active';
            this.chatHistory = [];
            this.chatHistory.push({ id: 1, role: 'system', content: 'Connecting to WebRTC...', timestamp: new Date().toLocaleTimeString() });

            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/call/webrtc`;

            this.client = new VoiceAgentClient(wsUrl, 'webrtc');

            this.client.onEvent = async (msg) => {
                if (msg.event === 'answer') {
                    this.chatHistory.push({ id: Date.now(), role: 'system', content: 'Call established. Speak now.', timestamp: new Date().toLocaleTimeString() });
                } else if (msg.event === 'asr_final') {
                    const userText = msg.text;
                    this.chatHistory.push({ id: Date.now(), role: 'user', content: userText, timestamp: new Date().toLocaleTimeString() });
                    await this.processLLM(userText);
                }
            };

            this.client.onClose = () => {
                this.endDebugCall();
            };

            try {
                await this.client.connect();
                await this.client.startWebrtc();
            } catch (e) {
                console.error('Connection failed', e);
                this.chatHistory.push({ id: Date.now(), role: 'system', content: 'Connection failed: ' + e.message, timestamp: new Date().toLocaleTimeString() });
                this.callStatus = 'idle';
            }
        },

        async processLLM(userText) {
            const start = Date.now();
            try {
                const response = await fetch(this.debugConfig.llmEndpoint, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                        'Authorization': `Bearer ${this.debugConfig.apiKey}`
                    },
                    body: JSON.stringify({
                        model: this.debugConfig.llmModel,
                        messages: [
                            { role: 'system', content: this.debugConfig.systemPrompt },
                            { role: 'user', content: userText }
                        ],
                        temperature: parseFloat(this.debugConfig.temperature)
                    })
                });

                const data = await response.json();
                const aiText = data.choices[0].message.content;

                this.metrics.latency = Date.now() - start;
                this.chatHistory.push({ id: Date.now(), role: 'ai', content: aiText, timestamp: new Date().toLocaleTimeString() });

                this.client.sendTts(aiText, this.debugConfig.voiceId);

            } catch (e) {
                console.error('LLM Error', e);
                this.chatHistory.push({ id: Date.now(), role: 'system', content: 'LLM Error: ' + e.message, timestamp: new Date().toLocaleTimeString() });
            }
        },

        endDebugCall() {
            if (this.client) {
                this.client.close();
                this.client = null;
            }
            this.callStatus = 'idle';
            this.chatHistory.push({ id: Date.now(), role: 'system', content: 'Call ended.', timestamp: new Date().toLocaleTimeString() });
        },

        saveAsPlaybook() {
            this.currentTab = 'playbooks';
            this.createNewPlaybook();
            const config = {
                system_prompt: this.debugConfig.systemPrompt,
                voice_id: this.debugConfig.voiceId,
                temperature: this.debugConfig.temperature,
                llm_endpoint: this.debugConfig.llmEndpoint,
                llm_model: this.debugConfig.llmModel,
                asr_endpoint: this.debugConfig.asrEndpoint,
                tts_endpoint: this.debugConfig.ttsEndpoint
            };

            const json = JSON.stringify(config, null, 4);
            const content = `\`\`\`json
${json}
\`\`\`

# Generated from Debug Session
`;
            this.editor.setValue(content);
        },

        async runBatch(playbookName) {
            if (!confirm(`Run batch test for ${playbookName}?`)) return;

            alert(`Batch run started for ${playbookName}. This will simulate a call.`);

            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/call/sip`;

            // Parse playbook content to get config
            // In a real app, we'd parse the JSON block from the editor or fetch it

            const client = new VoiceAgentClient(wsUrl, 'sip');

            client.onEvent = (msg) => {
                console.log('Batch Event:', msg);
                if (msg.event === 'asr_final') {
                    const userText = msg.text;
                    console.log('Batch User said:', userText);
                    client.sendTts("This is an automated batch test response.");
                }
            };

            await client.connect();

            // Simulate dialing
            client.send({
                command: 'invite',
                option: {
                    sip: {
                        callee: '1000',
                        caller: 'test-agent'
                    }
                }
            });

            setTimeout(() => {
                client.close();
                console.log('Batch: Call ended');
            }, 10000);
        }

            for(let i = 0; i < 3; i++) {
        // Simulate 3 calls
        console.log(`Initiating call ${i + 1}...`);
        // const ws = new WebSocket(...)
    }
}
    }
}
