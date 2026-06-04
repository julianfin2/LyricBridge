# LyricBridge Protocol

LyricBridge uses a local WebSocket connection from the browser extension to the
desktop app.

## Transport

- URL: `ws://127.0.0.1:32190`
- Producer: LyricBridge browser extension content script
- Consumer: LyricBridge Desktop
- Encoding: JSON text frames

The extension sends frequent state snapshots while a YouTube page contains a
`<video>` element. The desktop app should treat snapshots as clock corrections
and render lyrics from its own local timer between snapshots.

## Message: `player-state`

```json
{
  "type": "player-state",
  "payload": {
    "protocolVersion": 1,
    "source": "youtube",
    "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
    "videoId": "dQw4w9WgXcQ",
    "title": "Song title",
    "currentTime": 42.25,
    "duration": 213.5,
    "paused": false,
    "playbackRate": 1,
    "observedAt": 1770000000000
  }
}
```

## Desktop Binding Model

The first persistent binding table should map a YouTube video ID to a local LRC
file and per-video timing offset.

```text
video_id -> lyric_file_path -> offset_ms
```

`offset_ms` is required because YouTube uploads often contain intros, silence,
or MV-only timing that does not match a normal LRC file exactly.

## Config Directory

LyricBridge can load bindings from a user-selected config directory. The
directory must contain `bindings.json`.

```json
{
  "bindings": [
    {
      "videoId": "dQw4w9WgXcQ",
      "lyricFile": "lyrics/song.lrc",
      "offsetMs": 0
    }
  ]
}
```

`lyricFile` may be an absolute path or a relative path. Relative paths are
resolved from the selected config directory.

Config directory bindings take priority over the app's manual local bindings.
