<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
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

type OverlayStyleSettings = {
  currentFontSize: number;
  nextFontSize: number;
  currentColor: string;
  nextColor: string;
  shadowColor: string;
  shadowOpacity: number;
  fontWeight: number;
};

type ConfigDirectoryChangedEvent = {
  source: "main" | "lyrics";
  updatedAt: number;
};

type LogEntry = {
  id: number;
  kind: "info" | "success" | "warning" | "error";
  message: string;
  timestamp: number;
};

const CONFIG_DIRECTORY_CHANGED_EVENT = "config-directory-changed";
const MAX_LOG_ENTRIES = 120;
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
  directory: "https://karlblue.github.io/lyricbridge-bindings/",
  bindingCount: 0,
  error: null
});
const bindingSourceUrl = ref("https://karlblue.github.io/lyricbridge-bindings/");
const latestState = ref<PlayerState | null>(null);
const latestError = ref<string | null>(null);
const bindingError = ref<string | null>(null);
const activeBinding = ref<LyricBindingWithContent | null>(null);
const logs = ref<LogEntry[]>([]);
const loadedVideoId = ref<string | null>(null);
const lastLoggedVideoId = ref<string | null>(null);
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
const overlayStyleSettings = ref<OverlayStyleSettings>({
  currentFontSize: 42,
  nextFontSize: 24,
  currentColor: "#f8fafc",
  nextColor: "#f8fafc",
  shadowColor: "#000000",
  shadowOpacity: 0.9,
  fontWeight: 900
});
const isLyricsWindow = new URLSearchParams(window.location.search).get("window") === "lyrics";
const windowSource = isLyricsWindow ? "lyrics" : "main";

