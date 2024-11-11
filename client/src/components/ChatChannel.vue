<script setup lang="ts">
import type { SocketMessage } from '@/bindings/SocketMessage'
import type { VTextarea } from 'vuetify/components'
import { username } from '@/utils/user'
import { notify } from '@kyvg/vue3-notification'

const props = defineProps({
  channelId: {
    type: String,
    required: true,
  },
})

const { fetch: fetchRooms, rooms } = useRooms()

const currentRoom = computed(() => {
  return rooms.value?.find(room => room.id === props.channelId)
})

const editor = useTemplateRef<VTextarea | null>('editor')
const content = ref<string | null>(null)

const { status, data, send, open } = useWebSocket('/ws', {
  autoReconnect: true,
  heartbeat: false,
  immediate: false,
  onMessage: (_, { data: msg }) => {
    if (msg) {
      if (msg === 'pong') {
        return
      }

      try {
        const { type, username: msgUsername, value } = JSON.parse(msg) as SocketMessage
        if (type === 'error') {
          console.error('Error', value)
          notify({ type: 'error', title: 'Error', text: value })
        } else if (type === 'join') {
          if (!msgUsername) {
            console.error('Invalid join message', msg)
            return
          }
          console.log(`User ${msgUsername} joined`)
          if (msgUsername !== username) {
            consola.info('[FETCH] Join', msgUsername, username)
            fetchRooms()
          } else {
            console.log('Ignoring own join')

            if (rooms.value) {
              const room = rooms.value.find(r => r.id === props.channelId)
              if (!room) {
                console.log('Room not found')
                fetchRooms()
              } else {
                console.log('Room found', room)
                // Avoid refreshing the rooms, just update the users
                rooms.value = rooms.value.map((r) => {
                  if (r.id === props.channelId) {
                    return {
                      ...r,
                      users: r.users.includes(username)
                        ? r.users
                        : [...r.users, username],
                    }
                  }
                  return {
                    ...r,
                    users: r.users.filter(u => u !== username),
                  }
                })
              }
            }
          }
        } else if (type === 'leave') {
          if (!msgUsername) {
            console.error('Invalid leave message', msg)
            return
          }
          console.log(`User ${msgUsername} left`)
          consola.info('[FETCH] Leave')
          fetchRooms()
        } else if (type === 'update-rooms-list') {
          console.log('Rooms updated')
          consola.info('[FETCH] Update rooms')
          fetchRooms()
        } else if (value != null) {
          if (!msgUsername) {
            console.error('Invalid message', msg)
            return
          }
          console.log(`User ${msgUsername} sent: ${value}`, content.value)
          if (content.value !== value) {
            content.value = value
          } else {
            console.log('Ignoring own message')
          }
        }
      } catch (err) {
        console.error('Invalid JSON', err, data.value)
      }
    }
  },
  onConnected: (data) => {
    if (data.OPEN) {
      console.log('Connected')
      const channelId = props.channelId
      console.log('Sending username', username, channelId)
      send(JSON.stringify({ channel: channelId, username }))
    }
  },
  onError: (err) => {
    consola.error('Error', err)
  },
})

const canWrite = computed(() => status.value === 'OPEN' && content.value !== null)

whenever(canWrite, () => {
  tryOnMounted(() => {
    console.log('Focus')
    editor.value?.focus()
  }, false)
})

const channelId = computed(() => props.channelId)
watch(channelId, (cId, oldCId) => {
  if (!cId) {
    console.warn('No channel ID')
    return
  }
  console.log('Channel ID changed', oldCId, '->', cId)
  content.value = null // Reset the content
  open() // Reconnect
}, { immediate: true })
</script>

<template>
  <v-container class="fill-height">
    <div class="d-flex flex-column w-100">
      <div class="mb-3">
        <h3>Start typing!<span v-if="currentRoom" class="ml-2 text-caption">#{{ currentRoom.id }}</span></h3>
      </div>
      <v-textarea
        id="editor"
        ref="editor"
        v-model="content"
        :loading="status === 'CONNECTING'"
        :disabled="!canWrite"
        no-resize
        class="w-100"
        :placeholder="content === null ? '' : 'Type your message here...'"
        variant="filled"
        max-rows="40"
        hide-details
        @update:model-value="send($event)"
      />
    </div>

    <div v-if="currentRoom?.users?.length">
      <v-chip-group :model-value="[username]">
        <v-chip v-for="user in currentRoom.users" :key="user" color="primary" label :value="user">
          {{ user }}
        </v-chip>
      </v-chip-group>
    </div>
    <div v-else style="height: 48px">
      ...
    </div>
  </v-container>
</template>

<style>
#editor  {
  font-size: 1rem;
  line-height: 1.5;
  font-family: 'Fira Code', monospace;
  height: calc(100vh - 220px);
}
</style>
