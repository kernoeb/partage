import { notify } from '@kyvg/vue3-notification'

interface Room {
  id: string
  users: string[]
}

const { isFetching, error, data: rooms, execute: fetch } = useFetch('/rooms', { immediate: false })
  .json<Room[]>()

const defaultRoom = 'channel-1'

consola.info('[FETCH] Use rooms')
fetch().catch(console.error)

export function useRooms() {
  const router = useRouter()

  async function removeRoom(id: string) {
    try {
      await ofetch(`/rooms/${id}`, { method: 'DELETE' })
    } catch (err) {
      const text = err && typeof err === 'object'
        && 'data' in err && typeof err.data === 'object' && err.data
        && 'error' in err.data && typeof err.data.error === 'string'
        ? err.data.error
        : 'Could not delete room'
      notify({ title: 'Error', text, type: 'error' })
      throw err
    }

    consola.info('[FETCH] Room removed')
    await fetch()

    // Redirect to first room
    if (rooms.value && rooms.value.length) {
      router.push(`/c/${rooms.value[0].id}`)
    }
  }

  function redirectToDefaultRoom(rooms: Room[]) {
    const id = (!rooms.length || rooms.some(room => room.id === defaultRoom))
      ? defaultRoom
      : rooms[0].id

    router.push({ name: '/c/[id]', params: { id } })
  }

  return {
    defaultRoom,
    redirectToDefaultRoom,
    rooms,
    fetch,
    removeRoom,
    isFetching,
    error,
  }
}
