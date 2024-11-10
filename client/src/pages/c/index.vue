<script lang="ts" setup>
const { rooms, redirectToDefaultRoom } = useRooms()

whenever(rooms, (newRooms) => {
  console.log('Redirecting to default room')
  redirectToDefaultRoom(newRooms)
}, { immediate: true })
</script>
