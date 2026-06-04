import { EXTENSION_STATUS_STORAGE_KEY, type ExtensionPlaybackStatus } from "../status";

type StatusMessage = {
  payload: ExtensionPlaybackStatus;
  type: "lyricbridge-status";
};

type ActiveTabMessage = {
  type: "lyricbridge-active-tab-request";
};

type RuntimeMessage = ActiveTabMessage | StatusMessage;

let activeTabId: number | null = null;

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
    tabId: sender.tab?.id ?? null,
    updatedAt: Date.now()
  };

  chrome.storage.session.set({
    [EXTENSION_STATUS_STORAGE_KEY]: status
  });
});
