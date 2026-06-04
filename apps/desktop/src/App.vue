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

  await listen<OverlaySettings>("overlay-settings-changed", (event) => {
    overlaySettings.value = event.payload;
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
  if (!isLyricsWindow || event.button !== 0) {
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
    <header class="topbar">
      <div class="brand">
        <img src="/icon.png" alt="" />
        <div>
          <h1>LyricBridge</h1>
          <p>YouTube 桌面歌词桥接工具</p>
        </div>
      </div>
      <div class="status-stack">
        <div class="status-pill" :class="{ online: serverStatus.running }">
          <span class="status-dot" />
          <span>{{ serverStatus.running ? "桥接服务已启动" : "桥接服务离线" }}</span>
        </div>
        <small>{{ serverStatus.address }}</small>
      </div>
    </header>

    <section class="hero-band">
      <div class="now-playing">
        <span class="eyebrow">正在播放</span>
        <strong>{{ videoLabel }}</strong>
        <span>{{ progressLabel }}</span>
      </div>
      <dl class="metrics">
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
    </section>

    <section class="content-grid">
      <section class="lyrics surface primary-surface">
        <div class="section-heading">
          <span class="eyebrow">歌词</span>
          <span class="video-id">{{ latestState?.videoId || "未检测到视频 ID" }}</span>
        </div>
        <p class="current-line">{{ currentLyric?.text || "当前没有歌词" }}</p>
        <p class="next-line">{{ nextLyric?.text || "配置目录中未找到该视频的歌词" }}</p>
        <p v-if="activeBinding" class="hint">
          已加载 {{ parsedLrc?.lines.length ?? 0 }} 行歌词
        </p>
      </section>

      <aside class="side-column">
        <section class="surface">
          <div class="section-heading">
            <span class="eyebrow">桌面歌词窗口</span>
          </div>
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

        <section class="surface">
          <div class="section-heading">
            <span class="eyebrow">配置目录</span>
            <span class="binding-count">{{ configDirectoryStatus.bindingCount }} 个绑定</span>
          </div>
          <p class="path-display">
            {{ configDirectoryStatus.directory || "未选择配置目录" }}
          </p>
          <div class="config-actions">
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
          <p v-if="activeBinding" class="hint compact">
            {{ activeBinding.lyricFilePath }}
          </p>
          <p v-if="configDirectoryStatus.error" class="error compact">
            {{ configDirectoryStatus.error }}
          </p>
        </section>
      </aside>
    </section>

    <section v-if="serverStatus.error || latestError || bindingError" class="error-list">
      <p v-if="serverStatus.error" class="error">{{ serverStatus.error }}</p>
      <p v-if="latestError" class="error">{{ latestError }}</p>
      <p v-if="bindingError" class="error">{{ bindingError }}</p>
    </section>
  </main>
</template>

<style>
:root {
  color: #172033;
  background: #edf4f6;
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
  background:
    linear-gradient(135deg, rgba(16, 185, 129, 0.12), transparent 38%),
    linear-gradient(315deg, rgba(59, 130, 246, 0.12), transparent 34%),
    #edf4f6;
  box-sizing: border-box;
}

.topbar {
  min-height: 72px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  padding: 12px 22px;
  background: rgba(255, 255, 255, 0.82);
  border-bottom: 1px solid rgba(142, 159, 176, 0.22);
  box-sizing: border-box;
  backdrop-filter: blur(14px);
}

.brand {
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 12px;
}

.brand img {
  width: 42px;
  height: 42px;
  flex: 0 0 auto;
  border-radius: 12px;
}

h1 {
  margin: 0 0 3px;
  font-size: 25px;
  line-height: 1.1;
}

.brand p {
  margin: 0;
  color: #637083;
  font-size: 13px;
}

.status-stack {
  display: grid;
  justify-items: end;
  gap: 5px;
  color: #637083;
  font-size: 12px;
}

.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 30px;
  padding: 0 10px;
  color: #7f1d1d;
  background: #fff1f2;
  border: 1px solid rgba(244, 63, 94, 0.28);
  border-radius: 999px;
  font-size: 13px;
  font-weight: 700;
}

.status-pill.online {
  color: #065f46;
  background: #e8fff4;
  border-color: rgba(16, 185, 129, 0.28);
}

.status-dot {
  width: 9px;
  height: 9px;
  border-radius: 999px;
  background: #ef4444;
  box-shadow: 0 0 0 4px rgba(239, 68, 68, 0.12);
}

.status-pill.online .status-dot {
  background: #10b981;
  box-shadow: 0 0 0 4px rgba(16, 185, 129, 0.14);
}

.hero-band {
  display: grid;
  grid-template-columns: minmax(0, 1.45fr) minmax(360px, 1fr);
  gap: 14px;
  padding: 16px 22px 0;
}

.now-playing {
  min-width: 0;
  display: grid;
  align-content: center;
  gap: 8px;
  min-height: 108px;
  padding: 16px 18px;
  color: #f8fafc;
  background:
    linear-gradient(135deg, rgba(20, 184, 166, 0.92), rgba(37, 99, 235, 0.86)),
    #0f766e;
  border: 1px solid rgba(255, 255, 255, 0.38);
  border-radius: 16px;
  box-shadow: 0 12px 26px rgba(15, 118, 110, 0.18);
}

.now-playing strong {
  min-width: 0;
  overflow: hidden;
  font-size: 24px;
  line-height: 1.25;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.eyebrow {
  color: #64748b;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0;
}

.metrics {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin: 0;
}

.metrics div,
.surface {
  background: rgba(255, 255, 255, 0.82);
  border: 1px solid rgba(142, 159, 176, 0.22);
  border-radius: 14px;
  box-shadow: 0 10px 22px rgba(43, 58, 84, 0.07);
}

.metrics div {
  min-width: 0;
  padding: 10px 13px;
}

dt,
.video-id,
.binding-count {
  color: #64748b;
  font-size: 13px;
}

dd {
  margin: 4px 0 0;
  overflow-wrap: anywhere;
  color: #172033;
  font-size: 17px;
  font-weight: 750;
}

.content-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.45fr) minmax(360px, 1fr);
  gap: 14px;
  padding: 12px 22px 18px;
}

.surface {
  min-width: 0;
  padding: 12px 14px;
  box-sizing: border-box;
}

.lyrics {
  display: grid;
  gap: 8px;
}

.primary-surface {
  min-height: 142px;
  align-content: start;
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.94), rgba(248, 250, 252, 0.86)),
    #ffffff;
}

