/**
 * plugins/vuetify.ts
 *
 * Framework documentation: https://vuetifyjs.com`
 */

// Styles
import { mdiClose, mdiDotsVertical, mdiForumPlusOutline, mdiMagnify, mdiPencil, mdiRefresh, mdiThemeLightDark, mdiTrashCan } from '@mdi/js'
import { aliases, mdi } from 'vuetify/iconsets/mdi-svg'
import 'vuetify/styles'
// Composables
import { createVuetify, type ThemeDefinition } from 'vuetify'

// Utils
import { getSavedTheme } from '@/utils/theme'

const lightTheme: ThemeDefinition = {
  dark: false,
  colors: {
    primary: '#1976D2',
    secondary: '#616161',
    accent: '#82B1FF',
    error: '#FF5252',
    info: '#2196F3',
    success: '#4CAF50',
    warning: '#FFC107',
    background: '#ffffff',
    surface: '#f5f5f5',
  },
}

const darkTheme: ThemeDefinition = {
  dark: true,
  colors: {
    primary: '#1976D2',
    secondary: '#BDBDBD',
    accent: '#82B1FF',
    error: '#FF5252',
    info: '#2196F3',
    success: '#4CAF50',
    warning: '#FFC107',
    background: '#212121',
    surface: '#171717',
  },
}

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
      'forum-plus-outline': mdiForumPlusOutline,
      'theme-light-dark': mdiThemeLightDark,
      'refresh': mdiRefresh,
    },
  },
  defaults: {
    VBtn: {
      rounded: 'lg',
    },
  },
  theme: {
    defaultTheme: getSavedTheme(),
    themes: {
      dark: darkTheme,
      light: lightTheme,
    },
  },
})
