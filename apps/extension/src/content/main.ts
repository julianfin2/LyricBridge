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

let socket: WebSocket | null = null;
let reconnectTimer: number | null = null;
let heartbeatTimer: number | null = null;
let observedVideo: HTMLVideoElement | null = null;
let lastUrl = location.href;

connect();
startHeartbeat();
observeUrlChanges();

function connect() {
  if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
    return;
  }

  socket = new WebSocket(BRIDGE_URL);
  socket.addEventListener("open", () => {
    publishStatus();
    sendPlayerState();
  });
  socket.addEventListener("close", () => {
    publishStatus();
    scheduleReconnect();
  });
  socket.addEventListener("error", () => {
    publishStatus();
    scheduleReconnect();
  });
}

function scheduleReconnect() {
  socket = null;

  if (reconnectTimer !== null) {
    return;
  }

  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, RECONNECT_MS);
}

function startHeartbeat() {
  if (heartbeatTimer !== null) {
    return;
  }

  heartbeatTimer = window.setInterval(() => {
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
    bindCurrentVideo();
    publishStatus();
    sendPlayerState();
  }, 500);
}

function sendPlayerState() {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    return;
  }

  bindCurrentVideo();

  if (!observedVideo) {
    return;
  }

  const message: BridgeMessage = {
    type: "player-state",
    payload: createPlayerState(observedVideo)
  };

  socket.send(JSON.stringify(message));
}

function publishStatus() {
  bindCurrentVideo();

  const status: ExtensionPlaybackStatus = {
    ...createStatusBase(observedVideo),
    bridgeConnected: socket?.readyState === WebSocket.OPEN,
    tabId: null,
    updatedAt: Date.now()
  };

  chrome.runtime.sendMessage({ type: "lyricbridge-status", payload: status }).catch(() => {
    // The popup/background can be unavailable during extension reloads.
  });
}

function createPlayerState(video: HTMLVideoElement): PlayerState {
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
    observedAt: Date.now()
  };
}

function createStatusBase(video: HTMLVideoElement | null) {
  return {
    currentTime: video?.currentTime ?? 0,
    duration: video && Number.isFinite(video.duration) ? video.duration : null,
    hasVideo: Boolean(video),
    isYouTubePage: location.hostname.endsWith("youtube.com"),
    paused: video?.paused ?? true,
    playbackRate: video?.playbackRate ?? 1,
    title: video ? readTitle() : null,
    url: location.href,
    videoId: parseYouTubeVideoId(location.href)
  };
}

function readTitle(): string | null {
  const title = document.querySelector("h1 yt-formatted-string");
  const text = title?.textContent?.trim() || document.title.replace(/ - YouTube$/, "").trim();
  return text || null;
}
