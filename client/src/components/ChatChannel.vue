<script setup lang="ts">
import type { SocketMessage } from '@/bindings/SocketMessage'
import type { VTextarea } from 'vuetify/components'
import { username, usernameInitials } from '@/utils/user'
import { notify } from '@kyvg/vue3-notification'
import { useTheme } from 'vuetify'

const props = defineProps({
  channelId: {
    type: String,
    required: true,
  },
})

const theme = useTheme()

const { fetch: fetchRooms, rooms } = useRooms()

const currentRoom = computed(() => {
  return rooms.value?.find(room => room.id === props.channelId)
})

const currentRoomUsersWithMeFirst = computed(() => {
  const room = currentRoom.value
  if (!room || !room.users || !room.users.length) return

  const users = room.users
  const meIndex = users.indexOf(username)
  if (meIndex === -1) {
    return users
  }
  return [username, ...users.slice(0, meIndex), ...users.slice(meIndex + 1)]
})

const editor = useTemplateRef<VTextarea | null>('editor')
const content = ref<string | null>(null)

const pingFrame = new Uint8Array([0x9]) // Ping frame
const pongFrame = new Uint8Array([0xA]) // Pong frame

const { status, data, send, open } = useWebSocket('/ws', {
  autoReconnect: true,
  heartbeat: {
    interval: 5000,
    message: pingFrame.buffer,
    responseMessage: pongFrame.buffer,
  },
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

    <div v-if="currentRoomUsersWithMeFirst">
      <v-avatar
        v-for="user in currentRoomUsersWithMeFirst"
        :key="user" rounded="lg"
        size="28"
        class="mr-2"
        :style="{
          outline: user === username
            ? (theme.global.name.value === 'dark' ? '1px solid grey' : 'none')
            : 'none',
          opacity: user === username ? 1 : 0.7,
        }"
        :color="user === username
          ? 'black'
          : (theme.global.name.value === 'dark' ? 'grey-darken-3' : 'grey-lighten-3')
        "
      >
        <v-tooltip location="top">
          <template #activator="{ props: propsTooltip }">
            <span v-bind="propsTooltip" class="text-caption">{{ usernameInitials(user) }}</span>
          </template>
          <span>{{ user }}</span>
        </v-tooltip>
      </v-avatar>
    </div>
    <div v-else style="height: 28px">
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
