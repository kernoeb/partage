/**
 * plugins/index.ts
 *
 * Automatically included in `./src/main.ts`
 */

import Notifications from '@kyvg/vue3-notification'
import router from '../router'
// Plugins
import vuetify from './vuetify'

// Types
import type { App } from 'vue'

export function registerPlugins(app: App) {
  app
    .use(vuetify)
    .use(router)
    .use(Notifications)
}
