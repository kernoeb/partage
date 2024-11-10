/**
 * plugins/vuetify.ts
 *
 * Framework documentation: https://vuetifyjs.com`
 */

// Styles
import { mdiClose, mdiDotsVertical, mdiMagnify, mdiPencil, mdiTrashCan } from '@mdi/js'
import { aliases, mdi } from 'vuetify/iconsets/mdi-svg'
import 'vuetify/styles'

// Composables
import { createVuetify } from 'vuetify'

// https://vuetifyjs.com/en/introduction/why-vuetify/#feature-guides
export default createVuetify({
  icons: {
    defaultSet: 'mdi',
    sets: {
      mdi,
    },
    aliases: {
      ...aliases,
      'close': mdiClose,
      'magnify': mdiMagnify,
      'dots-vertical': mdiDotsVertical,
      'pencil': mdiPencil,
      'trash-can': mdiTrashCan,
    },
  },
  theme: {
    defaultTheme: 'light',
  },
})
