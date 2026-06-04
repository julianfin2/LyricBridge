<script setup lang="ts">
const bridgeAddress = "ws://127.0.0.1:32190";
</script>

<template>
  <main>
    <h1>LyricBridge</h1>
    <p>Open a YouTube video and keep LyricBridge Desktop running.</p>
    <dl>
      <div>
        <dt>Bridge</dt>
        <dd>{{ bridgeAddress }}</dd>
      </div>
      <div>
        <dt>Scope</dt>
        <dd>YouTube pages only</dd>
      </div>
    </dl>
  </main>
</template>
