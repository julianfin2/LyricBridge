<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import type { BridgeMessage, PlayerState } from "@lyricbridge/shared";
import { computed, onMounted, ref } from "vue";

type BridgeServerStatus = {
  address: string;
  running: boolean;
  error: string | null;
};

const serverStatus = ref<BridgeServerStatus>({
  address: "127.0.0.1:32190",
  running: false,
  error: null
});
const latestState = ref<PlayerState | null>(null);
const latestError = ref<string | null>(null);

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

onMounted(async () => {
  await listen<BridgeServerStatus>("bridge-server-status", (event) => {
    serverStatus.value = event.payload;
  });

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
  <main class="shell">
    <section class="panel">
      <div class="status-row">
        <span class="status-dot" :class="{ active: serverStatus.running }" />
        <span>
          {{ serverStatus.running ? "Bridge listening" : "Bridge offline" }}
          <small>{{ serverStatus.address }}</small>
        </span>
      </div>

      <h1>LyricBridge</h1>
      <p class="subtitle">YouTube playback bridge for desktop lyrics.</p>

      <div class="now-playing">
        <span class="eyebrow">Now playing</span>
        <strong>{{ videoLabel }}</strong>
        <span>{{ progressLabel }}</span>
      </div>

      <dl>
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

      <p v-if="serverStatus.error" class="error">{{ serverStatus.error }}</p>
      <p v-if="latestError" class="error">{{ latestError }}</p>
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

.shell {
  min-height: 100vh;
  display: grid;
  place-items: center;
  padding: 32px;
  box-sizing: border-box;
}

.panel {
  width: min(720px, 100%);
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

h1 {
  margin: 24px 0 8px;
  font-size: 34px;
  line-height: 1.15;
}

.subtitle {
  margin: 0 0 28px;
  color: #52606d;
}

.now-playing {
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

dl {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
  margin: 18px 0 0;
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

.error {
  color: #b42318;
}
</style>
