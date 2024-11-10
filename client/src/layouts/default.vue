<script lang="ts" setup>
import slug from 'slug'

const { rooms, fetch, removeRoom } = useRooms()
const { username } = useUser()

const router = useRouter()
const route = useRoute()

const drawer = ref<boolean | null>(null)

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

    <v-navigation-drawer v-model="drawer" color="#f5f5f5" floating>
      <div class="d-flex align-center w-100 pt-2 px-2 mb-3">
        <v-btn
          color="grey-darken-2"
          variant="text"
          :icon="showSearch ? '$close' : '$magnify'"
          @click="search = ''; showSearch = !showSearch"
        />
        <template v-if="showSearch">
          <v-text-field
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
          <v-btn color="grey-darken-2" variant="text" icon="$dots-vertical" />
        </template>
      </div>
      <v-list-subheader style="min-height: 24px">
        <span class="text-caption pl-4 text-grey-darken-4">Channels</span>
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
      <template #append>
        <div class="pa-2">
          <v-dialog v-model="dialog" max-width="290">
            <template #activator="{ props }">
              <v-btn block variant="tonal" color="primary" v-bind="props">
                New channel
              </v-btn>
            </template>
            <v-card>
              <v-card-title class="text-h6">
                New channel
              </v-card-title>
              <v-card-text class="py-0 px-4">
                <v-text-field
                  v-model="roomTitle"
                  autofocus
                  placeholder="Room title"
                  outlined
                  variant="underlined"
                  dense
                  class="mt-n4"
                  clearable
                  :rules="[v => !!v || 'Title is required', v => (v && v.length >= 2) || 'Title must be at least 2 characters']"
                  @keydown.enter="createRoom(roomTitle)"
                />
              </v-card-text>
              <v-card-actions>
                <v-btn variant="text" @click="dialog = false">
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
        </div>
      </template>
    </v-navigation-drawer>

    <v-app-bar order="1" color="white" elevation="0">
      <template #prepend>
        <v-app-bar-nav-icon
          color="grey-darken-2"
          @click.stop="drawer = !drawer"
        />
      </template>
      <div class="d-flex justify-between align-center w-100 pl-2 pr-6">
        <div class="text-h6 text-grey-darken-2">
          Partage
        </div>
        <v-spacer />
        <v-btn color="grey-darken-2" variant="text" icon="$magnify" />
        <v-btn color="grey-darken-2" variant="text" icon="$dots-vertical" />
      </div>
    </v-app-bar>

    <v-main>
      <router-view />
    </v-main>

    <AppFooter />
  </v-app>
</template>
