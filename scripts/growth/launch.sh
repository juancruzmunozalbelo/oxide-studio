#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
POSTS_DIR="$ROOT_DIR/docs/growth/posts"
PROMPTS_DIR="$ROOT_DIR/docs/growth/jules-prompts"
REPO="juancruzmunozalbelo/oxide-studio"

open_post() {
  local title="$1"
  local body_file="$2"
  local submit_url="$3"
  local label="$4"

  # Extract body without the first markdown H1 title line
  tail -n +3 "$body_file" | pbcopy

  echo "========================================================"
  echo "🚀 CANAL: $label"
  echo "========================================================"
  echo "📌 TÍTULO (copialo arriba):"
  echo "   $title"
  echo ""
  echo "✅ EL CUERPO DEL POST YA ESTÁ COPIADO EN TU PORTAPAPELES (Cmd+V)"
  echo "🌐 Abriendo $submit_url en tu Chrome..."
  open "$submit_url"
}

case "${1:-help}" in
  hn)
    open_post \
      "Show HN: Oxide Studio – Native Rust DAW with Ableton .als interop and 70 MCP tools" \
      "$POSTS_DIR/01_show_hn.md" \
      "https://news.ycombinator.com/submitlink?u=https%3A%2F%2Fgithub.com%2Fjuancruzmunozalbelo%2Foxide-studio&t=Show%20HN%3A%20Oxide%20Studio%20%E2%80%93%20Native%20Rust%20DAW%20with%20Ableton%20.als%20interop%20and%2070%20MCP%20tools" \
      "Hacker News (Show HN)"
    ;;
  rust)
    open_post \
      "I wrote a clean-room electronic music DAW in 100% Rust (cpal + egui, lock-free DSP, Ableton .als interop, and an MCP server)" \
      "$POSTS_DIR/02_r_rust.md" \
      "https://www.reddit.com/r/rust/submit?type=TEXT&title=I%20wrote%20a%20clean-room%20electronic%20music%20DAW%20in%20100%25%20Rust%20%28cpal%20%2B%20egui%2C%20lock-free%20DSP%2C%20Ableton%20.als%20interop%2C%20and%20an%20MCP%20server%29" \
      "r/rust"
    ;;
  ableton)
    open_post \
      "I built a free, open-source (MIT) lightweight DAW in Rust that natively imports and exports Ableton Live 12 (.als) projects" \
      "$POSTS_DIR/03_r_ableton.md" \
      "https://www.reddit.com/r/ableton/submit?type=TEXT&title=I%20built%20a%20free%2C%20open-source%20%28MIT%29%20lightweight%20DAW%20in%20Rust%20that%20natively%20imports%20and%20exports%20Ableton%20Live%2012%20%28.als%29%20projects" \
      "r/ableton"
    ;;
  localllama)
    open_post \
      "Instead of black-box MP3 generators, I built an open-source Rust DAW with a 76-tool MCP server so local LLMs can program real synths, 909 drums, and DSP live" \
      "$POSTS_DIR/04_r_localllama.md" \
      "https://www.reddit.com/r/LocalLLaMA/submit?type=TEXT&title=Instead%20of%20black-box%20MP3%20generators%2C%20I%20built%20an%20open-source%20Rust%20DAW%20with%20a%2076-tool%20MCP%20server%20so%20local%20LLMs%20can%20program%20real%20synths%2C%20909%20drums%2C%20and%20DSP%20live" \
      "r/LocalLLaMA"
    ;;
  techno)
    open_post \
      "Built a free open-source 909/303/FM groovebox & DAW in Rust with Euclidean generators, Roar saturation, and native Ableton .als export (MIT license)" \
      "$POSTS_DIR/05_r_technoproduction.md" \
      "https://www.reddit.com/r/TechnoProduction/submit?type=TEXT&title=Built%20a%20free%20open-source%20909%2F303%2FFM%20groovebox%20%26%20DAW%20in%20Rust%20with%20Euclidean%20generators%2C%20Roar%20saturation%2C%20and%20native%20Ableton%20.als%20export%20%28MIT%20license%29" \
      "r/TechnoProduction"
    ;;
  kvr)
    open_post \
      "Oxide Studio: Open-source (MIT) native Rust DAW with 4-op FM, 909, Euclidean sequencer & Ableton .als interop" \
      "$POSTS_DIR/06_kvr_elektronauts.md" \
      "https://www.kvraudio.com/forum/viewforum.php?f=7" \
      "KVR Audio (Hosts & Applications)"
    ;;
  jules)
    echo "⚡ Lanzando las 3 mejoras del repo en Jules..."
    jules new --repo "$REPO" "$(cat "$PROMPTS_DIR/jules_01_release_binaries.txt")"
    jules new --repo "$REPO" "$(cat "$PROMPTS_DIR/jules_02_readme_hn_conversion.txt")"
    jules new --repo "$REPO" "$(cat "$PROMPTS_DIR/jules_03_mcp_quickstart_examples.txt")"
    ;;
  *)
    echo "Uso: ./scripts/growth/launch.sh [canal]"
    echo ""
    echo "Canales listos (abren tu Chrome con el título pre-cargado y el cuerpo en Cmd+V):"
    echo "  ./scripts/growth/launch.sh rust        -> Día 1: r/rust"
    echo "  ./scripts/growth/launch.sh hn          -> Día 2: Show HN (Hacker News)"
    echo "  ./scripts/growth/launch.sh localllama  -> Día 4: r/LocalLLaMA"
    echo "  ./scripts/growth/launch.sh ableton     -> Semana 2: r/ableton"
    echo "  ./scripts/growth/launch.sh techno      -> Semana 2: r/TechnoProduction"
    echo "  ./scripts/growth/launch.sh kvr         -> Semana 3: KVR Audio"
    echo "  ./scripts/growth/launch.sh jules       -> Enviar las 3 tareas de código a Jules"
    ;;
esac
