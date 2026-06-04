# LyricBridge Extension

LyricBridge Extension 是 LyricBridge 的浏览器扩展，面向 Chromium 浏览器。

它会在 YouTube 页面中读取播放进度、视频 ID 和播放状态，并发送给 LyricBridge 桌面端。

## 主要功能

- 监听 YouTube 播放状态
- 发送播放进度到桌面端
- 显示扩展连接状态

## 开发

```bash
pnpm dev:extension
```

## 构建

```bash
pnpm build:extension
```

构建后在浏览器中加载 `apps/extension/dist`。
