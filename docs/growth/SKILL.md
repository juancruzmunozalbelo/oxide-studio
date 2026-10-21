---
name: oxide-growth
description: >
  Playbook de crecimiento orgánico, ingeniería de lanzamientos (Hacker News, Reddit, KVR, Elektronauts),
  orquestación de tareas de código vía Jules y monitoreo de tracción en vivo para Oxide Studio
  (https://github.com/juancruzmunozalbelo/oxide-studio).
---

# Oxide Studio — Growth & Launch Engineering (`oxide-growth`)

Protocolo operativo para posicionar **Oxide Studio** (DAW nativo en Rust con puente bidireccional `.als` de Ableton Live 12 y servidor MCP de 76 herramientas) en comunidades técnicas y de producción musical sin caer en spam ni marketing corporativo.

---

## 1. Hechos Técnicos Reales (Single Source of Truth — Prohibido Inventar)

* **Repositorio:** `https://github.com/juancruzmunozalbelo/oxide-studio`
* **Landing / Demo:** `https://juancruzmunozalbelo.github.io/oxide-studio/`
* **Licencia:** `MIT OR Apache-2.0` (100% libre, sin telemetría, sin cuentas).
* **Arquitectura:** Workspace de 11 crates en Rust (`ableton-core`, `ableton-dsp`, `ableton-drums`, `ableton-fx`, `ableton-audio`, `ableton-bounce`, `ableton-timeline`, `ableton-project`, `ableton-plugin-scanner`, `ableton-gui`, `ableton-mcp`).
* **Audio Real-Time:** Callback de `cpal` con cero alocaciones en el audio thread (`zero-allocation`). Comunicación entre UI/RPC y el motor de audio mediante colas SPSC lock-free (`rtrb` / `ringbuf`).
* **GUI:** 60 FPS en modo inmediato con `egui` / `eframe` sobre `wgpu` (Metal en macOS). Incluye Session Matrix, Arrangement Timeline, Piano Roll con `Scale-Fold`, generador euclidiano, Chord Stacks y XY Perform Pad de 8 macros con Orbit LFO.
* **Puente `.als` Bidireccional:** Lee y escribe archivos `.als` de Ableton Live 12 (gzip + XML) preservando pistas, clips MIDI, tempo y estructura de sesión/arreglo.
* **DSP Nativo:** Operator FM (4 operadores), Wavetable, sintetizador ácido 303, Drum Rack TR-909, saturador multibanda estilo Roar (con ADAA), OTT de 3 bandas, EQ Eight paramétrico, Auto Filter, Glue Compressor y separador de stems en 4 bandas (`Kick+Sub`, `Bass`, `Synths/Mids`, `Hats/Air`).
* **76 Herramientas MCP + Live RPC Bridge (`127.0.0.1:19384`):** Control determinístico en tiempo real desde Claude, Gemini, Qwen/Llama local o cualquier cliente MCP.

---

## 2. Reglas Anti-Ban y Anti-Cringe (Innegociables)

1. **Cero prosa de LLM corporativo:** Prohibido escribir *"revolutionizing music production"*, *"the future of AI music"*, *"seamless integration"*, *"delve"*, *"tapestry"*, *"not just X, it's Y"*.
2. **Postura honesta de ingeniero/productor:** Siempre admitir qué está verde o en qué casos conviene usar Ableton Live completo (ej. sesiones de mezcla de 120 pistas con plugins de terceros o parches complejos de Max for Live).
3. **Adaptar el ángulo a la tribu:**
   * En **`r/rust`**: Importa el audio thread lock-free, las 11 crates, `cpal` y cómo domesticar `egui` a 60 FPS sin spikes de frame-time.
   * En **Hacker News (`Show HN`)**: Importa la ingeniería inversa limpia de `.als` (gzip+XML) y que el servidor MCP manipula síntesis y DSP simbólico en vez de escupir MP3 caja negra estilo Suno.
   * En **`r/ableton` / `r/edmproduction` / `r/technoproduction`**: **No poner "AI" en el título.** Entrar por el bloc de notas liviano que abre en 0.5s, importa/exporta `.als`, separa stems en 4 bandas gratis y trae generador euclidiano + 909 + Roar nativo.
   * En **`r/LocalLLaMA` / `r/ClaudeAI`**: Entrar por el servidor MCP de 76 tools moviendo sintetizadores FM/303/909 en vivo por TCP (`127.0.0.1:19384`).

---

## 3. Archivos y Herramientas de esta Skill

* `~/.hermes/skills/oxide-growth/campaign_queue.json`: Cola estructurada de las 4 semanas con los posts listos para publicar, horarios óptimos y estado de cada canal.
* `~/.hermes/skills/oxide-growth/jules-prompts/`: Specs técnicas autocontenidas listas para delegar a **Jules** (`~/.hermes/scripts/oxide_jules_dispatch.sh`) para mejorar conversión de visitas a Stars.
* `~/.hermes/scripts/oxide_growth_monitor.py`: Monitor determinístico que consulta GitHub API (Stars, Forks, Issues, PRs), Hacker News Algolia API y Reddit JSON API para detectar tracción o comentarios nuevos y avisar a Jota por WhatsApp.
