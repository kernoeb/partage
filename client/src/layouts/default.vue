<script lang="ts" setup>
import { isBrowserDark } from '@/utils/theme'
import { username, usernameInitials } from '@/utils/user'
import slug from 'slug'
import { useDisplay, useTheme } from 'vuetify'
import { VTextField } from 'vuetify/components'

const display = useDisplay()

const theme = useTheme()
const myTheme = useLocalStorage('theme', 'system') as Ref<'system' | 'light' | 'dark'>

watch(myTheme, (value: 'system' | 'light' | 'dark') => {
  if (value === 'system') {
    theme.global.name.value = isBrowserDark() ? 'dark' : 'light'
  } else {
    theme.global.name.value = value
  }
})

onMounted(() => {
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (event) => {
    console.log('theme change', event)
    if (myTheme.value === 'system') {
      theme.global.name.value = event.matches ? 'dark' : 'light'
    }
  })
})

function newSession() {
  localStorage.removeItem('username')
  location.reload()
}

const { rooms, fetch, removeRoom } = useRooms()

const router = useRouter()
const route = useRoute()

const drawer = useLocalStorage('drawer', null, {
  serializer: {
    read: (value) => {
      if (display.mobile.value) return false
      return value ? (value === 'true') : null
    },
    write: value => value != null ? (value ? 'true' : 'false') : '',
  },
})

const search = ref('')
const showSearch = ref(false)

const filteredRooms = computed(() => {
  if (!rooms.value) return
  if (!search.value) return rooms.value
  const normalizedSearch = search.value.trim().toLowerCase().normalize('NFD').replace(/[\u0300-\u036F]/g, '')
  return rooms.value.filter((room) => {
    const normalizedRoomId = room.id.trim().toLowerCase().normalize('NFD').replace(/[\u0300-\u036F]/g, '')
    return normalizedRoomId.includes(normalizedSearch)
  })
})

const dialog = ref(false)
const roomTitle = ref('')
const roomTitleTextField = useTemplateRef<VTextField | null>('roomTitleTextField')

async function createRoom(title: string) {
  console.log('create room', title)
  // Get last room id
  await fetch()

  const newRoomId = slug(title, { lower: true })
  router.push({ name: '/c/[id]', params: { id: newRoomId } })

  dialog.value = false
  roomTitle.value = ''
}
</script>

