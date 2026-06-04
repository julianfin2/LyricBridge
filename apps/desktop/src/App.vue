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

type BridgeConnectionStatus = {
  connectedClients: number;
  lastMessageAt: number | null;
};

type ConfigDirectoryStatus = {
  directory: string | null;
  bindingCount: number;
  error: string | null;
};

type LyricBindingWithContent = {
  videoId: string;
  lyricFilePath: string;
  offsetMs: number;
  lyricText: string;
};

type OverlaySettings = {
  locked: boolean;
  visible: boolean;
  alwaysOnTop: boolean;
  x: number | null;
  y: number | null;
  width: number;
  height: number;
};

const serverStatus = ref<BridgeServerStatus>({
  address: "127.0.0.1:32190",
  running: false,
  error: null
});
const connectionStatus = ref<BridgeConnectionStatus>({
  connectedClients: 0,
  lastMessageAt: null
});
const configDirectoryStatus = ref<ConfigDirectoryStatus>({
  directory: null,
  bindingCount: 0,
  error: null
});
const latestState = ref<PlayerState | null>(null);
const latestError = ref<string | null>(null);
const bindingError = ref<string | null>(null);
const activeBinding = ref<LyricBindingWithContent | null>(null);
const loadedVideoId = ref<string | null>(null);
const offsetMs = ref(0);
const overlaySettings = ref<OverlaySettings>({
  locked: false,
  visible: true,
  alwaysOnTop: true,
  x: null,
  y: null,
  width: 1000,
  height: 150
});
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
    return "等待 YouTube 播放";
  }

  return latestState.value.title || latestState.value.videoId || "未命名视频";
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

const extensionStatusLabel = computed(() => {
  if (connectionStatus.value.connectedClients <= 0) {
    return "未连接";
  }

  return `${connectionStatus.value.connectedClients} 个连接`;
});

