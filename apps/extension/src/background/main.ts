import { EXTENSION_STATUS_STORAGE_KEY, type ExtensionPlaybackStatus } from "../status";

type StatusMessage = {
  payload: ExtensionPlaybackStatus;
  type: "lyricbridge-status";
};

chrome.runtime.onInstalled.addListener(() => {
  console.info("LyricBridge extension installed");
});

chrome.runtime.onMessage.addListener((message: StatusMessage, sender) => {
  if (message?.type !== "lyricbridge-status") {
    return;
  }

  const status: ExtensionPlaybackStatus = {
    ...message.payload,
    tabId: sender.tab?.id ?? null,
    updatedAt: Date.now()
  };

  chrome.storage.session.set({
    [EXTENSION_STATUS_STORAGE_KEY]: status
  });
});
