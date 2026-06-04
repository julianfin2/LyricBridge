import {
  BRIDGE_PROTOCOL_VERSION,
  parseYouTubeVideoId,
  type BridgeMessage,
  type PlayerState
} from "@lyricbridge/shared";

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
  socket.addEventListener("open", sendPlayerState);
  socket.addEventListener("close", scheduleReconnect);
  socket.addEventListener("error", scheduleReconnect);
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

function readTitle(): string | null {
  const title = document.querySelector("h1 yt-formatted-string");
  const text = title?.textContent?.trim() || document.title.replace(/ - YouTube$/, "").trim();
  return text || null;
}
