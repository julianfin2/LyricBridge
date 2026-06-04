<script setup lang="ts">
const bridgeAddress = "ws://127.0.0.1:32190";
</script>

<template>
  <main>
    <h1>LyricBridge</h1>
    <p>打开 YouTube 视频，并保持 LyricBridge 桌面端运行。</p>
    <dl>
      <div>
        <dt>桥接地址</dt>
        <dd>{{ bridgeAddress }}</dd>
      </div>
      <div>
        <dt>作用范围</dt>
        <dd>仅 YouTube 页面</dd>
      </div>
    </dl>
  </main>
</template>
