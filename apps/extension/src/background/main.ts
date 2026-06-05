import { EXTENSION_STATUS_STORAGE_KEY, type ExtensionPlaybackStatus } from "../status";

type StatusMessage = {
  payload: ExtensionPlaybackStatus;
  type: "lyricbridge-status";
};

type ActiveTabMessage = {
  type: "lyricbridge-active-tab-request";
};

type RuntimeMessage = ActiveTabMessage | StatusMessage;

const STATUS_STALE_MS = 5_000;
let activeTabId: number | null = null;
let currentStatus: ExtensionPlaybackStatus | null = null;

chrome.runtime.onInstalled.addListener(() => {
  console.info("LyricBridge extension installed");
});

chrome.tabs.onActivated.addListener((activeInfo) => {
  activeTabId = activeInfo.tabId;
});

chrome.runtime.onMessage.addListener((message: RuntimeMessage, sender, sendResponse) => {
  if (message?.type === "lyricbridge-active-tab-request") {
    const tabId = sender.tab?.id ?? null;
    const active = sender.tab?.active === true || (tabId !== null && tabId === activeTabId);

    sendResponse({
      active,
      tabId
    });
    return;
  }

  if (message?.type !== "lyricbridge-status") {
    return;
  }

  const status: ExtensionPlaybackStatus = {
    ...message.payload,
    sourceTabActive: sender.tab?.active === true || (sender.tab?.id !== undefined && sender.tab.id === activeTabId),
    tabId: sender.tab?.id ?? null,
    updatedAt: Date.now()
  };

  if (!shouldAcceptStatus(status, currentStatus)) {
    return;
  }

  currentStatus = status;
  chrome.storage.session.set({
    [EXTENSION_STATUS_STORAGE_KEY]: status
  });
});

function shouldAcceptStatus(
  nextStatus: ExtensionPlaybackStatus,
  previousStatus: ExtensionPlaybackStatus | null
): boolean {
  if (!previousStatus || isStatusStale(previousStatus)) {
    return true;
  }

  if (!nextStatus.isYouTubePage) {
    return !previousStatus.isYouTubePage;
  }

  if (!previousStatus.isYouTubePage) {
    return true;
  }

  if (nextStatus.videoId !== previousStatus.videoId) {
    if (nextStatus.paused && !previousStatus.paused) {
      return false;
    }

    if (
      nextStatus.paused &&
      previousStatus.paused &&
      !shouldAcceptPausedStatusSwitch(nextStatus, previousStatus)
    ) {
      return false;
    }

    return true;
  }

  return true;
}

function shouldAcceptPausedStatusSwitch(
  nextStatus: ExtensionPlaybackStatus,
  previousStatus: ExtensionPlaybackStatus
): boolean {
  if (nextStatus.sourceTabActive === true) {
    return true;
  }

  if (previousStatus.sourceTabActive === true) {
    return false;
  }

  return false;
}

function isStatusStale(status: ExtensionPlaybackStatus): boolean {
  return Date.now() - status.updatedAt > STATUS_STALE_MS;
}
