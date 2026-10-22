# Security Policy

## Supported Versions

We actively maintain and provide security patches for the following versions of **Oxide Studio**:

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | :white_check_mark: |
| < 1.0.0 | :x:                |

---

## Reporting a Vulnerability

If you discover a security vulnerability within Oxide Studio, please report it privately rather than opening a public issue on GitHub.

- **Email**: Send vulnerability reports directly to **juancruzmunozalbelo@gmail.com**.
- **Subject**: `[SECURITY VULNERABILITY] Oxide Studio: <Short Description>`
- **Include**:
  - A description of the vulnerability and affected crate(s) / subsystem(s).
  - Step-by-step instructions or proof-of-concept code to reproduce the issue.
  - Expected impact, potential attack vectors, and any proposed fixes.

We will acknowledge receipt within 48 hours and coordinate a coordinated disclosure timeline and fix release.

---

## Threat Model & Security Architecture

Oxide Studio operates as a native desktop DAW, a low-latency audio engine, and a Model Context Protocol (MCP) server. Our threat model focuses on two key boundaries:

### 1. Out-of-Process Plugin Sandbox Isolation (`ableton-plugin-scanner`)

- **Untrusted Code**: Audio plugins (AudioUnit v2, VST3, CLAP) are third-party native dynamic libraries (`.component`, `.vst3`, `.clap`) executing with host user privileges. Third-party plugins frequently contain memory-safety defects, invalid memory accesses, or unhandled exceptions.
- **Isolation Supervisor**: Plugin discovery and scanning run in an isolated subprocess (`ableton-plugin-scanner`). If an untrusted plugin segfaults, aborts, or corrupts memory during discovery, only the ephemeral worker process terminates. The main Oxide Studio workstation host remains unharmed and records the failing plugin in a crash-quarantine manifest.
- **Hosted Instance Boundaries**: Hosted plugins instantiated on track device chains run with input buffer bounding and floating-point denormal sanitization (`flush-to-zero` / `denormals-are-zero`).

### 2. Local Loopback RPC & Network Security

- **Loopback Binding**: The live Oxide Studio GUI JSON-RPC server binds strictly to the local loopback interface (`127.0.0.1:9877`). It never binds to `0.0.0.0` or listens on public/external network interfaces.
- **MCP stdio Communication**: The primary Model Context Protocol interface operates over standard input/output (`stdio`) JSON-RPC 2.0 under the execution context of the invoking agent, preventing remote unauthenticated network exploitation.
- **Input Validation**: All incoming JSON-RPC payloads undergo schema validation and bounds-checking before mutating DAW state or triggering audio commands.
