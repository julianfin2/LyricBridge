<script setup lang="ts">
const bridgeAddress = "ws://127.0.0.1:32190";
</script>

<template>
  <main>
    <header>
      <img src="/icons/48x48.png" alt="" />
      <div>
        <h1>LyricBridge</h1>
        <p>YouTube 歌词同步</p>
      </div>
    </header>

    <section class="notice">
      <span class="pulse" />
      <p>等待 YouTube 播放状态</p>
    </section>

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
