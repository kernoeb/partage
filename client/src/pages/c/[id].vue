<script setup lang="ts">
const route = useRoute()

const channelId = computed(() => {
  return 'id' in route.params && route.params.id
})
</script>

<template>
  <ChatChannel v-if="channelId" :channel-id="channelId" />
</template>
