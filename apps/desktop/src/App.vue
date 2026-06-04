<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  findActiveLyricLine,
  parseLrc,
  type BridgeMessage,
  type ParsedLrc,
  type PlayerState
} from "@lyricbridge/shared";
import { computed, onMounted, ref, watch } from "vue";

type BridgeServerStatus = {
  address: string;
  running: boolean;
  error: string | null;
};

type LyricBinding = {
  videoId: string;
  lyricFilePath: string;
  offsetMs: number;
};

type LyricBindingWithContent = LyricBinding & {
  lyricText: string;
};

const serverStatus = ref<BridgeServerStatus>({
  address: "127.0.0.1:32190",
  running: false,
  error: null
});
const latestState = ref<PlayerState | null>(null);
const latestError = ref<string | null>(null);
const bindingError = ref<string | null>(null);
const activeBinding = ref<LyricBindingWithContent | null>(null);
const loadedVideoId = ref<string | null>(null);
const lyricFilePath = ref("");
const offsetMs = ref(0);
const saving = ref(false);
const isLyricsWindow = new URLSearchParams(window.location.search).get("window") === "lyrics";

if (isLyricsWindow) {
  document.documentElement.classList.add("lyrics-root");
}

const parsedLrc = computed<ParsedLrc | null>(() => {
  if (!activeBinding.value) {
    return null;
  }

  return parseLrc(activeBinding.value.lyricText);
});

const videoLabel = computed(() => {
  if (!latestState.value) {
    return "Waiting for YouTube playback";
  }

  return latestState.value.title || latestState.value.videoId || "Untitled video";
});

const progressLabel = computed(() => {
  if (!latestState.value) {
    return "--:-- / --:--";
  }

  return `${formatTime(latestState.value.currentTime)} / ${formatTime(
    latestState.value.duration
  )}`;
});

const syncedTime = computed(() => {
  if (!latestState.value) {
    return 0;
  }

  return latestState.value.currentTime + offsetMs.value / 1000;
});

const activeLyricIndex = computed(() => {
  if (!parsedLrc.value) {
    return -1;
  }

  return findActiveLyricLine(parsedLrc.value.lines, syncedTime.value);
});

const currentLyric = computed(() => {
  if (!parsedLrc.value || activeLyricIndex.value < 0) {
    return null;
  }

  return parsedLrc.value.lines[activeLyricIndex.value] ?? null;
});

const nextLyric = computed(() => {
  if (!parsedLrc.value || activeLyricIndex.value < 0) {
    return null;
  }

  return parsedLrc.value.lines[activeLyricIndex.value + 1] ?? null;
});

const lyricProgress = computed(() => {
  if (!parsedLrc.value || activeLyricIndex.value < 0) {
    return 0;
  }

  const line = parsedLrc.value.lines[activeLyricIndex.value];
  const nextLine = parsedLrc.value.lines[activeLyricIndex.value + 1];
  if (!line || !nextLine) {
    return 1;
  }

  const span = nextLine.time - line.time;
  if (span <= 0) {
    return 1;
  }

  return Math.min(1, Math.max(0, (syncedTime.value - line.time) / span));
});

const canSaveBinding = computed(() => Boolean(latestState.value?.videoId && lyricFilePath.value.trim()));

onMounted(async () => {
  await listen<BridgeServerStatus>("bridge-server-status", (event) => {
    serverStatus.value = event.payload;
  });

  serverStatus.value = await invoke<BridgeServerStatus>("get_bridge_server_status");

  await listen<BridgeMessage>("bridge-message", (event) => {
    if (event.payload.type === "player-state") {
      latestState.value = event.payload.payload;
      latestError.value = null;
    }
  });

  await listen<string>("bridge-message-error", (event) => {
    latestError.value = event.payload;
  });
});

watch(
  () => latestState.value?.videoId ?? null,
  async (videoId) => {
    if (!videoId || videoId === loadedVideoId.value) {
      return;
    }

    loadedVideoId.value = videoId;
    await loadBinding(videoId);
  }
);

async function loadBinding(videoId: string) {
  bindingError.value = null;
  activeBinding.value = null;
  lyricFilePath.value = "";
  offsetMs.value = 0;

  try {
    const binding = await invoke<LyricBindingWithContent | null>("get_lyric_binding", {
      videoId
    });

    if (!binding) {
      return;
    }

    applyBinding(binding);
  } catch (error) {
    bindingError.value = String(error);
  }
}

