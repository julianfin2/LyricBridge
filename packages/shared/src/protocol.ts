export const BRIDGE_PROTOCOL_VERSION = 1;

export type PlaybackSource = "youtube";

export type PlayerState = {
  protocolVersion: typeof BRIDGE_PROTOCOL_VERSION;
  source: PlaybackSource;
  url: string;
  videoId: string | null;
  title: string | null;
  currentTime: number;
  duration: number | null;
  paused: boolean;
  playbackRate: number;
  observedAt: number;
};

export type BridgeMessage =
  | {
      type: "player-state";
      payload: PlayerState;
    }
  | {
      type: "page-hidden";
      payload: {
        source: PlaybackSource;
        observedAt: number;
      };
    };
