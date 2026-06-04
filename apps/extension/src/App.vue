<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { EXTENSION_STATUS_STORAGE_KEY, type ExtensionPlaybackStatus } from "./status";

const bridgeAddress = "ws://127.0.0.1:32190";
const status = ref<ExtensionPlaybackStatus | null>(null);
const now = ref(Date.now());
let clockTimer: number | null = null;

const statusIsStale = computed(() => {
  if (!status.value) {
    return true;
  }

  return now.value - status.value.updatedAt > 5_000;
});

const statusLabel = computed(() => {
  const currentStatus = status.value;

  if (statusIsStale.value || !currentStatus) {
    return "等待 YouTube 播放状态";
  }

  if (!currentStatus.hasVideo) {
    return "等待 YouTube 播放";
  }

  if (!currentStatus.bridgeConnected) {
    return "桌面端未连接";
  }

  return currentStatus.paused ? "已暂停" : "正在同步";
});

const statusKind = computed(() => {
  if (statusIsStale.value || !status.value?.hasVideo) {
    return "idle";
  }

  if (!status.value.bridgeConnected) {
    return "warn";
  }

  return status.value.paused ? "paused" : "ok";
});

const titleLabel = computed(() => {
  if (statusIsStale.value) {
    return "未检测到活动页面";
  }

  return status.value?.title || status.value?.videoId || "未命名视频";
});

const progressLabel = computed(() => {
  if (statusIsStale.value || !status.value?.hasVideo) {
    return "--:-- / --:--";
  }

  return `${formatTime(status.value.currentTime)} / ${formatTime(status.value.duration)}`;
});

const videoIdLabel = computed(() => {
  if (statusIsStale.value) {
    return "未检测到";
  }

  return status.value?.videoId || "无视频 ID";
});

onMounted(() => {
  clockTimer = window.setInterval(() => {
    now.value = Date.now();
  }, 1_000);

  chrome.storage.session.get(EXTENSION_STATUS_STORAGE_KEY).then((items) => {
    status.value = (items[EXTENSION_STATUS_STORAGE_KEY] as ExtensionPlaybackStatus | undefined) ?? null;
  });

  chrome.storage.onChanged.addListener(handleStorageChange);
});

onUnmounted(() => {
  if (clockTimer !== null) {
    window.clearInterval(clockTimer);
    clockTimer = null;
  }

  chrome.storage.onChanged.removeListener(handleStorageChange);
});

function handleStorageChange(changes: Record<string, chrome.storage.StorageChange>, areaName: string) {
  if (areaName !== "session" || !changes[EXTENSION_STATUS_STORAGE_KEY]) {
    return;
  }

  status.value =
    (changes[EXTENSION_STATUS_STORAGE_KEY].newValue as ExtensionPlaybackStatus | undefined) ?? null;
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
  <main>
    <header>
      <img src="/icons/48x48.png" alt="" />
      <div>
        <h1>LyricBridge</h1>
        <p>YouTube 歌词同步</p>
      </div>
    </header>

    <section class="notice" :class="statusKind">
      <span class="pulse" />
      <div>
        <strong>{{ statusLabel }}</strong>
        <p>{{ titleLabel }}</p>
      </div>
    </section>

    <dl>
      <div>
        <dt>播放进度</dt>
        <dd>{{ progressLabel }}</dd>
      </div>
      <div>
        <dt>视频 ID</dt>
        <dd>{{ videoIdLabel }}</dd>
      </div>
      <div>
        <dt>桥接地址</dt>
        <dd>{{ bridgeAddress }}</dd>
      </div>
      <!-- <div>
        <dt>作用范围</dt>
        <dd>仅 YouTube 页面</dd>
      </div> -->
    </dl>
  </main>
</template>