async function saveBinding() {
  const videoId = latestState.value?.videoId;
  if (!videoId) {
    bindingError.value = "Open a YouTube video before saving a lyric binding.";
    return;
  }

  saving.value = true;
  bindingError.value = null;

  try {
    const binding = await invoke<LyricBindingWithContent>("save_lyric_binding", {
      binding: {
        videoId,
        lyricFilePath: lyricFilePath.value.trim(),
        offsetMs: Number(offsetMs.value) || 0
      }
    });

    applyBinding(binding);
  } catch (error) {
    bindingError.value = String(error);
  } finally {
    saving.value = false;
  }
}

async function chooseLyricFile() {
  const selected = await open({
    multiple: false,
    filters: [
      {
        name: "LRC lyrics",
        extensions: ["lrc"]
      },
      {
        name: "Text files",
        extensions: ["txt"]
      }
    ]
  });

  if (typeof selected === "string") {
    lyricFilePath.value = selected;
  }
}

function applyBinding(binding: LyricBindingWithContent) {
  activeBinding.value = binding;
  lyricFilePath.value = binding.lyricFilePath;
  offsetMs.value = binding.offsetMs;
}

function formatTime(value: number | null): string {
  if (value === null || !Number.isFinite(value)) {
    return "--:--";
  }

  const totalSeconds = Math.max(0, Math.floor(value));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
</script>

<template>
  <main v-if="isLyricsWindow" class="lyrics-window">
    <section class="floating-lyrics" :style="{ '--line-progress': `${lyricProgress * 100}%` }">
      <p class="floating-current" :data-text="currentLyric?.text || 'LyricBridge'">
        {{ currentLyric?.text || "LyricBridge" }}
      </p>
      <p class="floating-next">{{ nextLyric?.text || "Waiting for bound YouTube lyrics" }}</p>
    </section>
  </main>

  <main v-else class="shell">
    <section class="panel">
      <div class="status-row">
        <span class="status-dot" :class="{ active: serverStatus.running }" />
        <span>
          {{ serverStatus.running ? "Bridge listening" : "Bridge offline" }}
          <small>{{ serverStatus.address }}</small>
        </span>
      </div>

      <header>
        <h1>LyricBridge</h1>
        <p>YouTube playback bridge for desktop lyrics.</p>
      </header>

      <section class="now-playing">
        <span class="eyebrow">Now playing</span>
        <strong>{{ videoLabel }}</strong>
        <span>{{ progressLabel }}</span>
      </section>

      <dl class="metrics">
        <div>
          <dt>Video ID</dt>
          <dd>{{ latestState?.videoId || "Not detected" }}</dd>
        </div>
        <div>
          <dt>Status</dt>
          <dd>{{ latestState ? (latestState.paused ? "Paused" : "Playing") : "Idle" }}</dd>
        </div>
        <div>
          <dt>Rate</dt>
          <dd>{{ latestState?.playbackRate ?? 1 }}x</dd>
        </div>
      </dl>

      <section class="lyrics">
        <span class="eyebrow">Lyrics</span>
        <p class="current-line">{{ currentLyric?.text || "No lyric line active" }}</p>
        <p class="next-line">{{ nextLyric?.text || "Bind an LRC file for this video" }}</p>
      </section>

      <form class="binding-form" @submit.prevent="saveBinding">
        <label>
          <span>LRC file path</span>
          <div class="file-input-row">
            <input
              v-model="lyricFilePath"
              placeholder="F:\Lyrics\song.lrc"
              spellcheck="false"
            />
            <button class="secondary-button" type="button" @click="chooseLyricFile">Browse</button>
          </div>
        </label>

        <label>
          <span>Offset ms</span>
          <input v-model.number="offsetMs" type="number" step="1" />
        </label>

        <button type="submit" :disabled="!canSaveBinding || saving">
          {{ saving ? "Saving" : "Save binding" }}
        </button>
      </form>

      <p v-if="activeBinding" class="hint">
        {{ parsedLrc?.lines.length ?? 0 }} lines loaded from {{ activeBinding.lyricFilePath }}
      </p>
      <p v-if="serverStatus.error" class="error">{{ serverStatus.error }}</p>
      <p v-if="latestError" class="error">{{ latestError }}</p>
      <p v-if="bindingError" class="error">{{ bindingError }}</p>
    </section>
  </main>
</template>

<style>
:root {
  color: #1f2933;
  background: #eef1f5;
  font-family:
    Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI",
    sans-serif;
  font-size: 16px;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

body {
  margin: 0;
}

html.lyrics-root,
html.lyrics-root body,
html.lyrics-root #app {
  background: transparent;
}

button,
input {
  font: inherit;
}

.shell {
  min-height: 100vh;
  display: grid;
  place-items: center;
  padding: 32px;
  box-sizing: border-box;
}

.panel {
  width: min(780px, 100%);
  background: #ffffff;
  border: 1px solid #d9e0e8;
  border-radius: 8px;
  padding: 28px;
  box-shadow: 0 18px 50px rgba(23, 31, 42, 0.12);
}

.status-row {
  display: flex;
  align-items: center;
  gap: 10px;
  color: #52606d;
  font-size: 14px;
}

.status-row small {
  display: block;
  color: #7b8794;
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: #d64545;
}

.status-dot.active {
  background: #2f9e44;
}

header {
  margin: 24px 0 28px;
}

h1 {
  margin: 0 0 8px;
  font-size: 34px;
  line-height: 1.15;
}

header p {
  margin: 0;
  color: #52606d;
}

.now-playing,
.lyrics {
  display: grid;
  gap: 6px;
  padding: 18px;
  background: #f7f9fb;
  border: 1px solid #d9e0e8;
  border-radius: 8px;
}

.now-playing strong {
  font-size: 20px;
  line-height: 1.3;
}

.eyebrow {
  color: #66788a;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0;
  text-transform: uppercase;
}

.metrics {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
  margin: 18px 0;
}

dt {
  color: #66788a;
  font-size: 13px;
}

dd {
  margin: 4px 0 0;
  overflow-wrap: anywhere;
  font-weight: 650;
}

.lyrics {
  margin-bottom: 18px;
}

.current-line {
  min-height: 38px;
  margin: 0;
  color: #111827;
  font-size: 28px;
  font-weight: 800;
  line-height: 1.35;
}

.next-line {
  min-height: 24px;
  margin: 0;
  color: #66788a;
  font-size: 17px;
  line-height: 1.4;
}

.binding-form {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 130px 130px;
  align-items: end;
  gap: 12px;
}

label {
  display: grid;
  gap: 6px;
}

label span {
  color: #52606d;
  font-size: 13px;
  font-weight: 650;
}

input {
  min-width: 0;
  height: 38px;
  padding: 0 10px;
  color: #1f2933;
  background: #ffffff;
  border: 1px solid #cbd5df;
  border-radius: 6px;
  box-sizing: border-box;
}

button {
  height: 38px;
  color: #ffffff;
  background: #2563eb;
  border: 1px solid #1d4ed8;
  border-radius: 6px;
  cursor: pointer;
  font-weight: 700;
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.secondary-button {
  color: #1f2933;
  background: #ffffff;
  border-color: #cbd5df;
}

.file-input-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 88px;
  gap: 8px;
}

.hint,
.error {
  margin: 14px 0 0;
  overflow-wrap: anywhere;
  font-size: 14px;
}

.hint {
  color: #52606d;
}

.error {
  color: #b42318;
}

@media (max-width: 680px) {
  .metrics,
  .binding-form {
    grid-template-columns: 1fr;
  }
}

.lyrics-window {
  min-height: 100vh;
  display: grid;
  align-items: center;
  padding: 12px 28px;
  box-sizing: border-box;
  background: transparent;
  user-select: none;
}

.floating-lyrics {
  --line-progress: 0%;
  display: grid;
  gap: 6px;
  text-align: center;
}

.floating-current,
.floating-next {
  margin: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  text-shadow:
    0 2px 3px rgba(0, 0, 0, 0.85),
    0 0 2px rgba(0, 0, 0, 0.9),
    1px 0 0 rgba(0, 0, 0, 0.95),
    -1px 0 0 rgba(0, 0, 0, 0.95),
    0 1px 0 rgba(0, 0, 0, 0.95),
    0 -1px 0 rgba(0, 0, 0, 0.95);
}

.floating-current {
  position: relative;
  color: #f8fafc;
  font-size: 42px;
  font-weight: 900;
  line-height: 1.18;
}

.floating-current::after {
  content: "";
  position: absolute;
  top: -18%;
  bottom: -18%;
  left: 0;
  width: 36%;
  overflow: hidden;
  background: linear-gradient(
    90deg,
    transparent 0%,
    rgba(255, 255, 255, 0.08) 18%,
    rgba(255, 255, 255, 0.34) 48%,
    rgba(255, 255, 255, 0.08) 78%,
    transparent 100%
  );
  filter: blur(1px);
  opacity: 0.8;
  pointer-events: none;
  transform: translateX(calc((var(--line-progress) * 1.36) - 36%));
}

.floating-next {
  color: rgba(248, 250, 252, 0.82);
  font-size: 24px;
  font-weight: 750;
  line-height: 1.25;
}
</style>
