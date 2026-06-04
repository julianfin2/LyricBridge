export const EXTENSION_STATUS_STORAGE_KEY = "lyricbridgeStatus";

export type ExtensionPlaybackStatus = {
  bridgeConnected: boolean;
  currentTime: number;
  duration: number | null;
  hasVideo: boolean;
  isYouTubePage: boolean;
  paused: boolean;
  playbackRate: number;
  tabId: number | null;
  title: string | null;
  updatedAt: number;
  url: string;
  videoId: string | null;
};