onMounted(async () => {
  await listen<BridgeServerStatus>("bridge-server-status", (event) => {
    serverStatus.value = event.payload;
  });

  await listen<BridgeConnectionStatus>("bridge-connection-status", (event) => {
    connectionStatus.value = event.payload;
  });

  serverStatus.value = await invoke<BridgeServerStatus>("get_bridge_server_status");
  connectionStatus.value = await invoke<BridgeConnectionStatus>("get_bridge_connection_status");
  configDirectoryStatus.value = await invoke<ConfigDirectoryStatus>("get_config_directory_status");
  overlaySettings.value = await invoke<OverlaySettings>("get_overlay_settings");

  await listen<BridgeMessage>("bridge-message", (event) => {
    if (event.payload.type === "player-state") {
      acceptPlayerState(event.payload.payload);
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

function acceptPlayerState(nextState: PlayerState) {
  const currentState = latestState.value;

  if (!currentState) {
    latestState.value = nextState;
    return;
  }

  if (!nextState.paused) {
    latestState.value = nextState;
    return;
  }

  if (nextState.videoId === currentState.videoId) {
    latestState.value = nextState;
  }
}

async function loadBinding(videoId: string) {
  bindingError.value = null;
  activeBinding.value = null;
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

async function chooseConfigDirectory() {
  const selected = await open({
    directory: true,
    multiple: false
  });

  if (typeof selected === "string") {
    configDirectoryStatus.value = await invoke<ConfigDirectoryStatus>("set_config_directory", {
      directory: selected
    });
  }
}

async function reloadConfigDirectory() {
  configDirectoryStatus.value = await invoke<ConfigDirectoryStatus>("get_config_directory_status");

  const videoId = latestState.value?.videoId;
  if (videoId) {
    loadedVideoId.value = null;
    await loadBinding(videoId);
  }
}

async function clearConfigDirectory() {
  configDirectoryStatus.value = await invoke<ConfigDirectoryStatus>("set_config_directory", {
    directory: null
  });

  const videoId = latestState.value?.videoId;
  if (videoId) {
    loadedVideoId.value = null;
    await loadBinding(videoId);
  }
}

async function setOverlayVisible(visible: boolean) {
  overlaySettings.value = await invoke<OverlaySettings>("set_overlay_visible", { visible });
}

async function setOverlayLocked(locked: boolean) {
  overlaySettings.value = await invoke<OverlaySettings>("set_overlay_locked", { locked });
}

async function setOverlayAlwaysOnTop(alwaysOnTop: boolean) {
  overlaySettings.value = await invoke<OverlaySettings>("set_overlay_always_on_top", {
    alwaysOnTop
  });
}

async function resetOverlay() {
  overlaySettings.value = await invoke<OverlaySettings>("reset_overlay_position");
}

async function startOverlayDrag(event: MouseEvent) {
  if (!isLyricsWindow || overlaySettings.value.locked || event.button !== 0) {
    return;
  }

  await invoke("start_overlay_drag");
}

function applyBinding(binding: LyricBindingWithContent) {
  activeBinding.value = binding;
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

function formatLastUpdate(value: number | null): string {
  if (value === null) {
    return "暂无更新";
  }

  return new Date(value).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  });
}
</script>

<template>
  <main
    v-if="isLyricsWindow"
    class="lyrics-window"
    :class="{ locked: overlaySettings.locked }"
    @mousedown="startOverlayDrag"
  >
    <section class="floating-lyrics">
      <p class="floating-current">{{ currentLyric?.text || "LyricBridge" }}</p>
      <p class="floating-next">{{ nextLyric?.text || "等待已配置的 YouTube 歌词" }}</p>
    </section>
  </main>

  <main v-else class="shell">
    <section class="panel">
      <div class="status-row">
        <span class="status-dot" :class="{ active: serverStatus.running }" />
        <span>
          {{ serverStatus.running ? "桥接服务已启动" : "桥接服务离线" }}
          <small>{{ serverStatus.address }}</small>
        </span>
      </div>

      <header>
        <h1>LyricBridge</h1>
        <p>YouTube 桌面歌词桥接工具</p>
      </header>

      <section class="now-playing">
        <span class="eyebrow">正在播放</span>
        <strong>{{ videoLabel }}</strong>
        <span>{{ progressLabel }}</span>
      </section>

      <dl class="metrics">
        <div>
          <dt>视频 ID</dt>
          <dd>{{ latestState?.videoId || "未检测到" }}</dd>
        </div>
        <div>
          <dt>状态</dt>
          <dd>{{ latestState ? (latestState.paused ? "已暂停" : "播放中") : "空闲" }}</dd>
        </div>
        <div>
          <dt>速度</dt>
          <dd>{{ latestState?.playbackRate ?? 1 }}x</dd>
        </div>
        <div>
          <dt>扩展</dt>
          <dd>{{ extensionStatusLabel }}</dd>
        </div>
        <div>
          <dt>最近更新</dt>
          <dd>{{ formatLastUpdate(connectionStatus.lastMessageAt) }}</dd>
        </div>
      </dl>

      <section class="lyrics">
        <span class="eyebrow">歌词</span>
        <p class="current-line">{{ currentLyric?.text || "当前没有歌词" }}</p>
        <p class="next-line">{{ nextLyric?.text || "配置目录中未找到该视频的歌词" }}</p>
      </section>

      <section class="overlay-controls">
        <span class="eyebrow">桌面歌词窗口</span>
        <div class="control-row">
          <button
            class="secondary-button"
            type="button"
            @click="setOverlayVisible(!overlaySettings.visible)"
          >
            {{ overlaySettings.visible ? "隐藏" : "显示" }}
          </button>
          <button
            class="secondary-button"
            type="button"
            @click="setOverlayLocked(!overlaySettings.locked)"
          >
            {{ overlaySettings.locked ? "解锁" : "锁定" }}
          </button>
          <button
            class="secondary-button"
            type="button"
            @click="setOverlayAlwaysOnTop(!overlaySettings.alwaysOnTop)"
          >
            {{ overlaySettings.alwaysOnTop ? "取消置顶" : "保持置顶" }}
          </button>
          <button class="secondary-button" type="button" @click="resetOverlay">重置</button>
        </div>
        <p class="hint compact">
          {{ overlaySettings.locked ? "已锁定：鼠标点击会穿透歌词窗口。" : "未锁定：拖动歌词窗口可移动位置。" }}
        </p>
      </section>

      <section class="config-controls">
        <span class="eyebrow">配置目录</span>
        <div class="config-row">
          <p class="path-display">
            {{ configDirectoryStatus.directory || "未选择配置目录" }}
          </p>
          <button class="secondary-button" type="button" @click="chooseConfigDirectory">
            浏览
          </button>
          <button class="secondary-button" type="button" @click="reloadConfigDirectory">
            重载
          </button>
          <button class="secondary-button" type="button" @click="clearConfigDirectory">
            清除
          </button>
        </div>
        <p class="hint compact">
          已从 bindings.json 加载 {{ configDirectoryStatus.bindingCount }} 个绑定
        </p>
        <p v-if="configDirectoryStatus.error" class="error compact">
          {{ configDirectoryStatus.error }}
        </p>
      </section>

      <p v-if="activeBinding" class="hint">
        已从 {{ activeBinding.lyricFilePath }} 加载 {{ parsedLrc?.lines.length ?? 0 }} 行歌词
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

html,
body,
#app {
  height: 100%;
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
  height: 100vh;
  overflow: auto;
  padding: 0;
  box-sizing: border-box;
}

.panel {
  width: 100%;
  min-height: 100%;
  background: #ffffff;
  border: 0;
  border-radius: 0;
  padding: 24px;
  box-shadow: none;
  box-sizing: border-box;
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
.lyrics,
.overlay-controls,
.config-controls {
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
  grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
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

.overlay-controls,
.config-controls {
  margin-bottom: 18px;
}

.control-row {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
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

.config-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) repeat(3, 88px);
  gap: 8px;
}

.path-display {
  min-width: 0;
  min-height: 38px;
  display: flex;
  align-items: center;
  margin: 0;
  padding: 0 10px;
  overflow-wrap: anywhere;
  color: #1f2933;
  background: #ffffff;
  border: 1px solid #cbd5df;
  border-radius: 6px;
  box-sizing: border-box;
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

.compact {
  margin-top: 4px;
}

.error {
  color: #b42318;
}

@media (max-width: 680px) {
  .metrics,
  .control-row,
  .config-row {
    grid-template-columns: 1fr;
  }
}

.lyrics-window {
  min-height: 100vh;
  display: grid;
  align-items: center;
  padding: 10px 26px;
  border: 2px solid transparent;
  border-radius: 8px;
  box-sizing: border-box;
  background: transparent;
  cursor: move;
  user-select: none;
  transition:
    border-color 120ms ease,
    background-color 120ms ease;
}

.lyrics-window:hover {
  background: rgba(15, 23, 42, 0.08);
  border-color: rgba(255, 255, 255, 0.32);
}

.lyrics-window.locked {
  cursor: default;
}

.floating-lyrics {
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

.floating-next {
  color: rgba(248, 250, 252, 0.82);
  font-size: 24px;
  font-weight: 750;
  line-height: 1.25;
}
</style>
