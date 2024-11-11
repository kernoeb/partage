import fs from 'node:fs'
import { fileURLToPath, URL } from 'node:url'
import NotificationsResolver from '@kyvg/vue3-notification/auto-import-resolver'
import Vue from '@vitejs/plugin-vue'
// Plugins
import AutoImport from 'unplugin-auto-import/vite'
import Fonts from 'unplugin-fonts/vite'
import Components from 'unplugin-vue-components/vite'
import VueRouter from 'unplugin-vue-router/vite'
import Layouts from 'vite-plugin-vue-layouts'

import Vuetify, { transformAssetUrls } from 'vite-plugin-vuetify'
// Utilities
import { defineConfig } from 'vite'

try {
  // Make sure dist is created, for the backend
  fs.mkdirSync('./dist')
} catch {}

// https://vitejs.dev/config/
export default defineConfig({
  css: {
    preprocessorOptions: {
      scss: {
        api: 'modern',
      },
      sass: {
        api: 'modern',
      },
    },
  },
  plugins: [
    VueRouter({
      dts: 'src/typed-router.d.ts',
    }),
    Layouts(),
    AutoImport({
      imports: [
        'vue',
        '@vueuse/head',
        '@vueuse/core',
        {
          'vue-router/auto': ['useRoute', 'useRouter'],
        },
        {
          from: 'ofetch',
          imports: ['ofetch'],
        },
        {
          from: 'consola',
          imports: ['consola'],
        },
      ],
      dirs: [
        'src/composables',
        'src/stores',
      ],
      dts: 'src/auto-imports.d.ts',
      eslintrc: {
        enabled: true,
      },
      vueTemplate: true,
    }),
    Components({
      dts: 'src/components.d.ts',
      resolvers: [
        NotificationsResolver(),
      ],
    }),
    Vue({
      template: { transformAssetUrls },
    }),
    // https://github.com/vuetifyjs/vuetify-loader/tree/master/packages/vite-plugin#readme
    Vuetify({
      autoImport: true,
    }),
    Fonts({
      google: {
        families: [{
          name: 'Roboto',
          styles: 'wght@100;300;400;500;700;900',
        }],
      },
    }),
  ],
  define: { 'process.env': {} },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
    extensions: [
      '.js',
      '.json',
      '.jsx',
      '.mjs',
      '.ts',
      '.tsx',
      '.vue',
    ],
  },
  server: {
    port: 13124,
    proxy: {
      '/rooms': 'http://0.0.0.0:3001',
      '/ws': {
        target: 'ws://0.0.0.0:3001',
        ws: true,
        rewriteWsOrigin: true,
      },
    },
  },
})
