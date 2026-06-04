import {
  BRIDGE_PROTOCOL_VERSION,
  parseYouTubeVideoId,
  type BridgeMessage,
  type PlayerState
} from "@lyricbridge/shared";
import type { ExtensionPlaybackStatus } from "../status";

const BRIDGE_URL = "ws://127.0.0.1:32190";
const HEARTBEAT_MS = 250;
const RECONNECT_MS = 1_500;
const YOUTUBE_WATCH_HOSTS = new Set(["www.youtube.com", "youtube.com", "music.youtube.com"]);

type ActiveTabResponse = {
  active: boolean;
  tabId: number | null;
};

let socket: WebSocket | null = null;
let reconnectTimer: number | null = null;
let heartbeatTimer: number | null = null;
let observedVideo: HTMLVideoElement | null = null;
let lastUrl = location.href;

reconcileBridgeConnection();
startHeartbeat();
observeUrlChanges();

function connect() {
  if (!isYouTubeWatchPage()) {
    return;
  }

  if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
    return;
  }

  socket = new WebSocket(BRIDGE_URL);
  socket.addEventListener("open", () => {
    publishStatus();
    sendPlayerState();
  });
  socket.addEventListener("close", () => {
    socket = null;
    publishStatus();
    if (isYouTubeWatchPage()) {
      scheduleReconnect();
    }
  });
  socket.addEventListener("error", () => {
    publishStatus();
    if (isYouTubeWatchPage()) {
      scheduleReconnect();
    }
  });
}

function scheduleReconnect() {
  if (reconnectTimer !== null) {
    return;
  }

  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null;
    reconcileBridgeConnection();
  }, RECONNECT_MS);
}

function startHeartbeat() {
  if (heartbeatTimer !== null) {
    return;
  }

  heartbeatTimer = window.setInterval(() => {
    reconcileBridgeConnection();
    bindCurrentVideo();
    publishStatus();
    sendPlayerState();
  }, HEARTBEAT_MS);
}

function bindCurrentVideo() {
  const video = document.querySelector("video");

  if (!(video instanceof HTMLVideoElement) || video === observedVideo) {
    return;
  }

  observedVideo = video;
  video.addEventListener("play", sendPlayerState);
  video.addEventListener("pause", sendPlayerState);
  video.addEventListener("seeked", sendPlayerState);
  video.addEventListener("ratechange", sendPlayerState);
  video.addEventListener("durationchange", sendPlayerState);
}

function observeUrlChanges() {
  window.setInterval(() => {
    if (location.href === lastUrl) {
      return;
    }

    lastUrl = location.href;
    observedVideo = null;
    reconcileBridgeConnection();
    bindCurrentVideo();
    publishStatus();
    sendPlayerState();
  }, 500);
}

function reconcileBridgeConnection() {
  if (isYouTubeWatchPage()) {
    connect();
    return;
  }

  if (reconnectTimer !== null) {
    window.clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }

  if (socket && socket.readyState !== WebSocket.CLOSED && socket.readyState !== WebSocket.CLOSING) {
    socket.close();
  }
  socket = null;
}

async function sendPlayerState() {
  if (!isYouTubeWatchPage()) {
    return;
  }

  if (!socket || socket.readyState !== WebSocket.OPEN) {
    return;
  }

  bindCurrentVideo();

  if (!observedVideo || !parseYouTubeVideoId(location.href)) {
    return;
  }

  const message: BridgeMessage = {
    type: "player-state",
    payload: createPlayerState(observedVideo, await getActiveTabState())
  };

  socket.send(JSON.stringify(message));
}

function publishStatus() {
  bindCurrentVideo();

  const isWatchPage = isYouTubeWatchPage();
  const status: ExtensionPlaybackStatus = {
    ...createStatusBase(isWatchPage ? observedVideo : null),
    bridgeConnected: socket?.readyState === WebSocket.OPEN,
    tabId: null,
    updatedAt: Date.now()
  };

  chrome.runtime.sendMessage({ type: "lyricbridge-status", payload: status }).catch(() => {
    // The popup/background can be unavailable during extension reloads.
  });
}

function createPlayerState(video: HTMLVideoElement, activeTabState: ActiveTabResponse | null): PlayerState {
  return {
    protocolVersion: BRIDGE_PROTOCOL_VERSION,
    source: "youtube",
    url: location.href,
    videoId: parseYouTubeVideoId(location.href),
    title: readTitle(),
    currentTime: video.currentTime,
    duration: Number.isFinite(video.duration) ? video.duration : null,
    paused: video.paused,
    playbackRate: video.playbackRate,
    sourceTabActive: activeTabState?.active,
    sourceTabId: activeTabState?.tabId ?? null,
    observedAt: Date.now()
  };
}

function createStatusBase(video: HTMLVideoElement | null) {
  const isWatchPage = isYouTubeWatchPage();

  return {
    currentTime: video?.currentTime ?? 0,
    duration: video && Number.isFinite(video.duration) ? video.duration : null,
    hasVideo: Boolean(video),
    isYouTubePage: isWatchPage,
    paused: video?.paused ?? true,
    playbackRate: video?.playbackRate ?? 1,
    title: video ? readTitle() : null,
    url: location.href,
    videoId: isWatchPage ? parseYouTubeVideoId(location.href) : null
  };
}

function isYouTubeWatchPage(): boolean {
  return YOUTUBE_WATCH_HOSTS.has(location.hostname) && location.pathname === "/watch" && Boolean(parseYouTubeVideoId(location.href));
}

async function getActiveTabState(): Promise<ActiveTabResponse | null> {
  try {
    const response = await chrome.runtime.sendMessage({ type: "lyricbridge-active-tab-request" });

    if (!response || typeof response.active !== "boolean") {
      return null;
    }

    return {
      active: response.active,
      tabId: typeof response.tabId === "number" ? response.tabId : null
    };
  } catch {
    return null;
  }
}

function readTitle(): string | null {
  const title = document.querySelector("h1 yt-formatted-string");
  const text = title?.textContent?.trim() || document.title.replace(/ - YouTube$/, "").trim();
  return text || null;
}