<template>
  <v-app>
    <notifications position="bottom center" class="ma-2">
      <template #body="props">
        <v-alert
          :text="props.item.text"
          :title="props.item.title"
          :type="(props.item.type as 'success' | 'info' | 'warning' | 'error')"
          closable
          @click:close="props.close"
        />
      </template>
    </notifications>

    <v-navigation-drawer
      v-model="drawer"
      color="surface"
      floating
    >
      <div class="d-flex align-center w-100 pt-1 px-2 mb-3">
        <v-btn
          color="secondary"
          variant="text"
          :icon="showSearch ? '$close' : '$magnify'"
          @click="search = ''; showSearch = !showSearch"
        />
        <template v-if="showSearch">
          <VTextField
            v-model="search"
            autofocus
            style="width: 100%;"
            outlined
            hide-details
            variant="underlined"
            density="compact"
            placeholder="Search"
            class="ml-2 mt-n2"
          />
        </template>
        <template v-else>
          <v-spacer />
          <v-dialog
            v-model="dialog" max-width="290"
            @after-enter="roomTitleTextField?.focus()"
          >
            <template #activator="{ props }">
              <v-btn
                color="secondary"
                variant="text"
                icon="$forum-plus-outline"
                v-bind="props"
              />
            </template>
            <v-card>
              <v-card-title class="text-h6">
                New channel
              </v-card-title>
              <v-card-text class="py-0 px-4">
                <VTextField
                  ref="roomTitleTextField"
                  v-model="roomTitle"
                  autofocus
                  placeholder="Room title"
                  outlined
                  variant="underlined"
                  dense
                  class="mt-n4"
                  :rules="[v => !!v || 'Title is required', v => (v && v.length >= 2) || 'Title must be at least 2 characters']"
                  @keydown.enter="createRoom(roomTitle)"
                />
              </v-card-text>
              <v-card-actions>
                <v-btn @click="dialog = false">
                  Cancel
                </v-btn>
                <v-btn
                  color="primary"
                  :disabled="!roomTitle || roomTitle.length < 2"
                  @click="createRoom(roomTitle)"
                >
                  Create
                </v-btn>
              </v-card-actions>
            </v-card>
          </v-dialog>
        </template>
      </div>
      <v-list-subheader style="min-height: 24px">
        <span
          class="text-caption pl-4"
        >
          Channels
        </span>
      </v-list-subheader>
      <template v-if="rooms">
        <v-list-item v-for="room in filteredRooms" :key="room.id" link :to="{ name: '/c/[id]', params: { id: room.id } }">
          <v-list-item-title>
            #{{ room.id }}
          </v-list-item-title>
          <v-list-item-subtitle>
            {{ room.users.length }} {{ room.users.length > 1 ? 'members' : 'member' }}
          </v-list-item-subtitle>
          <template v-if="'id' in route.params && room.id === route.params.id" #append>
            <v-menu offset-y>
              <template #activator="{ props }">
                <v-btn v-bind="props" icon="$dots-vertical" variant="text" size="small" @click.prevent />
              </template>
              <v-list>
                <v-list-item disabled density="compact">
                  <v-list-item-title>
                    <v-icon>$pencil</v-icon>
                    <span class="ml-2">Renommer</span>
                  </v-list-item-title>
                </v-list-item>
                <v-list-item
                  v-if="room.users.length === 1 && room.users.includes(username)"
                  density="compact"
                  @click="removeRoom(room.id)"
                >
                  <v-list-item-title>
                    <v-icon>$trash-can</v-icon>
                    <span class="ml-2">Supprimer</span>
                  </v-list-item-title>
                </v-list-item>
              </v-list>
            </v-menu>
          </template>
        </v-list-item>
      </template>
    </v-navigation-drawer>

    <v-app-bar
      order="1"
      height="56"
      color="background"
      elevation="0"
    >
      <template #prepend>
        <v-app-bar-nav-icon
          color="secondary"
          @click.stop="drawer = !drawer"
        />
      </template>
      <div class="d-flex justify-between align-center w-100 pl-2 pr-3">
        <div class="text-h6 text-secondary">
          <v-avatar size="25" class="mr-2" rounded="0">
            <v-img alt="Partage" src="/partage-black-round.webp" />
          </v-avatar>
          Partage
        </div>
        <v-spacer />
        <v-menu :close-on-content-click="false" offset-y width="250">
          <template #activator="{ props }">
            <v-btn icon v-bind="props">
              <v-avatar rounded="lg" size="small" color="black">
                <span class="text-caption">{{ usernameInitials(username) }}</span>
              </v-avatar>
            </v-btn>
          </template>
          <v-card
            variant="outlined"
            :style="{ borderColor: theme.global.name.value === 'dark' ? 'rgba(255, 255, 255, 0.12)' : 'rgba(0, 0, 0, 0.12)' }"
            elevation="0"
            rounded="lg"
          >
            <v-list elevation="0">
              <v-list-subheader style="min-height: 24px">
                <span class="text-caption">Welcome, {{ username }}</span>
              </v-list-subheader>
              <v-list-item>
                <v-radio-group
                  v-model="myTheme"
                  class="custom-radio-group"
                  hide-details
                  density="comfortable"
                  label="Theme"
                >
                  <v-radio label="Light" value="light" />
                  <v-radio label="Dark" value="dark" />
                  <v-radio label="System" value="system" />
                </v-radio-group>
              </v-list-item>
              <v-divider class="my-2" />
              <v-list-item class="mb-1" title="New session" subtitle="Change username" @click="newSession()">
                <template #prepend>
                  <v-icon icon="$refresh" />
                </template>
              </v-list-item>
            </v-list>
          </v-card>
        </v-menu>
      </div>
    </v-app-bar>

    <v-main>
      <router-view />
    </v-main>

    <AppFooter />
  </v-app>
</template>

<style>
.custom-switch .v-label {
  opacity: 1;
}

.custom-radio-group .v-label, .custom-radio-group .v-selection-control-group {
  margin-inline-start: 0!important;
  padding-inline-start: 0!important;
}
</style>
