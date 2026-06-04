# LyricBridge

LyricBridge 是一个 YouTube 桌面歌词工具。

它由桌面端和浏览器扩展组成：扩展读取 YouTube 的播放进度，桌面端根据视频 ID 匹配歌词，并在桌面歌词栏中同步显示。

## 项目组成

- `apps/desktop`：Tauri + Vue 3 桌面端
- `apps/extension`：Chromium 浏览器扩展
- `packages/shared`：共享解析和协议代码

## 开发

```bash
pnpm install
pnpm dev:desktop
pnpm build:extension
```

## 构建

```bash
pnpm build
pnpm build:desktop
```

当前主要支持 Windows，暂不适配 macOS。