if (isLyricsWindow) {
  document.documentElement.classList.add("lyrics-root");
  window.addEventListener("contextmenu", (event) => {
    event.preventDefault();
  });
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

const previousLyric = computed(() => {
  if (!parsedLrc.value || activeLyricIndex.value <= 0) {
    return null;
  }

  return parsedLrc.value.lines[activeLyricIndex.value - 1] ?? null;
});

const earlierLyric = computed(() => {
  if (!parsedLrc.value || activeLyricIndex.value <= 1) {
    return null;
  }

  return parsedLrc.value.lines[activeLyricIndex.value - 2] ?? null;
});

const nextLyric = computed(() => {
  if (!parsedLrc.value || activeLyricIndex.value < 0) {
    return null;
  }

  return parsedLrc.value.lines[activeLyricIndex.value + 1] ?? null;
});

const floatingNextLyricText = computed(() => {
  if (nextLyric.value) {
    return nextLyric.value.text;
  }

  return activeBinding.value ? " " : "等待已配置的 YouTube 歌词";
});

const previewNextLyricText = computed(() => {
  if (nextLyric.value) {
    return nextLyric.value.text;
  }

  return activeBinding.value ? " " : "配置目录中未找到该视频的歌词";
});

const upcomingLyric = computed(() => {
  if (!parsedLrc.value || activeLyricIndex.value < 0) {
    return null;
  }

  return parsedLrc.value.lines[activeLyricIndex.value + 2] ?? null;
});

const laterLyric = computed(() => {
  if (!parsedLrc.value || activeLyricIndex.value < 0) {
    return null;
  }

  return parsedLrc.value.lines[activeLyricIndex.value + 3] ?? null;
});

const playbackProgressPercent = computed(() => {
  if (!latestState.value?.duration || latestState.value.duration <= 0) {
    return 0;
  }

  const percent = (latestState.value.currentTime / latestState.value.duration) * 100;
  return Math.min(100, Math.max(0, percent));
});

const overlayStyleVars = computed(() => ({
  "--floating-current-size": `${overlayStyleSettings.value.currentFontSize}px`,
  "--floating-next-size": `${overlayStyleSettings.value.nextFontSize}px`,
  "--floating-current-color": overlayStyleSettings.value.currentColor,
  "--floating-next-color": hexToRgba(
    overlayStyleSettings.value.nextColor,
    Math.min(1, overlayStyleSettings.value.shadowOpacity + 0.02)
  ),
  "--floating-shadow-color": hexToRgba(
    overlayStyleSettings.value.shadowColor,
    overlayStyleSettings.value.shadowOpacity
  ),
  "--floating-font-weight": String(overlayStyleSettings.value.fontWeight),
  "--floating-next-font-weight": String(Math.min(overlayStyleSettings.value.fontWeight, 750))
}));

const extensionStatusLabel = computed(() => {
  if (connectionStatus.value.connectedClients <= 0) {
    return "未连接";
  }

  return `${connectionStatus.value.connectedClients} 个连接`;
});

const primaryStatus = computed(() => {
  if (!serverStatus.value.running) {
    return { kind: "offline", label: "桥接服务离线" };
  }

  if (connectionStatus.value.connectedClients <= 0) {
    return { kind: "waiting", label: "等待扩展连接" };
  }

  if (!latestState.value) {
    return { kind: "waiting", label: "等待 YouTube 播放" };
  }

  if (!activeBinding.value || !parsedLrc.value) {
    return { kind: "warning", label: "未找到歌词配置" };
  }

  if (latestState.value.paused) {
    return { kind: "paused", label: "已暂停同步" };
  }

  return { kind: "online", label: "正在同步歌词" };
});

onMounted(async () => {
  await listen<BridgeServerStatus>("bridge-server-status", (event) => {
    serverStatus.value = event.payload;
    addLog(
      event.payload.running ? "success" : "error",
      event.payload.running ? "桥接服务已启动" : `桥接服务离线：${event.payload.error || "未知错误"}`
    );
  });

  await listen<BridgeConnectionStatus>("bridge-connection-status", (event) => {
    const previousCount = connectionStatus.value.connectedClients;
    connectionStatus.value = event.payload;
    if (event.payload.connectedClients !== previousCount) {
      addLog(
        event.payload.connectedClients > 0 ? "success" : "warning",
        `扩展连接数：${event.payload.connectedClients}`
      );
    }
  });

  await listen<OverlaySettings>("overlay-settings-changed", (event) => {
    overlaySettings.value = event.payload;
  });

  await listen<OverlayStyleSettings>("overlay-style-settings-changed", (event) => {
    overlayStyleSettings.value = event.payload;
  });

  await listen<ConfigDirectoryChangedEvent>(CONFIG_DIRECTORY_CHANGED_EVENT, async (event) => {
    if (event.payload.source === windowSource) {
      return;
    }

    configDirectoryStatus.value = await invoke<ConfigDirectoryStatus>("get_config_directory_status");
    await reloadCurrentVideoBinding();
    addLog("info", "其他窗口更新了绑定源，已重新加载当前歌词");
  });

  serverStatus.value = await invoke<BridgeServerStatus>("get_bridge_server_status");
  connectionStatus.value = await invoke<BridgeConnectionStatus>("get_bridge_connection_status");
  configDirectoryStatus.value = await invoke<ConfigDirectoryStatus>("get_config_directory_status");
  bindingSourceUrl.value = configDirectoryStatus.value.directory || bindingSourceUrl.value;
  overlaySettings.value = await invoke<OverlaySettings>("get_overlay_settings");
  overlayStyleSettings.value = await invoke<OverlayStyleSettings>("get_overlay_style_settings");
  addLog("info", "应用状态已加载");

  await listen<BridgeMessage>("bridge-message", (event) => {
    if (event.payload.type === "player-state") {
      const accepted = acceptPlayerState(event.payload.payload);
      latestError.value = null;
      if (
        accepted &&
        event.payload.payload.videoId &&
        event.payload.payload.videoId !== lastLoggedVideoId.value
      ) {
        lastLoggedVideoId.value = event.payload.payload.videoId;
        addLog("info", `检测到视频：${event.payload.payload.title || event.payload.payload.videoId}`);
      }
    }
  });

  await listen<string>("bridge-message-error", (event) => {
    latestError.value = event.payload;
    addLog("error", `桥接消息解析失败：${event.payload}`);
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

function acceptPlayerState(nextState: PlayerState): boolean {
  const currentState = latestState.value;

  if (!currentState) {
    latestState.value = nextState;
    return true;
  }

  if (nextState.videoId !== currentState.videoId) {
    if (nextState.paused && !currentState.paused) {
      return false;
    }

    if (nextState.paused && currentState.paused && !shouldAcceptPausedVideoSwitch(nextState, currentState)) {
      return false;
    }

    latestState.value = nextState;
    return true;
  }

  if (!nextState.paused) {
    latestState.value = nextState;
    return true;
  }

  if (nextState.videoId === currentState.videoId) {
    latestState.value = nextState;
    return true;
  }

  return false;
}

function shouldAcceptPausedVideoSwitch(nextState: PlayerState, currentState: PlayerState): boolean {
  if (nextState.sourceTabActive === true) {
    return true;
  }

  if (currentState.sourceTabActive === true) {
    return false;
  }

  return false;
}

async function loadBinding(videoId: string) {
  bindingError.value = null;
  activeBinding.value = null;
  offsetMs.value = 0;
  const videoName = formatVideoLogLabel(videoId);

  try {
    const binding = await invoke<LyricBindingWithContent | null>("get_lyric_binding", {
      videoId
    });

    if (!binding) {
      addLog("warning", `未找到歌词绑定：${videoName}`);
      return;
    }

    applyBinding(binding);
    addLog("success", `已加载歌词：${videoName}`);
  } catch (error) {
    bindingError.value = String(error);
    addLog("error", `加载歌词失败：${bindingError.value}`);
  }
}

async function syncRemoteBindings() {
  bindingError.value = null;
  addLog("info", "正在同步绑定源");

  try {
    configDirectoryStatus.value = await invoke<ConfigDirectoryStatus>("sync_remote_bindings", {
      sourceUrl: bindingSourceUrl.value
    });
    bindingSourceUrl.value = configDirectoryStatus.value.directory || bindingSourceUrl.value;

    await reloadCurrentVideoBinding();
    await notifyConfigDirectoryChanged();
    addLog("success", `绑定源同步完成：${configDirectoryStatus.value.bindingCount} 个有效绑定`);
  } catch (error) {
    bindingError.value = String(error);
    addLog("error", `同步绑定源失败：${bindingError.value}`);
  }
}

async function reloadConfigDirectory() {
  bindingError.value = null;
  addLog("info", "正在重新加载本地绑定源");

  try {
    configDirectoryStatus.value = await invoke<ConfigDirectoryStatus>("get_config_directory_status");
    bindingSourceUrl.value = configDirectoryStatus.value.directory || bindingSourceUrl.value;

    await reloadCurrentVideoBinding();
    await notifyConfigDirectoryChanged();
    addLog("success", `本地绑定源已加载：${configDirectoryStatus.value.bindingCount} 个有效绑定`);
  } catch (error) {
    bindingError.value = String(error);
    addLog("error", `加载本地绑定源失败：${bindingError.value}`);
  }
}

async function updateCurrentLyric() {
  const videoId = latestState.value?.videoId;
  if (!videoId) {
    bindingError.value = "未检测到当前视频，无法更新歌词。";
    addLog("warning", bindingError.value);
    return;
  }
  const videoName = formatVideoLogLabel(videoId);

  try {
    bindingError.value = null;
    addLog("info", `正在更新歌词：${videoName}`);
    const binding = await invoke<LyricBindingWithContent | null>("update_lyric_binding", {
      videoId
    });

    if (binding) {
      applyBinding(binding);
      addLog("success", `歌词已更新：${videoName}`);
    } else {
      activeBinding.value = null;
      offsetMs.value = 0;
      addLog("warning", `未找到可更新的歌词：${videoName}`);
    }

    await notifyConfigDirectoryChanged();
  } catch (error) {
    bindingError.value = String(error);
    addLog("error", `更新歌词失败：${bindingError.value}`);
  }
}

async function reloadCurrentVideoBinding() {
  const videoId = latestState.value?.videoId;
  if (videoId) {
    loadedVideoId.value = null;
    await loadLocalBinding(videoId);
  }
}

async function loadLocalBinding(videoId: string) {
  bindingError.value = null;
  activeBinding.value = null;
  offsetMs.value = 0;
  const videoName = formatVideoLogLabel(videoId);

  try {
    const binding = await invoke<LyricBindingWithContent | null>("get_local_lyric_binding", {
      videoId
    });

    if (!binding) {
      addLog("warning", `本地缓存未找到歌词：${videoName}`);
      return;
    }

    applyBinding(binding);
    addLog("success", `已加载本地歌词：${videoName}`);
  } catch (error) {
    bindingError.value = String(error);
    addLog("error", `加载本地歌词失败：${bindingError.value}`);
  }
}

async function notifyConfigDirectoryChanged() {
  await emit<ConfigDirectoryChangedEvent>(CONFIG_DIRECTORY_CHANGED_EVENT, {
    source: windowSource,
    updatedAt: Date.now()
  });
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

async function updateOverlaySize() {
  overlaySettings.value = await invoke<OverlaySettings>("set_overlay_size", {
    width: overlaySettings.value.width,
    height: overlaySettings.value.height
  });
}

async function resetOverlay() {
  overlaySettings.value = await invoke<OverlaySettings>("reset_overlay_position");
}

async function updateOverlayStyle() {
  overlayStyleSettings.value = await invoke<OverlayStyleSettings>("set_overlay_style_settings", {
    settings: overlayStyleSettings.value
  });
}

async function resetOverlayStyle() {
  overlayStyleSettings.value = await invoke<OverlayStyleSettings>("reset_overlay_style_settings");
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
    hour12: false,
    hourCycle: "h23",
    minute: "2-digit",
    second: "2-digit"
  });
}

function addLog(kind: LogEntry["kind"], message: string) {
  const trimmedMessage = message.trim();
  if (!trimmedMessage) {
    return;
  }

  logs.value = [
    {
      id: Date.now() + Math.random(),
      kind,
      message: trimmedMessage,
      timestamp: Date.now()
    },
    ...logs.value
  ].slice(0, MAX_LOG_ENTRIES);
}

function formatVideoLogLabel(videoId: string): string {
  const currentState = latestState.value;
  if (currentState?.videoId === videoId && currentState.title) {
    return currentState.title;
  }

  return videoId;
}

function hexToRgba(hex: string, alpha: number): string {
  const normalized = /^#[0-9a-fA-F]{6}$/.test(hex) ? hex : "#000000";
  const red = Number.parseInt(normalized.slice(1, 3), 16);
  const green = Number.parseInt(normalized.slice(3, 5), 16);
  const blue = Number.parseInt(normalized.slice(5, 7), 16);
  return `rgba(${red}, ${green}, ${blue}, ${alpha.toFixed(2)})`;
}
</script>

<template>
  <main
    v-if="isLyricsWindow"
    class="lyrics-window"
    :class="{ locked: overlaySettings.locked }"
    :style="overlayStyleVars"
    @mousedown="startOverlayDrag"
  >
    <section class="floating-lyrics">
      <p class="floating-current">{{ currentLyric?.text || "LyricBridge" }}</p>
      <p class="floating-next">{{ floatingNextLyricText }}</p>
    </section>
  </main>

  <main v-else class="shell">
    <header class="topbar">
      <div class="brand">
        <img src="/icon.png" alt="" />
        <div>
          <h1>Youtube 歌词同步器</h1>
          <!-- <p>YouTube 桌面歌词桥接工具</p> -->
        </div>
      </div>
      <div class="status-stack">
        <div class="status-pill" :class="primaryStatus.kind">
          <span class="status-dot" />
          <span>{{ primaryStatus.label }}</span>
        </div>
        <!-- <small>{{ serverStatus.address }}</small> -->
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
      <div class="main-column">
        <section class="lyrics surface primary-surface">
          <div class="section-heading">
            <span class="eyebrow">歌词</span>
            <span class="video-id">{{ latestState?.videoId || "未检测到视频 ID" }}</span>
          </div>
          <div class="lyric-context">
            <p class="context-line faded">{{ earlierLyric?.text || " " }}</p>
            <p class="context-line muted">{{ previousLyric?.text || " " }}</p>
            <p class="current-line">{{ currentLyric?.text || "当前没有歌词" }}</p>
            <p class="context-line">{{ previewNextLyricText }}</p>
            <p class="context-line muted">{{ upcomingLyric?.text || " " }}</p>
            <p class="context-line faded">{{ laterLyric?.text || " " }}</p>
          </div>
          <div class="lyric-bottom">
            <div class="lyric-progress">
              <span :style="{ width: `${playbackProgressPercent}%` }" />
            </div>
            <div class="lyric-footer">
              <span>{{ progressLabel }}</span>
              <span v-if="activeBinding">已加载 {{ parsedLrc?.lines.length ?? 0 }} 行歌词</span>
            </div>
          </div>
        </section>

        <section class="surface log-panel">
          <div class="section-heading">
            <span class="eyebrow">日志</span>
            <span class="binding-count">{{ logs.length }} 条</span>
          </div>
          <div class="log-list">
            <p v-if="logs.length === 0" class="log-empty">暂无日志</p>
            <article v-for="entry in logs" :key="entry.id" class="log-entry" :class="entry.kind">
              <time>{{ formatLastUpdate(entry.timestamp) }}</time>
              <span>{{ entry.message }}</span>
            </article>
          </div>
        </section>
      </div>

      <aside class="side-column">
        <section class="surface">
          <div class="section-heading">
            <span class="eyebrow">桌面歌词窗口</span>
            <button class="link-button" type="button" @click="resetOverlay">重置</button>
          </div>
          <!-- <p class="hint compact">
            {{ overlaySettings.locked ? "已锁定：鼠标点击会穿透歌词窗口。" : "未锁定：拖动歌词窗口可移动位置。" }}
          </p> -->
          <div class="window-size-controls">
            <label class="range-control">
              <span>宽度</span>
              <input
                v-model.number="overlaySettings.width"
                type="range"
                min="480"
                max="1600"
                step="20"
                @input="updateOverlaySize"
              />
              <strong>{{ overlaySettings.width }}px</strong>
            </label>
            <label class="range-control">
              <span>高度</span>
              <input
                v-model.number="overlaySettings.height"
                type="range"
                min="90"
                max="320"
                step="10"
                @input="updateOverlaySize"
              />
              <strong>{{ overlaySettings.height }}px</strong>
            </label>
          </div>
          <div class="control-row overlay-action-row">
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
          </div>
        </section>

        <section class="surface">
          <div class="section-heading">
            <span class="eyebrow">歌词样式</span>
            <button class="link-button" type="button" @click="resetOverlayStyle">重置</button>
          </div>
          <div class="style-controls">
            <label class="range-control">
              <span>当前句字号</span>
              <input
                v-model.number="overlayStyleSettings.currentFontSize"
                type="range"
                min="24"
                max="72"
                @input="updateOverlayStyle"
              />
              <strong>{{ overlayStyleSettings.currentFontSize }}px</strong>
            </label>
            <label class="range-control">
              <span>下一句字号</span>
              <input
                v-model.number="overlayStyleSettings.nextFontSize"
                type="range"
                min="14"
                max="48"
                @input="updateOverlayStyle"
              />
              <strong>{{ overlayStyleSettings.nextFontSize }}px</strong>
            </label>
            <div class="color-grid">
              <label>
                <span>当前句</span>
                <input
                  v-model="overlayStyleSettings.currentColor"
                  type="color"
                  @input="updateOverlayStyle"
                />
              </label>
              <label>
                <span>下一句</span>
                <input
                  v-model="overlayStyleSettings.nextColor"
                  type="color"
                  @input="updateOverlayStyle"
                />
              </label>
              <label>
                <span>阴影</span>
                <input
                  v-model="overlayStyleSettings.shadowColor"
                  type="color"
                  @input="updateOverlayStyle"
                />
              </label>
            </div>
            <label class="range-control">
              <span>阴影强度</span>
              <input
                v-model.number="overlayStyleSettings.shadowOpacity"
                type="range"
                min="0"
                max="1"
                step="0.05"
                @input="updateOverlayStyle"
              />
              <strong>{{ Math.round(overlayStyleSettings.shadowOpacity * 100) }}%</strong>
            </label>
            <label class="select-control">
              <span>字重</span>
              <select v-model.number="overlayStyleSettings.fontWeight" @change="updateOverlayStyle">
                <option :value="700">粗</option>
                <option :value="800">更粗</option>
                <option :value="900">特粗</option>
                <option :value="600">半粗</option>
                <option :value="400">普通</option>
              </select>
            </label>
          </div>
        </section>

        <section class="surface">
          <div class="section-heading">
            <span class="eyebrow">绑定源</span>
            <span class="binding-count">{{ configDirectoryStatus.bindingCount }} 个绑定</span>
          </div>
          <input
            v-model="bindingSourceUrl"
            class="path-display source-input"
            type="url"
            spellcheck="false"
          />
          <div class="config-actions">
            <button class="secondary-button" type="button" @click="syncRemoteBindings">
              同步绑定
            </button>
            <button class="secondary-button" type="button" @click="updateCurrentLyric">
              更新歌词
            </button>
            <button class="secondary-button" type="button" @click="reloadConfigDirectory">
              本地重载
            </button>
          </div>
          <!-- <p v-if="activeBinding" class="hint compact">
            {{ activeBinding.lyricFilePath }}
          </p> -->
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

.status-pill.waiting {
  color: #075985;
  background: #eff6ff;
  border-color: rgba(14, 165, 233, 0.24);
}

.status-pill.warning {
  color: #92400e;
  background: #fffbeb;
  border-color: rgba(245, 158, 11, 0.26);
}

.status-pill.paused {
  color: #854d0e;
  background: #fefce8;
  border-color: rgba(234, 179, 8, 0.28);
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

.status-pill.waiting .status-dot {
  background: #38bdf8;
  box-shadow: 0 0 0 4px rgba(56, 189, 248, 0.14);
}

.status-pill.warning .status-dot,
.status-pill.paused .status-dot {
  background: #f59e0b;
  box-shadow: 0 0 0 4px rgba(245, 158, 11, 0.14);
}

.status-pill.online .status-dot {
  background: #10b981;
  box-shadow: 0 0 0 4px rgba(16, 185, 129, 0.14);
}

.hero-band {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(300px, 340px);
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
  grid-template-columns: minmax(0, 1fr) minmax(300px, 340px);
  align-items: stretch;
  gap: 14px;
  min-height: 0;
  padding: 12px 22px 18px;
  box-sizing: border-box;
}

.surface {
  min-width: 0;
  padding: 12px 14px;
  box-sizing: border-box;
}

.main-column {
  min-width: 0;
  min-height: 0;
  display: grid;
  grid-template-rows: minmax(0, 1fr) 150px;
  gap: 14px;
}

.lyrics {
  min-height: 0;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  gap: 12px;
}

.primary-surface {
  padding-bottom: 10px;
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

.lyric-context {
  display: grid;
  align-content: center;
  gap: 9px;
  min-width: 0;
  min-height: 0;
  text-align: center;
}

.current-line,
.context-line {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.current-line {
  color: #111827;
  font-size: 29px;
  font-weight: 800;
  line-height: 1.28;
}

.context-line {
  color: #64748b;
  font-size: 16px;
  line-height: 1.35;
}

.context-line.muted {
  color: #94a3b8;
}

.context-line.faded {
  color: #c0cad7;
}

.lyric-bottom {
  display: grid;
  gap: 8px;
}

.lyric-progress {
  height: 6px;
  overflow: hidden;
  background: #e2e8f0;
  border-radius: 999px;
}

.lyric-progress span {
  display: block;
  height: 100%;
  background: linear-gradient(90deg, #14b8a6, #38bdf8);
  border-radius: inherit;
  transition: width 180ms ease;
}

.lyric-footer {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  color: #64748b;
  font-size: 13px;
}

.lyric-footer span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log-panel {
  min-height: 0;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 8px;
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.9), rgba(248, 250, 252, 0.82)),
    #ffffff;
}

.log-list {
  min-height: 0;
  display: grid;
  align-content: start;
  gap: 3px;
  overflow: auto;
  padding-right: 4px;
}

.log-empty,
.log-entry {
  margin: 0;
  min-width: 0;
}

.log-empty {
  color: #94a3b8;
  font-size: 13px;
}

.log-entry {
  display: grid;
  grid-template-columns: 58px minmax(0, 1fr);
  gap: 10px;
  align-items: start;
  padding: 2px 0;
  color: #334155;
  font-size: 13px;
  line-height: 1.45;
}

.log-entry time {
  color: #64748b;
  font-size: 12px;
  white-space: nowrap;
}

.log-entry span {
  min-width: 0;
  overflow-wrap: anywhere;
}

.log-entry.success {
  color: #047857;
}

.log-entry.info {
  color: #334155;
}

.log-entry.warning {
  color: #b45309;
}

.log-entry.error {
  color: #b42318;
}

.control-row {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 9px;
}

.overlay-action-row {
  margin-top: 10px;
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

.link-button {
  height: auto;
  padding: 0;
  color: #0f766e;
  background: transparent;
  border: 0;
  border-radius: 0;
  font-size: 12px;
  box-shadow: none;
}

.link-button:hover {
  transform: none;
  box-shadow: none;
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

.source-input {
  width: 100%;
  outline: none;
}

.source-input:focus {
  border-color: rgba(20, 184, 166, 0.52);
  box-shadow: 0 0 0 3px rgba(20, 184, 166, 0.12);
}

.style-controls {
  display: grid;
  gap: 10px;
  margin-top: 10px;
}

.window-size-controls {
  display: grid;
  gap: 10px;
  margin-top: 10px;
}

.range-control,
.select-control {
  display: grid;
  grid-template-columns: 74px minmax(0, 1fr) 44px;
  align-items: center;
  gap: 8px;
  color: #64748b;
  font-size: 12px;
  font-weight: 700;
}

.range-control strong {
  color: #172033;
  font-size: 12px;
  text-align: right;
}

.range-control input[type="range"] {
  width: 100%;
  accent-color: #14b8a6;
}

.select-control {
  grid-template-columns: 74px minmax(0, 1fr);
}

.select-control select {
  min-width: 0;
  height: 34px;
  padding: 0 10px;
  color: #172033;
  background: #f8fafc;
  border: 1px solid rgba(148, 163, 184, 0.34);
  border-radius: 9px;
  font: inherit;
  font-weight: 700;
}

.color-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.color-grid label {
  min-width: 0;
  display: grid;
  gap: 5px;
  color: #64748b;
  font-size: 12px;
  font-weight: 700;
}

.color-grid input[type="color"] {
  width: 100%;
  height: 32px;
  padding: 3px;
  background: #f8fafc;
  border: 1px solid rgba(148, 163, 184, 0.34);
  border-radius: 9px;
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

  .content-grid {
    height: auto;
  }

  .main-column {
    grid-template-rows: minmax(220px, auto) 150px;
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

  .context-line,
  .lyric-footer span {
    white-space: normal;
  }

  .log-entry {
    grid-template-columns: 1fr;
    gap: 3px;
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
    0 2px 3px var(--floating-shadow-color),
    0 0 2px var(--floating-shadow-color),
    1px 0 0 var(--floating-shadow-color),
    -1px 0 0 var(--floating-shadow-color),
    0 1px 0 var(--floating-shadow-color),
    0 -1px 0 var(--floating-shadow-color);
}

.floating-current {
  position: relative;
  color: var(--floating-current-color);
  font-size: var(--floating-current-size);
  font-weight: var(--floating-font-weight);
  line-height: 1.18;
}

.floating-next {
  color: var(--floating-next-color);
  font-size: var(--floating-next-size);
  font-weight: var(--floating-next-font-weight);
  line-height: 1.25;
}
</style>
