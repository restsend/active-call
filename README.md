# Active Call

`active-call` is a standalone Rust crate designed for building AI Voice Agents. It was originally decoupled from [rustpbx](https://github.com/restsend/rustpbx) to provide a dedicated, high-performance library for voice agent integration.

## Overview

This crate handles the complex low-level details of voice communication, making it easy to connect LLM/NLP engines with telephony platforms. It manages:

- **ASR (Automatic Speech Recognition)**: Handling speech-to-text transitions and events.
- **TTS (Text-to-Speech)**: Managing real-time audio synthesis and playback.
- **SIP & WebRTC**: Bridging traditional telephony and modern web communication protocols.
- **Voice Details**: Managing audio buffers, codecs (like Opus), and real-time streaming.

By abstracting these technical hurdles, `active-call` allows developers to focus on building intelligent dialogue systems rather than worrying about the underlying voice infrastructure.

## Key Features

- **Standalone Crate**: Decoupled from the main PBX logic for better modularity.
- **LLM/NLP Friendly**: Designed to easily feed ASR results into LLMs and stream TTS responses back to the caller.
- **Multi-Protocol Support**: Supports SIP, WebRTC, and raw WebSocket audio streams.
- **Real-time Performance**: Built with Rust for low-latency audio processing.

## API Documentation

For detailed information on REST endpoints and WebSocket protocols, please refer to the [API Documentation](docs/api.md).

## License

This project is licensed under the MIT License.