.side-column {
  display: grid;
  align-content: start;
  gap: 14px;
}

.section-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
}

.video-id,
.binding-count {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.current-line {
  min-height: 38px;
  margin: 0;
  color: #111827;
  font-size: 28px;
  font-weight: 800;
  line-height: 1.28;
}

.next-line {
  min-height: 26px;
  margin: 0;
  color: #64748b;
  font-size: 16px;
  line-height: 1.4;
}

.control-row {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 9px;
}

button {
  min-width: 0;
  height: 36px;
  color: #ffffff;
  background: #0f766e;
  border: 1px solid rgba(15, 118, 110, 0.28);
  border-radius: 10px;
  cursor: pointer;
  font-weight: 700;
  transition:
    transform 120ms ease,
    border-color 120ms ease,
    box-shadow 120ms ease;
}

button:hover {
  transform: translateY(-1px);
  box-shadow: 0 8px 18px rgba(15, 118, 110, 0.14);
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.secondary-button {
  color: #172033;
  background: #f8fafc;
  border-color: rgba(148, 163, 184, 0.34);
}

.config-actions {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 9px;
  margin-top: 9px;
}

.path-display {
  min-width: 0;
  min-height: 40px;
  display: flex;
  align-items: center;
  margin: 9px 0 0;
  padding: 0 12px;
  overflow-wrap: anywhere;
  color: #172033;
  background: #f8fafc;
  border: 1px solid rgba(148, 163, 184, 0.28);
  border-radius: 10px;
  box-sizing: border-box;
}

.hint,
.error {
  margin: 9px 0 0;
  overflow-wrap: anywhere;
  font-size: 14px;
}

.hint {
  color: #64748b;
}

.compact {
  margin-top: 8px;
}

.error {
  color: #b42318;
}

.error-list {
  margin: 0 22px 20px;
  padding: 12px 14px;
  background: #fff1f2;
  border: 1px solid rgba(244, 63, 94, 0.22);
  border-radius: 14px;
}

@media (max-width: 860px) {
  .topbar,
  .hero-band,
  .content-grid {
    grid-template-columns: 1fr;
  }

  .topbar {
    align-items: flex-start;
    flex-direction: column;
  }

  .status-stack {
    justify-items: start;
  }

  .hero-band,
  .content-grid {
    padding-left: 16px;
    padding-right: 16px;
  }
}

@media (max-width: 560px) {
  .metrics,
  .control-row,
  .config-actions {
    grid-template-columns: 1fr;
  }

  .now-playing strong,
  .current-line {
    font-size: 24px;
    white-space: normal;
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
